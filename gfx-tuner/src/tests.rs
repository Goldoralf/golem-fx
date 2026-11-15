// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5), on the review prototype's synthetic notes: partials 1 / k up to 5 kHz,
//! inharmonicity, exponential decay (faster for the upper partials), an optional pitch glide, noise.

use crate::{Tuner, METERS, PARAMS, STATE_SETTLED, STATE_STRUM, TUNINGS, TUNING_NAMES};
use gfx_dsp::lfo::Rng;
use gfx_dsp::zoom::cents;
use gfx_dsp::Effect;
use std::f64::consts::TAU;

const SR: f32 = 48000.0;
/// Notes start here (s), as in the prototype.
const T0: f64 = 0.05;

fn midi_hz(m: f64) -> f64 {
    440.0 * ((m - 69.0) / 12.0).exp2()
}

/// A synthetic plucked string (the prototype's `pluck`).
#[derive(Clone, Copy)]
struct Pluck {
    f0: f64,
    /// inharmonicity
    b: f64,
    /// +¢ at the onset, relaxing with `gtau`
    glide: f64,
    gtau: f64,
    amp: f64,
    /// the fundamental's decay time constant (s; infinite = a steady tone)
    tau1: f64,
    /// onset (s; negative = already ringing)
    t0: f64,
    /// partial k's amplitude 1 / k^rolloff
    rolloff: f64,
    /// fundamental gain (a bridge pickup: 0.05); `even_only` = a 12th-fret harmonic (odd partials
    /// above the fundamental removed; the fundamental keeps `fund`)
    fund: f64,
    even_only: bool,
    seed: u64,
}

impl Pluck {
    fn new(f0: f64) -> Self {
        Self { f0, b: 0.0, glide: 0.0, gtau: 0.15, amp: 0.25, tau1: 2.0, t0: T0, rolloff: 1.0, fund: 1.0, even_only: false, seed: 0 }
    }

    /// The fundamental's true frequency once the glide is over (Hz).
    fn truth(&self) -> f64 {
        self.f0 * (1.0 + self.b).sqrt()
    }

    fn add_to(&self, x: &mut [f32], sr: f64) {
        let mut rng = Rng::new(self.seed.wrapping_mul(7919).wrapping_add(13));
        let mut k = 1;
        while (k as f64) * self.f0 * (1.0 + self.b * (k * k) as f64).sqrt() < 5000.0 && k < 60 {
            let fk = k as f64 * self.f0 * (1.0 + self.b * (k * k) as f64).sqrt();
            let mut ph = TAU * 0.5 * (rng.bipolar() as f64 + 1.0);
            let tau = self.tau1 / (1.0 + 0.25 * (k - 1) as f64);
            let mut a = self.amp / (k as f64).powf(self.rolloff);
            if k == 1 {
                a *= self.fund;
            }
            if self.even_only && k % 2 == 1 && k > 1 {
                a = 0.0;
            }
            for (i, v) in x.iter_mut().enumerate() {
                let t = i as f64 / sr - self.t0;
                if t < 0.0 {
                    continue;
                }
                let g = self.glide * (-t / self.gtau).exp();
                ph += TAU * fk * (g / 1200.0).exp2() / sr;
                if a > 0.0 {
                    *v += (a * (-t / tau).exp() * (1.0 - (-t / 0.002).exp()) * ph.sin()) as f32;
                }
            }
            k += 1;
        }
    }
}

fn render(plucks: &[Pluck], secs: f64, noise_db: f64, seed: u64) -> Vec<f32> {
    let n = (secs * SR as f64) as usize;
    let mut x = vec![0.0f32; n];
    for p in plucks {
        p.add_to(&mut x, SR as f64);
    }
    let g = 10f64.powf(noise_db / 20.0) as f32 * 1.732;
    let mut rng = Rng::new(seed + 3);
    for v in x.iter_mut() {
        *v += g * rng.bipolar();
    }
    x
}

fn tuner(params: &[(&str, f32)], sr: f32) -> Tuner {
    let mut t = Tuner::new();
    for (k, v) in params {
        t.set_param(k, *v);
    }
    t.prepare(sr, 2048, 1);
    t.reset();
    t
}

/// One hop's view: the meters, and the strobe's own (f64) reading.
#[derive(Clone, Copy)]
struct Hop {
    t: f64,
    m: [f32; 23],
    fine: Option<(f64, f64)>,
}

impl Hop {
    fn get(&self, id: &str) -> f32 {
        self.m[METERS.iter().position(|m| *m == id).unwrap()]
    }
}

/// Runs a mono signal hop by hop (blocks of one hop).
fn trace(t: &mut Tuner, x: &[f32], sr: f32) -> Vec<Hop> {
    let hop = (0.01 * sr).round() as usize;
    let mut out = Vec::new();
    let mut buf = x.to_vec();
    for (k, c) in buf.chunks_mut(hop).enumerate() {
        t.process(&mut [c]);
        out.push(Hop { t: (k + 1) as f64 * hop as f64 / sr as f64 - T0, m: *t.meters(), fine: t.fine() });
    }
    out
}

/// Timing and accuracy of one mono reading against `truth`.
#[derive(Debug)]
struct Reading {
    coarse: f64,
    first_fine: f64,
    settled: f64,
    worst_settled: f64,
    late_max: f64,
    late_sigma_shown: f64,
}

fn read(h: &[Hop], truth: f64) -> Reading {
    let nan = f64::NAN;
    let coarse = h.iter().find(|p| p.get("state") >= 1.0 && cents(p.get("freq") as f64 / truth).abs() <= 5.0).map_or(nan, |p| p.t);
    let first_fine = h.iter().find(|p| p.fine.is_some()).map_or(nan, |p| p.t);
    let settled = h.iter().find(|p| p.get("state") == STATE_SETTLED).map_or(nan, |p| p.t);
    let worst_settled = h
        .iter()
        .filter(|p| p.get("state") == STATE_SETTLED)
        .filter_map(|p| p.fine.map(|f| cents(f.0 / truth).abs()))
        .fold(0.0, f64::max);
    let late: Vec<f64> = h.iter().filter(|p| p.t > 1.8 && p.t < 2.8).filter_map(|p| p.fine.map(|f| cents(f.0 / truth))).collect();
    let late_max = late.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let shown: Vec<f64> = h.iter().filter(|p| p.t > 1.8 && p.t < 2.8).map(|p| p.get("cents") as f64).collect();
    let mean = shown.iter().sum::<f64>() / shown.len().max(1) as f64;
    let late_sigma_shown = (shown.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / shown.len().max(1) as f64).sqrt();
    Reading { coarse, first_fine, settled, worst_settled, late_max, late_sigma_shown }
}

const NOTES: [(&str, f64); 7] = [("A0", 27.5), ("B0", 30.868), ("E1", 41.203), ("E2", 82.407), ("D3", 146.83), ("E4", 329.63), ("E6", 1318.5)];

/// The README §2.4 table, as coded (cargo test -p gfx-tuner --release -- --ignored survey --nocapture).
#[test]
#[ignore]
fn survey() {
    for (prec, name) in [(0.0, "Fast"), (1.0, "Strobe")] {
        println!("== {name}");
        for (nm, f) in NOTES {
            for (case, p) in [
                ("clean -37c", Pluck { f0: f * (-37.0f64 / 1200.0).exp2(), ..Pluck::new(f) }),
                ("glide8 B2e-4", Pluck { f0: f * (3.0f64 / 1200.0).exp2(), b: 2e-4, glide: 8.0, ..Pluck::new(f) }),
                ("glide20 t.25", Pluck { f0: f * (3.0f64 / 1200.0).exp2(), glide: 20.0, gtau: 0.25, ..Pluck::new(f) }),
            ] {
                let x = render(&[p], 3.2, -60.0, 1);
                let mut t = tuner(&[("precision", prec)], SR);
                let r = read(&trace(&mut t, &x, SR), p.truth());
                let c = t.counters;
                println!(
                    "  {nm} {case:14} {c:?}\n     coarse {:.3}s first {:.2}s settled {:.2}s worst settled {:.3}c late max {:.3}c shown σ {:.3}c",
                    r.coarse, r.first_fine, r.settled, r.worst_settled, r.late_max, r.late_sigma_shown
                );
            }
        }
    }
}


fn off(f: f64, c: f64) -> f64 {
    f * (c / 1200.0).exp2()
}

/// Renders plucks (noise at −60 dBFS) and traces them through a tuner with `params`.
fn run(params: &[(&str, f32)], plucks: &[Pluck], secs: f64) -> (Vec<Hop>, Tuner) {
    let x = render(plucks, secs, -60.0, 1);
    let mut t = tuner(params, SR);
    let h = trace(&mut t, &x, SR);
    (h, t)
}

/// The tuning with this option name.
fn tuning_rank(name: &str) -> f32 {
    TUNING_NAMES.iter().position(|n| *n == name).unwrap() as f32
}

#[test]
fn params_and_tables() {
    assert_eq!(PARAMS.len(), crate::NP);
    assert_eq!(TUNING_NAMES.len(), 22);
    assert_eq!(TUNINGS.len() + 2, TUNING_NAMES.len());
    assert_eq!(METERS.len(), 23);
    let mut ids: Vec<&str> = PARAMS.iter().map(|p| p.id).chain(METERS.iter().copied()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), crate::NP + 23, "ids are unique");
    for p in PARAMS.iter() {
        assert!(p.min <= p.default && p.default <= p.max, "{}", p.id);
    }
}

/// README §5 "Tuning table": the embedded tunings equal golem's file when it is present.
#[test]
fn tuning_table_matches_golem() {
    let path = std::env::var("HOME").map(|h| format!("{h}/golem/library/catalog/tunings.json")).unwrap_or_default();
    let Ok(text) = std::fs::read_to_string(&path) else {
        println!("tunings.json not found: skipped");
        return;
    };
    // a small reader for this file's shape: "id": "…", then "midi": [ … ]
    let mut found = 0;
    let mut rest = text.as_str();
    while let Some(i) = rest.find("\"id\": \"") {
        rest = &rest[i + 7..];
        let id = &rest[..rest.find('"').unwrap()];
        let m = rest.find("\"midi\": [").unwrap();
        let body = &rest[m + 9..];
        let notes: Vec<u8> = body[..body.find(']').unwrap()].split(',').map(|v| v.trim().parse().unwrap()).collect();
        let rank = TUNING_NAMES.iter().position(|n| *n == id).unwrap_or_else(|| panic!("{id} is not a named option"));
        assert_eq!(TUNINGS[rank - 2], &notes[..], "{id}");
        found += 1;
    }
    assert_eq!(found, 20, "golem's file has new tunings: append them");
}

/// README §5 "Tuning table": a named option and Custom with the same notes give identical meters.
#[test]
fn named_and_custom_are_identical() {
    let p = Pluck::new(off(midi_hz(50.0), 7.0));
    let x = render(&[p], 0.8, -60.0, 4);
    let mut a = tuner(&[("tuning", tuning_rank("dadgad"))], SR);
    let mut custom = vec![("tuning", 1.0)];
    let ids = ["string_1", "string_2", "string_3", "string_4", "string_5", "string_6", "string_7", "string_8"];
    for (k, id) in ids.iter().enumerate() {
        custom.push((id, TUNINGS[4].get(k).copied().unwrap_or(0) as f32));
    }
    let mut b = tuner(&custom, SR);
    let ha = trace(&mut a, &x, SR);
    let hb = trace(&mut b, &x, SR);
    assert!(ha.iter().zip(&hb).all(|(p, q)| p.m == q.m));
    assert!(ha.iter().any(|h| h.get("string") == 3.0), "D3 is string 3 of DADGAD");
}

/// README §5 "Accuracy, Fast": every string of every tuning (offsets 0, ±3, ±20, ±49, −80, −100 ¢ in
/// turn) and chromatic A0 – F6: after `state` 2, |error| ≤ 0.3 ¢.
#[test]
fn accuracy_fast() {
    let offs = [0.0, 3.0, -3.0, 20.0, -20.0, 49.0, -49.0, -80.0, -100.0];
    let mut worst = (0.0f64, String::new());
    let mut late_settle = (0.0f64, String::new());
    let mut cases: Vec<(f32, f64, f64)> = Vec::new();
    for (t, notes) in TUNINGS.iter().enumerate() {
        for (i, &m) in notes.iter().enumerate() {
            cases.push(((t + 2) as f32, m as f64, offs[(i + 2 * t) % offs.len()]));
        }
    }
    for m in (21..=89).step_by(4) {
        cases.push((0.0, m as f64, offs[(m as usize) % 7]));
    }
    for (tuning, m, c) in cases {
        let p = Pluck::new(off(midi_hz(m), c));
        let (h, _) = run(&[("tuning", tuning)], &[p], 1.4);
        let r = read(&h, p.truth());
        let what = format!("tuning {tuning} midi {m} {c:+} ¢");
        assert!(r.settled.is_finite(), "{what}: never settled");
        if r.worst_settled > worst.0 {
            worst = (r.worst_settled, what.clone());
        }
        if r.settled > late_settle.0 {
            late_settle = (r.settled, what);
        }
    }
    println!("accuracy Fast: worst |error| while settled {:.3} ¢ ({}); slowest settle {:.2} s ({})", worst.0, worst.1, late_settle.0, late_settle.1);
    assert!(worst.0 <= 0.3);
}

/// README §5 "Accuracy, Strobe": |error| ≤ 0.1 ¢ and the shown cents' σ ≤ 0.03 ¢ over 2 s, settled
/// within 1.2 s, A0 and B0 included.
#[test]
fn accuracy_strobe() {
    let mut worst = (0.0f64, 0.0f64, 0.0f64);
    for (k, (nm, f)) in NOTES.iter().enumerate() {
        for c in [0.0, 20.0, -49.0] {
            let p = Pluck { tau1: 4.0, ..Pluck::new(off(*f, c + k as f64)) };
            let (h, _) = run(&[("precision", 1.0)], &[p], 3.0);
            let r = read(&h, p.truth());
            assert!(r.settled <= 1.2, "{nm} {c:+}: settled at {:.2} s", r.settled);
            assert!(r.worst_settled <= 0.1 && r.late_max <= 0.1, "{nm} {c:+}: {r:?}");
            assert!(r.late_sigma_shown <= 0.03, "{nm} {c:+}: shown σ {:.3}", r.late_sigma_shown);
            worst = (worst.0.max(r.worst_settled), worst.1.max(r.late_sigma_shown), worst.2.max(r.settled));
        }
    }
    println!("accuracy Strobe: worst |error| settled {:.3} ¢, shown σ {:.3} ¢, slowest settle {:.2} s", worst.0, worst.1, worst.2);
}

/// README §5 "Reference": a tone at the reference reads 0.00 ± 0.05 ¢ (415 / 432 / 466 Hz), and every
/// sample rate agrees within ± 0.05 ¢.
#[test]
fn reference_and_sample_rates() {
    for r in [415.0f32, 432.0, 466.0] {
        for sr in [44100.0f32, 48000.0, 96000.0, 192000.0] {
            let p = Pluck { tau1: 4.0, ..Pluck::new(r as f64) };
            let n = (1.5 * sr as f64) as usize;
            let mut x = vec![0.0f32; n];
            p.add_to(&mut x, sr as f64);
            let mut t = tuner(&[("reference", r)], sr);
            let h = trace(&mut t, &x, sr);
            let last = h.last().unwrap();
            assert_eq!(last.get("note"), 69.0);
            let c = last.get("cents");
            assert!(c.abs() <= 0.05, "A = {r} at {sr}: {c:+.3} ¢");
        }
    }
}

/// README §5 "Speed, coarse" and "Speed, strobe".
#[test]
fn speed() {
    let coarse = [("E4", 64.0, 0.035), ("E2", 40.0, 0.045), ("B1", 35.0, 0.050), ("B0", 23.0, 0.095)];
    for (nm, m, bound) in coarse {
        let p = Pluck::new(midi_hz(m));
        let (h, _) = run(&[], &[p], 0.4);
        let r = read(&h, p.truth());
        println!("{nm}: coarse within ±5 ¢ at {:.0} ms", 1e3 * r.coarse);
        assert!(r.coarse <= bound + 1e-9, "{nm}: coarse at {:.3} s", r.coarse);
    }
    // first strobe reading; state 2 on a clean note in Fast and Strobe
    let rows = [("E4", 64.0, 0.08, 0.25, 0.40), ("E2", 40.0, 0.18, 0.35, 0.50), ("B0", 23.0, 0.45, 0.80, 1.10)];
    for (nm, m, first, fast, strobe) in rows {
        let p = Pluck::new(midi_hz(m));
        let (h, _) = run(&[], &[p], 1.5);
        let r = read(&h, p.truth());
        let (hs, _) = run(&[("precision", 1.0)], &[p], 1.5);
        let rs = read(&hs, p.truth());
        println!("{nm}: first strobe {:.2} s, settled Fast {:.2} s, Strobe {:.2} s", r.first_fine, r.settled, rs.settled);
        assert!(r.first_fine <= first + 1e-9 && r.settled <= fast + 1e-9 && rs.settled <= strobe + 1e-9, "{nm}");
    }
}

/// README §5 "Open string or harmonic" and "12th-fret harmonic".
#[test]
fn open_string_and_harmonic() {
    let e2 = midi_hz(40.0);
    // a bridge pickup: the fundamental 20 dB under its 2nd partial
    let bridge = Pluck { fund: 0.05, ..Pluck::new(e2) };
    let (h, t) = run(&[("tuning", 2.0)], &[bridge], 1.0);
    let last = h.last().unwrap();
    println!("bridge-pickup E2: note {}, {:?}", last.get("note"), t.counters);
    assert_eq!((last.get("note"), last.get("string")), (40.0, 1.0), "{:?}", t.counters);
    // the 12th-fret harmonic: even partials only, B = 2e-4
    let harm = Pluck { even_only: true, fund: 0.0, b: 2e-4, ..Pluck::new(e2) };
    let (h, t) = run(&[("tuning", 2.0)], &[harm], 1.5);
    let last = h.last().unwrap();
    assert_eq!((last.get("note"), last.get("string")), (52.0, 1.0), "{:?}", t.counters);
    // it reads sharp of the open string's fundamental by the inharmonicity: +0.52 ¢ (not corrected)
    let rel = last.get("cents") as f64 - cents(harm.truth() / e2);
    println!("12th-fret harmonic, B = 2e-4: +{rel:.3} ¢ over the fundamental");
    assert!((rel - 0.52).abs() <= 0.1, "{rel}");
    // the coarse stage reads 2 f when the odd partials are gone: the probe at f decides by the
    // fundamental's presence (≥ 20 dB under the 2nd partial: the harmonic)
    for (fund, open) in [(0.07f64, true), (0.018, false)] {
        let p = Pluck { even_only: true, fund, ..Pluck::new(e2) };
        let (h, t) = run(&[("tuning", 2.0)], &[p], 1.0);
        let last = h.last().unwrap();
        println!("even partials, fundamental {:.0} dB re the 2nd: note {}, {:?}", 20.0 * (2.0 * fund).log10(), last.get("note"), t.counters);
        assert_eq!(last.get("note"), if open { 40.0 } else { 52.0 });
        assert_eq!(t.counters.octave_fixes, open as u32);
        if open {
            assert!(last.fine.is_some_and(|f| cents(f.0 / p.truth()).abs() < 0.1), "the strobe moved down to f");
        }
    }
}

/// README §5 "Coincidence (DADGAD)": D2's harmonic is attributed to the D3 string.
#[test]
fn coincidence_fundamental_wins() {
    let mut t = tuner(&[("tuning", tuning_rank("dadgad"))], SR);
    t.process(&mut [&mut [0.0f32; 480][..]]);
    let a = t.attribute(2.0 * midi_hz(38.0)).unwrap();
    assert_eq!((a.string, a.harmonic, a.note), (3, false, 50));
    // the 6th string's own harmonic in standard tuning: no string at 2 × E2 (E3)
    let mut t = tuner(&[("tuning", 2.0)], SR);
    t.process(&mut [&mut [0.0f32; 480][..]]);
    let a = t.attribute(2.0 * midi_hz(40.0) * (0.3f64 / 1200.0).exp2()).unwrap();
    assert_eq!((a.string, a.harmonic, a.note), (1, true, 52));
    // more than 100 ¢ from every string or 2×: chromatic
    let a = t.attribute(midi_hz(42.0));
    assert!(a.is_none());
}

/// README §5 "Needle jitter": steady tone under −60 dBFS noise.
#[test]
fn needle_jitter() {
    for (prec, bound) in [(0.0f32, 0.1f64), (1.0, 0.02)] {
        let p = Pluck { tau1: f64::INFINITY, amp: 0.1, ..Pluck::new(off(midi_hz(45.0), 6.0)) };
        let (h, _) = run(&[("precision", prec)], &[p], 3.0);
        let after: Vec<f64> = h.iter().filter(|q| q.t > 1.5).map(|q| q.get("cents") as f64).collect();
        let mean = after.iter().sum::<f64>() / after.len() as f64;
        let sd = (after.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / after.len() as f64).sqrt();
        let step = after.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f64::max);
        println!("jitter {}: σ {sd:.4} ¢, largest step {step:.3} ¢", if prec > 0.5 { "Strobe" } else { "Fast" });
        assert!(sd <= bound && step <= 0.5);
        assert!(h.iter().filter(|q| q.t > 1.5).all(|q| q.get("state") == STATE_SETTLED));
    }
}

/// README §5 "Error bar": σ within a factor 3 of the readings' measured spread on clean notes; ≥ 0.5 ¢
/// with a neighbour 200 ¢ away at 0 dB.
#[test]
fn error_bar() {
    // clean: a steady tone under noise at −40 dBFS (enough spread to measure)
    let p = Pluck { tau1: f64::INFINITY, amp: 0.05, ..Pluck::new(midi_hz(40.0)) };
    let x = render(&[p], 3.0, -40.0, 9);
    let mut t = tuner(&[], SR);
    let h = trace(&mut t, &x, SR);
    let fine: Vec<(f64, f64)> = h.iter().filter(|q| q.t > 1.0).filter_map(|q| q.fine).collect();
    let c: Vec<f64> = fine.iter().map(|f| cents(f.0 / p.truth())).collect();
    let mean = c.iter().sum::<f64>() / c.len() as f64;
    let sd = (c.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / c.len() as f64).sqrt();
    let sig = fine.iter().map(|f| f.1).sum::<f64>() / fine.len() as f64;
    println!("error bar, clean: σ {sig:.4} ¢ against a measured spread of {sd:.4} ¢");
    assert!(sig <= 3.0 * sd && sig >= sd / 3.0);
    // a neighbour 200 ¢ below at 0 dB (Fast): σ rises
    let a2 = midi_hz(45.0);
    let n = Pluck { t0: -0.3, seed: 7, ..Pluck::new(off(a2, -200.0)) };
    let (h, _) = run(&[], &[Pluck::new(a2), n], 2.5);
    let sig = h.iter().filter(|q| q.t > 0.8 && q.t < 2.0).filter_map(|q| q.fine.map(|f| f.1)).fold(f64::MAX, f64::min);
    println!("error bar, neighbour 200 ¢ at 0 dB: σ ≥ {sig:.2} ¢");
    assert!(sig >= 0.5);
}

/// The swing of the fine reading over a span, against the truth.
fn swing(h: &[Hop], truth: f64, from: f64, to: f64) -> f64 {
    h.iter().filter(|q| q.t > from && q.t < to).filter_map(|q| q.fine.map(|f| cents(f.0 / truth).abs())).fold(0.0, f64::max)
}

/// README §5 "Neighbour strings".
#[test]
fn neighbour_strings() {
    let (d3, e2, a2) = (midi_hz(50.0), midi_hz(40.0), midi_hz(45.0));
    let ring = |f: f64, db: f64| Pluck { t0: -0.3, seed: 7, amp: 0.25 * 10f64.powf(db / 20.0), ..Pluck::new(f) };
    // D3 with a whole E2 ringing 10 dB under it
    let note = Pluck::new(d3);
    let (h, _) = run(&[], &[note, ring(e2, -10.0)], 2.5);
    let fast = swing(&h, note.truth(), 0.6, 2.4);
    let (h, _) = run(&[("precision", 1.0)], &[note, ring(e2, -10.0)], 2.5);
    let strobe = swing(&h, note.truth(), 0.6, 2.4);
    let st = read(&h, note.truth()).settled;
    println!("D3 + E2 at −10 dB: Fast swing {fast:.3} ¢, Strobe {strobe:.3} ¢ settled at {st:.2} s");
    assert!(fast <= 0.4 && strobe <= 0.1 && st <= 0.7);
    // a single partial 200 ¢ below
    // (it decays with the note, so the level ratio holds)
    let single = |db: f64| Pluck { rolloff: 60.0, t0: T0, ..ring(off(a2, -200.0), db) };
    // at −20 dB: ≤ 0.2 ¢ and settled in both
    for prec in [0.0f32, 1.0] {
        let p = Pluck::new(a2);
        let (h, _) = run(&[("precision", prec)], &[p, single(-20.0)], 2.5);
        let s = swing(&h, p.truth(), 1.0, 2.4);
        let r = read(&h, p.truth());
        println!("A2 + a partial 200 ¢ below at −20 dB, precision {prec}: {s:.3} ¢, settled {:.2} s", r.settled);
        assert!(s <= 0.2 && r.settled.is_finite());
    }
    // at 0 dB, Strobe: ≤ 0.2 ¢ and never called settled
    let p = Pluck::new(a2);
    let (h, _) = run(&[("precision", 1.0)], &[p, ring(off(a2, -200.0), 0.0)], 3.0);
    let s = swing(&h, p.truth(), 1.8, 2.8);
    let ever = h.iter().any(|q| q.get("state") == STATE_SETTLED);
    println!("A2 + a string 200 ¢ below at 0 dB, Strobe: {s:.3} ¢, settled ever: {ever}");
    assert!(s <= 0.2 && !ever);
    // G2 with the low B ringing 10 dB under (its 3rd partial 98 ¢ away), Strobe
    let g2 = Pluck::new(midi_hz(43.0));
    let (h, _) = run(&[("precision", 1.0)], &[g2, ring(midi_hz(23.0), -10.0)], 2.5);
    let s = swing(&h, g2.truth(), 1.2, 2.4);
    println!("G2 + B0 at −10 dB, Strobe: {s:.3} ¢");
    assert!(s <= 0.1);
}

/// README §5 "Capture guard": A2 read while E2 rings 10 dB louder, the track forced on A2.
#[test]
fn capture_guard() {
    let fd = 12000.0;
    let mut x = vec![0.0f32; (2.5 * fd) as usize];
    let a2 = Pluck::new(midi_hz(45.0));
    a2.add_to(&mut x, fd);
    Pluck { t0: -0.3, amp: 0.25 * 10f64.powf(0.5), seed: 7, ..Pluck::new(midi_hz(40.0)) }.add_to(&mut x, fd);
    let mut z = gfx_dsp::zoom::Zoom::new();
    z.prepare(fd as f32);
    let coarse = a2.truth() * (4.0f64 / 1200.0).exp2();
    z.start(coarse);
    let f = coarse;
    let tmax = (48.0 / f).clamp(0.3, 1.2);
    let hold = (3.0 / f).max(0.025) + 2.0 / f;
    let (mut worst_guard, mut worst_late, mut held) = (0.0f64, 0.0f64, 0u32);
    for (i, v) in x.iter().enumerate() {
        z.push(*v);
        if i % 120 == 119 {
            let avail = z.elapsed() - T0 - hold;
            let t = avail.min(tmax);
            if t >= 8.0 / f {
                let from = ((T0 + hold) * z.fb()).ceil() as u64;
                z.set_capture(coarse * (-35.0f64 / 1200.0).exp2(), coarse * (35.0f64 / 1200.0).exp2());
                if let Some(e) = z.estimate(t, from) {
                    held += e.clamped as u32;
                    worst_guard = worst_guard.max(cents(e.hz / coarse).abs());
                    if z.elapsed() > 1.4 {
                        worst_late = worst_late.max(cents(e.hz / a2.truth()).abs());
                    }
                }
            }
        }
    }
    println!("capture guard: within {worst_guard:.2} ¢ of the coarse value ({held} hops held at the edge); Strobe {worst_late:.3} ¢");
    assert!(worst_guard <= 35.0 + 1e-9 && worst_late <= 0.1);
}

/// README §5 "Glide": `state` stays 1 while the pitch moves; error at the first `state` 2.
#[test]
fn glide() {
    for (prec, bound) in [(0.0f32, 0.3f64), (1.0, 0.1)] {
        for (g, tau) in [(8.0, 0.15), (20.0, 0.25)] {
            for m in [28.0, 40.0, 64.0] {
                let p = Pluck { glide: g, gtau: tau, ..Pluck::new(midi_hz(m)) };
                let (h, _) = run(&[("precision", prec)], &[p], 3.0);
                let first = h.iter().find(|q| q.get("state") == STATE_SETTLED).expect("settles");
                let e = cents(first.fine.unwrap().0 / p.truth()).abs();
                let last = h.iter().rev().find(|q| q.fine.is_some()).unwrap();
                let nominal = cents(last.fine.unwrap().0 / p.truth()).abs();
                assert!(e <= bound, "precision {prec} glide {g} midi {m}: {e:.3} ¢ at the first settled hop");
                assert!(nominal <= 0.1, "precision {prec} glide {g} midi {m}: ends {nominal:.3} ¢ off");
            }
        }
    }
}

/// A steady harmonic tone whose pitch slides in cents (a peg being turned).
fn slide(f: f64, from_c: f64, to_c: f64, t_slide: (f64, f64), secs: f64) -> Vec<f32> {
    let n = (secs * SR as f64) as usize;
    let mut ph = [0.0f64; 12];
    (0..n)
        .map(|i| {
            let t = i as f64 / SR as f64;
            if t < T0 {
                return 0.0;
            }
            let u = ((t - t_slide.0) / (t_slide.1 - t_slide.0)).clamp(0.0, 1.0);
            let fk = f * ((from_c + (to_c - from_c) * u) / 1200.0).exp2();
            let mut s = 0.0;
            for (k, p) in ph.iter_mut().enumerate() {
                *p += TAU * fk * (k + 1) as f64 / SR as f64;
                s += p.sin() / (k + 1) as f64;
            }
            (0.1 * s) as f32
        })
        .collect()
}

/// README §5 "Peg turn": E2 sliding from −80 ¢ to 0 in 2 s, Fast.
#[test]
fn peg_turn() {
    let e2 = midi_hz(40.0);
    let (a, b) = (0.6, 2.6);
    let x = slide(e2, -80.0, 0.0, (a, b), 3.6);
    let split = (a * SR as f64) as usize;
    let mut t = tuner(&[("tuning", 2.0)], SR);
    let h0 = trace(&mut t, &x[..split], SR);
    assert!(h0.iter().any(|q| q.fine.is_some()));
    let starts0 = t.counters.starts;
    let h = trace(&mut t, &x[split..], SR);
    let mut lag = 0.0f64;
    let mut gap = false;
    for (k, q) in h.iter().enumerate() {
        let now = a + (k + 1) as f64 * 0.01;
        let truth_c = -80.0 * (1.0 - ((now - a) / (b - a)).clamp(0.0, 1.0));
        match q.fine {
            Some(f) => lag = lag.max((cents(f.0 / e2) - truth_c).abs()),
            None => gap = true,
        }
    }
    let stop = h
        .iter()
        .enumerate()
        .find(|(k, q)| a + (*k + 1) as f64 * 0.01 > b && q.get("state") == STATE_SETTLED)
        .map(|(k, _)| a + (k + 1) as f64 * 0.01 - b);
    println!("peg turn: behind by ≤ {lag:.2} ¢, gap {gap}, restarts {}, settled {:?} s after the stop", t.counters.starts - starts0, stop);
    assert!(lag <= 8.0 && !gap && t.counters.starts == starts0);
    assert!(stop.is_some_and(|s| s <= 0.45));
}

/// A strum: the tuning's strings at random offsets (±30 ¢), 20 ms spread, B = 1e-4.
fn strum(notes: &[u8], seed: u64, secs: f64) -> (Vec<f32>, Vec<f64>) {
    let mut rng = Rng::new(100 + seed);
    let n = notes.len();
    let mut ps = Vec::new();
    let mut offs = Vec::new();
    for (i, &m) in notes.iter().enumerate() {
        let c = 30.0 * rng.bipolar() as f64;
        offs.push(c);
        ps.push(Pluck {
            b: 1e-4,
            amp: 0.12,
            t0: T0 + 0.02 * i as f64 / (n - 1) as f64,
            seed: seed * 10 + i as u64,
            ..Pluck::new(off(midi_hz(m as f64), c))
        });
    }
    (render(&ps, secs, -60.0, seed), offs)
}

const S_CENTS: [&str; 8] = ["s1_cents", "s2_cents", "s3_cents", "s4_cents", "s5_cents", "s6_cents", "s7_cents", "s8_cents"];
const S_CONF: [&str; 8] = ["s1_conf", "s2_conf", "s3_conf", "s4_conf", "s5_conf", "s6_conf", "s7_conf", "s8_conf"];

/// README §5 "Poly": strums of every tuning.
#[test]
fn poly_strums() {
    let mut report = Vec::new();
    for (k, notes) in TUNINGS.iter().enumerate() {
        let name = TUNING_NAMES[k + 2];
        let rank = (k + 2) as f32;
        let table = {
            let mut t = tuner(&[("tuning", rank)], SR);
            t.process(&mut [&mut [0.0f32; 480][..]]);
            t.string_table()
        };
        let (mut worst_clean, mut worst_shared, mut t3) = (0.0f64, 0.0f64, 0.0f64);
        let mut per = [0.0f64; 8];
        for seed in 0..3u64 {
            let (x, offs) = strum(notes, seed, 1.6);
            let mut t = tuner(&[("tuning", rank)], SR);
            let h = trace(&mut t, &x, SR);
            let first = h.iter().find(|q| q.get("state") == STATE_STRUM).unwrap_or_else(|| panic!("{name}: no strum read"));
            t3 = t3.max(first.t);
            for (i, &(_, clean, ti)) in table.iter().enumerate() {
                // (the shared strings' grid searches follow the clean ones', two per hop)
                let at = h.iter().find(|q| q.t >= (ti + 0.1).max(first.t + 0.05) && q.get("state") == STATE_STRUM).unwrap();
                // the fundamental of a stiff string: + B / 2 in ratio
                let truth = cents((1.0f64 + 1e-4).sqrt()) + offs[i];
                let shown = at.get(S_CENTS[i]) as f64;
                let err = shown - truth;
                if clean {
                    assert!(at.get(S_CONF[i]) > 0.5, "{name} string {}: conf {}", i + 1, at.get(S_CONF[i]));
                    worst_clean = worst_clean.max(err.abs());
                    per[i] = per[i].max(err.abs());
                } else {
                    // shared strings: flagged unless their peak is clearly their own, sign right when > 10 ¢ off
                    assert!(at.get(S_CONF[i]) <= 0.5 || err.abs() < 1.0, "{name} string {} shared: conf {}, {err:+.2} ¢", i + 1, at.get(S_CONF[i]));
                    if offs[i].abs() > 10.0 {
                        assert!(shown * offs[i] > 0.0, "{name} string {}: sign ({shown:+.1} for {:+.1})", i + 1, offs[i]);
                    }
                    worst_shared = worst_shared.max(err.abs());
                }
            }
        }
        report.push((
            worst_clean,
            format!("{name:14} strum shown at {t3:.2} s; clean ≤ {worst_clean:.2} ¢ {:.2?}, shared ≤ {worst_shared:.1} ¢", &per[..notes.len()]),
        ));
    }
    for r in &report {
        println!("{}", r.1);
    }
    assert!(report.iter().all(|r| r.0 <= 1.0));
}

/// README §5 "Poly, false strum", and a single pluck after a strum returns to Mono.
#[test]
fn poly_false_strum_and_back_to_mono() {
    for (k, notes) in TUNINGS.iter().enumerate() {
        let p = Pluck { b: 1e-4, ..Pluck::new(midi_hz(notes[0] as f64)) };
        let (h, _) = run(&[("tuning", (k + 2) as f32)], &[p], 1.5);
        assert!(h.iter().all(|q| q.get("state") != STATE_STRUM), "{}: a lone low string called a strum", TUNING_NAMES[k + 2]);
    }
    let (mut x, _) = strum(TUNINGS[0], 5, 1.5);
    let mut y = vec![0.0f32; (1.5 * SR) as usize];
    Pluck { amp: 0.6, ..Pluck::new(midi_hz(45.0)) }.add_to(&mut y, SR as f64);
    x.extend(y);
    let mut t = tuner(&[("tuning", 2.0)], SR);
    let h = trace(&mut t, &x, SR);
    assert!(h.iter().any(|q| q.t < 1.4 && q.get("state") == STATE_STRUM));
    let end = h.last().unwrap();
    assert!(end.get("state") == STATE_SETTLED || end.get("state") == 1.0, "state {}", end.get("state"));
    assert_eq!(end.get("string"), 2.0);
}

/// README §5 "Mute": the ramps leave no burst; unmuted and bypassed, the output is the input bit for bit.
#[test]
fn mute_and_bypass() {
    let n = SR as usize;
    let sine: Vec<f32> = (0..n).map(|i| 0.25 * (TAU as f32 * 1000.0 * i as f32 / SR).sin()).collect();
    type Set<'a> = &'a [(&'a str, f32)];
    let cases: [(Set, Set); 3] = [(&[], &[("mute", 1.0)]), (&[("mute", 1.0)], &[("mute", 0.0)]), (&[("mute", 1.0)], &[("bypass", 1.0)])];
    for (pre, toggles) in cases {
        let mut t = tuner(pre, SR);
        let mut y = sine.clone();
        let (a, b) = y.split_at_mut(n / 2);
        t.process(&mut [a]);
        for (id, v) in toggles {
            t.set_param(id, *v);
        }
        t.process(&mut [b]);
        // HF (above 10 kHz) against the input's peak
        let mut hp = gfx_dsp::svf::Svf::new();
        hp.set(gfx_dsp::svf::SvfMode::HighPass, 10000.0, 0.7, 0.0, SR);
        let mut hp2 = hp.clone();
        let hf = y.iter().map(|&v| hp2.process(hp.process(v)).abs()).skip(n / 4).fold(0.0f32, f32::max);
        let rel = 20.0 * (hf / 0.25).max(1e-12).log10();
        println!("{toggles:?}: HF {rel:.1} dB re peak");
        assert!(rel <= -50.0, "{toggles:?}: {rel:.1} dB");
    }
    let x = render(&[Pluck::new(110.0)], 1.0, -20.0, 3);
    let cases: [&[(&str, f32)]; 4] = [&[], &[("bypass", 1.0)], &[("bypass", 1.0), ("always_on", 1.0)], &[("bypass", 1.0), ("mute", 1.0)]];
    for params in cases {
        let mut t = tuner(params, SR);
        let mut y = x.clone();
        for c in y.chunks_mut(256) {
            t.process(&mut [c]);
        }
        assert!(y == x, "{params:?}");
    }
    // muted and engaged: silent once the ramp is over
    let mut t = tuner(&[("mute", 1.0)], SR);
    let mut y = x.clone();
    t.process(&mut [&mut y[..]]);
    assert!(y.iter().all(|v| *v == 0.0));
}

/// README §5 "Freshness": `seq` + 1 per 10 ms at any block size and rate, in silence too; frozen when
/// bypassed without `always_on`.
#[test]
fn freshness() {
    for sr in [44100.0f32, 48000.0, 96000.0, 192000.0] {
        for block in [32usize, 333, 2048] {
            let mut t = tuner(&[], sr);
            let mut x = vec![0.0f32; sr as usize];
            for c in x.chunks_mut(block) {
                t.process(&mut [c]);
            }
            assert_eq!(t.meter("seq"), Some(100.0), "{sr} Hz, blocks of {block}");
        }
    }
    let mut t = tuner(&[("bypass", 1.0)], SR);
    t.process(&mut [&mut vec![0.0f32; 48000][..]]);
    assert_eq!(t.meter("seq"), Some(0.0));
    t.set_param("always_on", 1.0);
    t.process(&mut [&mut vec![0.0f32; 48000][..]]);
    assert_eq!(t.meter("seq"), Some(100.0));
}

/// The meters do not depend on the host's block size.
#[test]
fn block_size_independent() {
    let (x, _) = strum(TUNINGS[0], 1, 1.0);
    let run_b = |b: usize| {
        let mut t = tuner(&[("tuning", 2.0)], SR);
        let mut y = x.clone();
        let mut ms: Vec<[f32; 23]> = Vec::new();
        for c in y.chunks_mut(b) {
            t.process(&mut [c]);
            ms.push(*t.meters());
        }
        ms
    };
    let a = run_b(16);
    let b = run_b(480);
    let at = |v: &Vec<[f32; 23]>, s: f32| v.iter().rev().find(|m| m[6] == s).copied();
    for s in [20.0f32, 50.0, 90.0] {
        assert_eq!(at(&a, s), at(&b, s), "seq {s}");
    }
}

/// CPU (cargo test -p gfx-tuner --release -- --ignored cpu --nocapture).
#[test]
#[ignore]
fn cpu() {
    use std::time::Instant;
    let (x, _) = strum(TUNINGS[9], 2, 10.0);
    let cases: [(&str, &[(&str, f32)]); 4] = [
        ("bypassed", &[("bypass", 1.0)]),
        ("chromatic (Mono)", &[]),
        ("8-std, Mono", &[("tuning", 11.0), ("mode", 1.0)]),
        ("8-std, Auto (8 poly tracks)", &[("tuning", 11.0)]),
    ];
    for (name, params) in cases {
        let mut t = tuner(params, SR);
        let mut y = x.clone();
        let mut worst = 0.0f64;
        let start = Instant::now();
        for c in y.chunks_mut(128) {
            let b = Instant::now();
            t.process(&mut [c]);
            worst = worst.max(b.elapsed().as_secs_f64());
        }
        let ns = start.elapsed().as_secs_f64() * 1e9 / x.len() as f64;
        println!("{name}: {ns:.1} ns / sample ({:.2} % of a core at 48 kHz), worst block {:.3} ms", ns * 48000.0 * 1e-7, worst * 1e3);
    }
}

/// README §5 "Octave errors" on synthetic notes (the DI rows need Raph's recordings): low strings with
/// a weak fundamental (bridge pickups) keep their note and string, with no wrong note flashed.
#[test]
fn weak_fundamentals_keep_their_string() {
    let cases = [
        ("bass-5-std", 23.0, 0.1),
        ("bass-5-std", 23.0, 0.05),
        ("bass-5-std", 23.0, 0.02),
        ("7-drop-a", 33.0, 0.05),
        ("bass-5-drop-a", 21.0, 0.05),
        ("6-std", 40.0, 0.02),
    ];
    for (name, m, fund) in cases {
        let p = Pluck { fund, ..Pluck::new(midi_hz(m)) };
        let (h, _) = run(&[("tuning", tuning_rank(name))], &[p], 1.5);
        let mut notes: Vec<f32> = h.iter().map(|q| q.get("note")).collect();
        notes.dedup();
        let last = h.last().unwrap();
        assert_eq!(notes, [0.0, m as f32], "{name} midi {m}, fundamental {fund}");
        assert_eq!(last.get("string"), 1.0);
        assert!(last.get("cents").abs() < 0.5, "{name} midi {m}: {} ¢", last.get("cents"));
    }
}
