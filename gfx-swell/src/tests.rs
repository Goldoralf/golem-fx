// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5), on synthetic Karplus-Strong strings with known onsets.

use crate::{Event, Swell, LOOKAHEAD_MS};
use gfx_dsp::delay::{DelayLine, Interp};
use gfx_dsp::lfo::Rng;
use gfx_dsp::onset::Source;
use gfx_dsp::swell::{Curve, State};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::Effect;

const SR: f32 = 48000.0;

fn n_of(s: f32) -> usize {
    (s * SR) as usize
}

/// A Karplus-Strong string: a delay of one period, a two-point average and a loss per period. A pick
/// is a burst of noise one period long (`soft` low-passes it: a finger, no transient); a pitch change
/// moves the period without exciting the string (legato).
struct Ks {
    line: DelayLine,
    p: f64,
    g: f32,
    decay_db_s: f32,
    prev: f32,
    rng: Rng,
    burst: usize,
    amp: f32,
    soft: f32,
    lp: f32,
    sr: f32,
}

impl Ks {
    fn new(hz: f32, decay_db_s: f32, seed: u64) -> Self {
        Self::at_rate(hz, decay_db_s, seed, SR)
    }
    fn at_rate(hz: f32, decay_db_s: f32, seed: u64, sr: f32) -> Self {
        let mut line = DelayLine::new();
        line.prepare(8192);
        let mut k = Self { line, p: 0.0, g: 1.0, decay_db_s, prev: 0.0, rng: Rng::new(seed), burst: 0, amp: 0.0, soft: 0.0, lp: 0.0, sr };
        k.set_hz(hz);
        k
    }
    fn set_hz(&mut self, hz: f32) {
        self.p = self.sr as f64 / hz as f64;
        self.g = 10f32.powf(-self.decay_db_s * (self.p as f32 / self.sr) / 20.0);
    }
    fn pluck(&mut self, amp: f32) {
        self.burst = self.p as usize;
        self.amp = amp;
        self.soft = 0.0;
    }
    /// a soft attack: the burst low-passed at ~300 Hz, no pick transient
    fn finger(&mut self, amp: f32) {
        self.burst = self.p as usize;
        self.amp = amp * 4.0;
        self.soft = 0.96;
    }
    fn next(&mut self) -> f32 {
        let mut ex = 0.0;
        if self.burst > 0 {
            self.burst -= 1;
            let w = self.amp * self.rng.bipolar();
            self.lp = w + (self.lp - w) * self.soft;
            ex = self.lp;
        }
        let r = self.line.read(self.p - 1.5, Interp::Lagrange3);
        let y = ex + self.g * 0.5 * (r + self.prev);
        self.prev = r;
        self.line.push(y);
        y
    }
}

fn swell() -> Swell {
    let mut s = Swell::new();
    s.prepare(SR, 64, 1);
    s.reset();
    s
}

fn with(params: &[(&str, f32)]) -> Swell {
    let mut s = Swell::new();
    s.prepare(SR, 64, 1);
    for (id, v) in params {
        s.set_param(id, *v);
    }
    s.reset();
    s
}

/// Renders in blocks of 64, collecting the onset log.
fn run(s: &mut Swell, x: &[f32]) -> (Vec<f32>, Vec<Event>) {
    let mut y = x.to_vec();
    let mut ev = vec![];
    for c in y.chunks_mut(64) {
        s.process(&mut [c]);
        s.take_events(&mut ev);
    }
    (y, ev)
}

fn accepted(ev: &[Event]) -> Vec<Event> {
    ev.iter().filter(|e| e.accepted).copied().collect()
}

fn ms_of(at: u64) -> f32 {
    at as f32 * 1000.0 / SR
}

fn energy(x: &[f32]) -> f64 {
    x.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>()
}

fn db(x: f64) -> f64 {
    10.0 * x.max(1e-30).log10()
}

/// Pink noise (Kellett's filter on white), scaled to an RMS in dBFS.
fn pink(n: usize, rms_db: f32, seed: u64) -> Vec<f32> {
    let mut r = Rng::new(seed);
    let mut b = [0.0f32; 7];
    let mut x: Vec<f32> = (0..n)
        .map(|_| {
            let w = r.bipolar();
            b[0] = 0.99886 * b[0] + w * 0.0555179;
            b[1] = 0.99332 * b[1] + w * 0.0750759;
            b[2] = 0.96900 * b[2] + w * 0.153_852;
            b[3] = 0.86650 * b[3] + w * 0.3104856;
            b[4] = 0.55000 * b[4] + w * 0.5329522;
            b[5] = -0.7616 * b[5] - w * 0.0168980;
            let y = b.iter().sum::<f32>() + w * 0.5362;
            b[6] = w * 0.115926;
            y
        })
        .collect();
    let rms = (energy(&x) / n as f64).sqrt() as f32;
    let k = 10f32.powf(rms_db / 20.0) / rms;
    x.iter_mut().for_each(|v| *v *= k);
    x
}

/// Renders strings: `events` are (time s, string, action) with action Pluck(amp), Finger(amp),
/// Hz(f) (an instantaneous pitch change), Stop.
#[derive(Clone, Copy)]
enum Act {
    Pluck(f32),
    Finger(f32),
    Hz(f32),
}

fn strings(len_s: f32, voices: &[(f32, f32)], events: &[(f32, usize, Act)]) -> Vec<f32> {
    let mut ks: Vec<Ks> = voices.iter().enumerate().map(|(k, &(hz, dec))| Ks::new(hz, dec, 11 + k as u64)).collect();
    let mut evs = events.to_vec();
    evs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut e = 0;
    (0..n_of(len_s))
        .map(|i| {
            while e < evs.len() && n_of(evs[e].0) <= i {
                let (_, k, a) = evs[e];
                match a {
                    Act::Pluck(amp) => ks[k].pluck(amp),
                    Act::Finger(amp) => ks[k].finger(amp),
                    Act::Hz(f) => ks[k].set_hz(f),
                }
                e += 1;
            }
            ks.iter_mut().map(|k| k.next()).sum()
        })
        .collect()
}

/// A pitch path in time (Hz).
type PitchPath = Box<dyn Fn(f32) -> f32>;

/// A string with its pitch moved every sample by `f(t)` (Hz) after a pluck at 0.1 s.
fn moving_string(len_s: f32, f: &dyn Fn(f32) -> f32) -> Vec<f32> {
    let mut k = Ks::new(f(0.0), 4.0, 3);
    (0..n_of(len_s))
        .map(|i| {
            let t = i as f32 / SR;
            if i == n_of(0.1) {
                k.pluck(0.3);
            }
            k.set_hz(f(t));
            k.next()
        })
        .collect()
}

fn st(f: f32, cents: f32) -> f32 {
    f * 2f32.powf(cents / 1200.0)
}

// ---------------------------------------------------------------------------------------------

fn print(name: &str, ev: &[Event]) {
    let v: Vec<String> = ev.iter().map(|e| format!("{:?}{} {:.1}", e.src, if e.accepted { "" } else { "~" }, ms_of(e.at))).collect();
    println!("{name}: {}", v.join(", "));
}

/// The accepted onsets after `from` seconds.
fn after(ev: &[Event], from: f32) -> Vec<Event> {
    accepted(ev).into_iter().filter(|e| ms_of(e.at) > from * 1000.0).collect()
}

/// Curve: a note after silence follows each curve within ±0.3 dB over 10 – 100 % of the rise; `sync`
/// 1/4 at 120 BPM rises in 500 ms ± 1 sample.
#[test]
fn curves_follow_their_law() {
    let x: Vec<f32> = (0..n_of(1.0)).map(|i| if i < n_of(0.1) { 0.0 } else { 0.3 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / SR).sin() }).collect();
    for (k, c) in Curve::ALL.into_iter().enumerate() {
        let mut s = with(&[("curve", k as f32), ("rise", 300.0)]);
        let mut g = vec![];
        let mut y = x.clone();
        let mut ev = vec![];
        for c in y.chunks_mut(1) {
            s.process(&mut [c]);
            g.push(s.gain());
            s.take_events(&mut ev);
        }
        let on = accepted(&ev);
        assert_eq!(on.len(), 1, "{c:?}: {ev:?}");
        let t0 = on[0].at as usize - 1;
        let rise = 0.3 * SR;
        let mut worst = 0.0f64;
        for i in (t0 + (0.1 * rise) as usize)..(t0 + rise as usize) {
            let want = c.c((i - t0 + 1) as f64 / rise as f64);
            worst = worst.max((db(g[i] as f64 * g[i] as f64) - db(want * want)).abs());
        }
        assert!(worst < 0.3, "{c:?}: {worst:.3} dB");
        assert_eq!(g[t0 + rise as usize + 10], 1.0);
    }
    let mut s = with(&[("sync", 7.0), ("rise", 5.0)]);
    s.set_transport(TransportInfo { tempo: Some(120.0), pos_beats: None, playing: false });
    let mut y = x.clone();
    let mut g = vec![];
    let mut ev = vec![];
    for c in y.chunks_mut(1) {
        s.process(&mut [c]);
        g.push(s.gain());
        s.take_events(&mut ev);
    }
    let t0 = accepted(&ev)[0].at as usize - 1;
    let full = (t0..g.len()).find(|&i| g[i] == 1.0).unwrap() - t0;
    assert!((full as i64 - 24000).abs() <= 1, "{full}");
}

/// Attack removed after silence: the first 10 ms of a pick at ≤ −40 dB of the dry.
#[test]
fn attack_removed_after_silence() {
    for hz in [82.4, 196.0, 659.3] {
        let x = strings(0.6, &[(hz, 3.0)], &[(0.2, 0, Act::Pluck(0.3))]);
        let mut s = swell();
        let (y, ev) = run(&mut s, &x);
        let (a, b) = (n_of(0.2), n_of(0.21));
        let r = db(energy(&y[a..b])) - db(energy(&x[a..b]));
        println!("{hz} Hz: first 10 ms at {r:.1} dB, {:?}", accepted(&ev).first());
        assert!(r <= -40.0, "{hz} Hz: {r:.1} dB");
    }
}

/// Never silent: a sine faded in over 50 ms with no transient (`legato` 0) opens through L, once.
/// Engaging on a ringing chord swells it in.
#[test]
fn never_silent() {
    let x: Vec<f32> = (0..n_of(2.0))
        .map(|i| {
            let t = i as f32 / SR - 0.2;
            let env = if t < 0.0 { 0.0 } else { (t / 0.05).min(1.0) };
            0.2 * env * env * (2.0 * std::f32::consts::PI * 196.0 * i as f32 / SR).sin()
        })
        .collect();
    let mut s = with(&[("legato", 0.0)]);
    let (y, ev) = run(&mut s, &x);
    print("fade-in", &ev);
    let on = accepted(&ev);
    assert_eq!(on.len(), 1);
    assert_eq!(on[0].src, Source::Level);
    let tail = n_of(1.5)..n_of(2.0);
    assert!((db(energy(&y[tail.clone()])) - db(energy(&x[tail]))).abs() < 0.01);

    // engage on a ringing chord
    let x = strings(2.0, &[(110.0, 2.0), (164.8, 2.0), (220.0, 2.0)], &[(0.05, 0, Act::Pluck(0.2)), (0.06, 1, Act::Pluck(0.2)), (0.07, 2, Act::Pluck(0.2))]);
    let mut s = with(&[("bypass", 1.0)]);
    let mut y = x.clone();
    let mut ev = vec![];
    for (k, c) in y.chunks_mut(64).enumerate() {
        if k * 64 >= n_of(0.5) {
            s.set_param("bypass", 0.0);
        }
        s.process(&mut [c]);
        s.take_events(&mut ev);
    }
    print("engage", &ev);
    let on = after(&ev, 0.49);
    assert_eq!(on.len(), 1);
    assert_eq!(on[0].src, Source::Level);
    assert!(ms_of(on[0].at) < 503.0);
    // it swells in: quiet just after, at unity once the rise is done
    let r1 = db(energy(&y[n_of(0.51)..n_of(0.56)])) - db(energy(&x[n_of(0.51)..n_of(0.56)]));
    let r2 = db(energy(&y[n_of(0.9)..n_of(1.0)])) - db(energy(&x[n_of(0.9)..n_of(1.0)]));
    assert!(r1 < -20.0 && r2.abs() < 0.01, "{r1} {r2}");
}

/// Retrigger on pick: re-picks every 400 ms on a ringing string, one onset each, ≤ 1.5 ms late. The
/// leaked attack is measured at zero latency, ≤ −40 dB with `lookahead` 3 ms.
#[test]
fn retrigger_on_pick() {
    let picks: Vec<f32> = (0..6).map(|k| 0.2 + 0.4 * k as f32).collect();
    let evs: Vec<(f32, usize, Act)> = picks.iter().map(|&t| (t, 0, Act::Pluck(0.25))).collect();
    for hz in [110.0, 196.0, 440.0] {
        let x = strings(2.6, &[(hz, 3.0)], &evs);
        for la in [0usize, 1] {
            let mut s = with(&[("lookahead", la as f32)]);
            let lat = s.latency();
            let (y, ev) = run(&mut s, &x);
            let on = accepted(&ev);
            assert_eq!(on.len(), picks.len(), "{hz} Hz la {la}: {ev:?}");
            let mut leaks = vec![];
            let mut latest = 0.0f32;
            for (k, (e, &t)) in on.iter().zip(&picks).enumerate() {
                let late = ms_of(e.at) - t * 1000.0;
                assert!((0.0..=1.5).contains(&late), "{hz} Hz: late {late:.2} ms");
                latest = latest.max(late);
                if k > 0 {
                    assert_eq!(e.src, Source::Pick);
                    let (a, b) = (n_of(t), n_of(t + 0.01));
                    leaks.push(db(energy(&y[a + lat..b + lat])) - db(energy(&x[a..b])));
                }
            }
            let worst = leaks.iter().cloned().fold(f64::MIN, f64::max);
            println!("{hz} Hz lookahead {} ms: onsets ≤ {latest:.2} ms late, leaked attack worst {worst:.1} dB ({leaks:.1?})", LOOKAHEAD_MS[la]);
            if la == 1 {
                assert!(worst <= -40.0, "{hz} Hz: {worst:.1} dB");
            } else {
                assert!(worst <= -6.0, "{hz} Hz: {worst:.1} dB");
            }
        }
    }
}

/// Retrigger on legato: an instantaneous pitch change of the string with no excitation. A semitone at
/// 330 Hz fires within 18 ms, at 110 Hz within 30 ms, at 82 Hz within 35 ms. A new three-note chord
/// with no pick over a ringing one fires within 15 ms.
#[test]
fn retrigger_on_legato() {
    for (hz, within) in [(330.0, 18.0), (110.0, 30.0), (82.4, 35.0)] {
        for up in [true, false] {
            let to = st(hz, if up { 100.0 } else { -100.0 });
            let x = strings(1.2, &[(hz, 3.0)], &[(0.1, 0, Act::Pluck(0.3)), (0.6, 0, Act::Hz(to))]);
            let mut s = swell();
            let (_, ev) = run(&mut s, &x);
            print(&format!("legato {hz} → {to:.1}"), &ev);
            let on = after(&ev, 0.3);
            assert_eq!(on.len(), 1, "{hz} Hz {up}: {ev:?}");
            let late = ms_of(on[0].at) - 600.0;
            assert!((0.0..=within).contains(&late), "{hz} Hz: {late:.1} ms");
        }
    }
    // a new chord, fingered (no transient), over a ringing one
    let x = strings(
        1.2,
        &[(110.0, 3.0), (138.6, 3.0), (164.8, 3.0), (146.8, 3.0), (185.0, 3.0), (220.0, 3.0)],
        &[(0.1, 0, Act::Pluck(0.2)), (0.1, 1, Act::Pluck(0.2)), (0.1, 2, Act::Pluck(0.2)), (0.6, 3, Act::Finger(0.2)), (0.6, 4, Act::Finger(0.2)), (0.6, 5, Act::Finger(0.2))],
    );
    let mut s = swell();
    let (_, ev) = run(&mut s, &x);
    print("chord", &ev);
    let on = after(&ev, 0.3);
    assert_eq!(on.len(), 1, "{ev:?}");
    let late = ms_of(on[0].at) - 600.0;
    assert!((0.0..=15.0).contains(&late), "chord: {late:.1} ms");
    // B itself (the finger still leaves a little of a transient for A): within 15 ms
    let b = ev.iter().find(|e| e.src == Source::Flux && ms_of(e.at) > 300.0).expect("B saw no new chord");
    let late = ms_of(b.at) - 600.0;
    assert!((0.0..=15.0).contains(&late), "chord, B: {late:.1} ms");
}

/// With `lookahead` 25 ms the legato dips end within ± 3 ms of the change (330 and 110 Hz, the chord).
#[test]
fn legato_dips_placed_with_lookahead() {
    let cases: Vec<(&str, Vec<f32>)> = vec![
        ("330", strings(1.2, &[(330.0, 3.0)], &[(0.1, 0, Act::Pluck(0.3)), (0.6, 0, Act::Hz(st(330.0, 100.0)))])),
        ("110", strings(1.2, &[(110.0, 3.0)], &[(0.1, 0, Act::Pluck(0.3)), (0.6, 0, Act::Hz(st(110.0, 100.0)))])),
        (
            "chord",
            strings(
                1.2,
                &[(110.0, 3.0), (138.6, 3.0), (164.8, 3.0), (146.8, 3.0), (185.0, 3.0), (220.0, 3.0)],
                &[(0.1, 0, Act::Pluck(0.2)), (0.1, 1, Act::Pluck(0.2)), (0.1, 2, Act::Pluck(0.2)), (0.6, 3, Act::Finger(0.2)), (0.6, 4, Act::Finger(0.2)), (0.6, 5, Act::Finger(0.2))],
            ),
        ),
    ];
    for (name, x) in cases {
        let mut s = with(&[("lookahead", 3.0)]);
        let lat = s.latency();
        let mut g = vec![];
        let mut y = x.clone();
        for c in y.chunks_mut(1) {
            s.process(&mut [c]);
            g.push(s.gain());
        }
        // the dip's bottom (the gain's minimum after the first swell), in input time
        let from = n_of(0.5);
        let bottom = (from..n_of(0.7)).min_by(|&a, &b| g[a].total_cmp(&g[b])).unwrap();
        let end = ms_of((bottom - lat) as u64) - 600.0;
        println!("{name}: dip ends {end:+.1} ms from the change (gain {:.4})", g[bottom]);
        assert!(g[bottom] < 1e-3 && end.abs() <= 3.0, "{name}: {end:.1} ms");
    }
}

fn zero_after_first(name: &str, x: &[f32], params: &[(&str, f32)]) -> usize {
    let mut s = with(params);
    let (_, ev) = run(&mut s, x);
    let on = accepted(&ev);
    if on.len() > 1 {
        print(name, &ev);
    }
    on.len().saturating_sub(1)
}

/// No retrigger on decay: an 8 s decaying note; a 6-string chord with ±3 cent detuned strings beating at
/// 1 – 3 Hz.
#[test]
fn no_retrigger_on_decay() {
    let x = strings(8.5, &[(110.0, 6.0)], &[(0.1, 0, Act::Pluck(0.3))]);
    assert_eq!(zero_after_first("decay", &x, &[]), 0);
    let tuning = [82.41, 110.0, 146.83, 196.0, 246.94, 329.63];
    let voices: Vec<(f32, f32)> = tuning.iter().enumerate().map(|(k, &f)| (st(f, if k % 2 == 0 { 3.0 } else { -3.0 }), 3.0 + k as f32)).collect();
    let evs: Vec<(f32, usize, Act)> = (0..6).map(|k| (0.1 + 0.012 * k as f32, k, Act::Pluck(0.12))).collect();
    let x = strings(8.0, &voices, &evs);
    for legato in [50.0, 100.0] {
        assert_eq!(zero_after_first("beating chord", &x, &[("legato", legato)]), 0, "legato {legato}");
    }
}

/// No retrigger on vibrato, bends, a dive or an upstream tremolo; a fast chromatic slide swells once.
#[test]
fn no_retrigger_on_vibrato_and_bends() {
    let sine = |t: f32| (2.0 * std::f32::consts::PI * t).sin();
    // on five strings' worth of pitches, low E to the 12th fret of the high E
    for f0 in [82.4, 110.0, 220.0, 330.0, 659.3] {
        let cases: Vec<(&str, PitchPath)> = vec![
            ("vibrato ±50 c 6 Hz", Box::new(move |t| st(f0, if t > 0.4 { 50.0 * sine(6.0 * (t - 0.4)) } else { 0.0 }))),
            ("vibrato ±100 c 7 Hz", Box::new(move |t| st(f0, if t > 0.4 { 100.0 * sine(7.0 * (t - 0.4)) } else { 0.0 }))),
            ("bend 200 c / 100 ms", Box::new(move |t| st(f0, 200.0 * ((t - 0.5) / 0.1).clamp(0.0, 1.0) - 200.0 * ((t - 1.0) / 0.1).clamp(0.0, 1.0)))),
            ("bend 200 c / 40 ms", Box::new(move |t| st(f0, 200.0 * ((t - 0.5) / 0.04).clamp(0.0, 1.0) - 200.0 * ((t - 1.0) / 0.04).clamp(0.0, 1.0)))),
            ("dive 12 st / 300 ms", Box::new(move |t| st(2.0 * f0, -1200.0 * ((t - 0.5) / 0.3).clamp(0.0, 1.0)))),
        ];
        for legato in [50.0, 100.0] {
            for (name, f) in &cases {
                let x = moving_string(1.6, f.as_ref());
                assert_eq!(zero_after_first(name, &x, &[("legato", legato)]), 0, "{name} at {f0} Hz, legato {legato}");
            }
        }
    }
    for legato in [50.0, 100.0] {
        // tremolo of 6 dB at 5 Hz upstream (a sine)
        let x: Vec<f32> = (0..n_of(3.0))
            .map(|i| {
                let t = i as f32 / SR;
                let g = 10f32.powf(-6.0 * (0.5 + 0.5 * sine(5.0 * t)) / 20.0);
                0.3 * g * sine(220.0 * t)
            })
            .collect();
        assert_eq!(zero_after_first("tremolo", &x, &[("legato", legato)]), 0, "tremolo legato {legato}");
    }
    // a chromatic slide of 5 frets in 150 ms: one onset
    let slide = |t: f32| st(196.0, 100.0 * ((t - 0.5) / 0.03).floor().clamp(0.0, 5.0));
    let x = moving_string(1.2, &slide);
    let mut s = swell();
    let (_, ev) = run(&mut s, &x);
    print("slide", &ev);
    assert_eq!(after(&ev, 0.3).len(), 1);
}

/// No retrigger on let-ring: two and three strings ringing together, decaying at different rates: no
/// onset from C. An arpeggio left ringing: one onset per pick, from A.
#[test]
fn no_retrigger_on_let_ring() {
    let pairs = [("fifth", 2f32.powf(7.0 / 12.0)), ("fourth", 2f32.powf(5.0 / 12.0)), ("third", 2f32.powf(4.0 / 12.0)), ("octave", 2.0)];
    for legato in [50.0, 100.0] {
        for (name, r) in pairs {
            for base in [110.0, 196.0] {
                let x = strings(4.0, &[(base, 2.0), (base * r, 8.0)], &[(0.1, 0, Act::Pluck(0.2)), (0.1, 1, Act::Pluck(0.2))]);
                assert_eq!(zero_after_first(name, &x, &[("legato", legato)]), 0, "{name} {base} legato {legato}");
                let x = strings(4.0, &[(base * r, 2.0), (base, 8.0)], &[(0.1, 0, Act::Pluck(0.2)), (0.1, 1, Act::Pluck(0.2))]);
                assert_eq!(zero_after_first(name, &x, &[("legato", legato)]), 0, "{name} {base} reversed legato {legato}");
            }
        }
        let x = strings(4.0, &[(110.0, 2.0), (164.8, 5.0), (220.0, 8.0)], &[(0.1, 0, Act::Pluck(0.2)), (0.1, 1, Act::Pluck(0.2)), (0.1, 2, Act::Pluck(0.2))]);
        assert_eq!(zero_after_first("triad", &x, &[("legato", legato)]), 0, "triad legato {legato}");
    }
    // an arpeggio left ringing
    let notes = [110.0, 164.8, 220.0, 277.2, 329.6];
    let voices: Vec<(f32, f32)> = notes.iter().map(|&f| (f, 3.0)).collect();
    let evs: Vec<(f32, usize, Act)> = (0..5).map(|k| (0.1 + 0.3 * k as f32, k, Act::Pluck(0.2))).collect();
    let x = strings(2.5, &voices, &evs);
    let mut s = swell();
    let (_, ev) = run(&mut s, &x);
    print("arpeggio", &ev);
    let on = accepted(&ev);
    assert_eq!(on.len(), 5);
    assert!(on[1..].iter().all(|e| e.src == Source::Pick));
}

/// One swell per strum: 6 strings 12 ms apart (down), 6 ms apart (up); two strums 250 ms apart. With
/// `strum` 20 ms, a picked note change on a low E still gives one onset (the same-note hold-off).
#[test]
fn one_swell_per_strum() {
    let tuning = [82.41, 110.0, 146.83, 196.0, 246.94, 329.63];
    let voices: Vec<(f32, f32)> = tuning.iter().map(|&f| (f, 3.0)).collect();
    for (gap, rev) in [(0.012, false), (0.006, true)] {
        let evs: Vec<(f32, usize, Act)> = (0..6).map(|k| (0.1 + gap * k as f32, if rev { 5 - k } else { k }, Act::Pluck(0.12))).collect();
        let x = strings(1.0, &voices, &evs);
        let mut s = swell();
        let (_, ev) = run(&mut s, &x);
        print("strum", &ev);
        assert_eq!(accepted(&ev).len(), 1);
    }
    let mut evs: Vec<(f32, usize, Act)> = (0..6).map(|k| (0.1 + 0.012 * k as f32, k, Act::Pluck(0.12))).collect();
    evs.extend((0..6).map(|k| (0.35 + 0.006 * k as f32, 5 - k, Act::Pluck(0.12))));
    let x = strings(1.0, &voices, &evs);
    let mut s = swell();
    let (_, ev) = run(&mut s, &x);
    print("two strums", &ev);
    assert_eq!(accepted(&ev).len(), 2);
    // `strum` 20 ms: a picked change on the low E
    let x = strings(1.2, &[(82.41, 3.0)], &[(0.1, 0, Act::Pluck(0.3)), (0.6, 0, Act::Hz(87.31)), (0.6, 0, Act::Pluck(0.3))]);
    let mut s = with(&[("strum", 20.0)]);
    let (_, ev) = run(&mut s, &x);
    print("strum 20, low E", &ev);
    assert_eq!(after(&ev, 0.3).len(), 1);
}

/// Noise and level independence.
#[test]
fn noise_and_level_independence() {
    for (rms, p) in [(-50.0, 50.0), (-20.0, 50.0), (-50.0, 75.0), (-20.0, 75.0)] {
        let x = pink(n_of(6.0), rms, 3);
        assert_eq!(zero_after_first("pink", &x, &[("sens", p), ("legato", p)]), 0, "pink {rms} dB, {p} %");
    }
    // 50 Hz hum with harmonics at −50 dBFS
    let hum: Vec<f32> = (0..n_of(6.0))
        .map(|i| {
            let t = i as f32 / SR;
            (1..8).map(|h| (2.0 * std::f32::consts::PI * 50.0 * h as f32 * t).sin() / h as f32).sum::<f32>() * 0.0032 / 1.6
        })
        .collect();
    for p in [50.0, 75.0] {
        assert_eq!(zero_after_first("hum", &hum, &[("sens", p), ("legato", p)]), 0, "hum {p} %");
    }
    // a phrase with 3 s pauses over hiss 45 dB under it; the same phrase 12 and 24 dB quieter
    let phrase = |gain_db: f32| -> Vec<f32> {
        let g = 10f32.powf(gain_db / 20.0);
        let notes = [(0.2, 196.0), (0.6, 220.0), (1.0, 246.9), (4.5, 196.0), (4.9, 261.6), (8.5, 329.6)];
        let voices: Vec<(f32, f32)> = notes.iter().map(|&(_, f)| (f, 12.0)).collect();
        let evs: Vec<(f32, usize, Act)> = notes.iter().enumerate().map(|(k, &(t, _))| (t, k, Act::Pluck(0.25))).collect();
        let mut x = strings(10.0, &voices, &evs);
        let hiss = pink(x.len(), -12.0 - 45.0, 21);
        x.iter_mut().zip(&hiss).for_each(|(a, h)| *a = (*a + h) * g);
        x
    };
    let mut times: Vec<Vec<f32>> = vec![];
    for gdb in [0.0, -12.0, -24.0] {
        let x = phrase(gdb);
        let mut s = swell();
        let (_, ev) = run(&mut s, &x);
        print(&format!("phrase {gdb} dB"), &ev);
        // the hiss itself opens the swell at the start when it stands above `thresh` (0 and −12 dB: the
        // README's honest limit, §2.2); the notes are compared
        let on: Vec<f32> = accepted(&ev).iter().map(|e| ms_of(e.at)).filter(|&t| t > 100.0).collect();
        // nothing in the pauses (1.6 – 4.4 s, 5.5 – 8.4 s)
        assert!(on.iter().all(|&t| !(1600.0..4400.0).contains(&t) && !(5500.0..8400.0).contains(&t)), "{gdb} dB: {on:?}");
        times.push(on);
    }
    for t in &times[1..] {
        assert_eq!(t.len(), times[0].len(), "{times:?}");
        for (a, b) in t.iter().zip(&times[0]) {
            assert!((a - b).abs() <= 1.0, "{times:?}");
        }
    }
}

/// Reset: a note stopped by hand (−40 dB in 60 ms) is Closed within 150 ms, so a pick 200 ms later is
/// fully removed. A note left ringing 4 s does not reset.
#[test]
fn reset_after_a_stopped_note() {
    let mut x = strings(1.6, &[(196.0, 3.0), (196.0, 3.0)], &[(0.1, 0, Act::Pluck(0.3)), (1.2, 1, Act::Pluck(0.3))]);
    // the first string is muted at 1.0 s: −40 dB over 60 ms, then silent
    let ks1 = strings(1.6, &[(196.0, 3.0)], &[(0.1, 0, Act::Pluck(0.3))]);
    for i in 0..x.len() {
        let t = i as f32 / SR;
        let m = if t < 1.0 { 1.0 } else if t < 1.06 { 10f32.powf(-2.0 * (t - 1.0) / 0.06) } else { 0.0 };
        x[i] += ks1[i] * (m - 1.0);
    }
    let mut s = swell();
    let mut y = x.clone();
    let mut closed_at = None;
    for (k, c) in y.chunks_mut(64).enumerate() {
        s.process(&mut [c]);
        if closed_at.is_none() && k * 64 > n_of(1.0) && s.gain() == 0.0 {
            closed_at = Some(k as f32 * 64.0 / SR);
        }
    }
    let closed = closed_at.expect("never closed") - 1.0;
    println!("closed {:.0} ms after the stop", closed * 1000.0);
    assert!(closed <= 0.15);
    let (a, b) = (n_of(1.2), n_of(1.21));
    let r = db(energy(&y[a..b])) - db(energy(&x[a..b]));
    println!("pick after the reset: {r:.1} dB");
    assert!(r <= -40.0);
    // left ringing (3 dB / s): no reset in 4 s
    let x = strings(4.5, &[(196.0, 3.0)], &[(0.1, 0, Act::Pluck(0.3))]);
    let mut s = swell();
    let mut y = x.clone();
    for (k, c) in y.chunks_mut(64).enumerate() {
        s.process(&mut [c]);
        if k * 64 > n_of(0.5) {
            assert_eq!(s.state(), State::Hold, "reset at {:.2} s", k as f32 * 64.0 / SR);
        }
    }
}

/// 4th-order high-pass at 10 kHz (the bench's click detector).
fn hf(x: &[f32]) -> Vec<f32> {
    use gfx_dsp::svf::{Svf, SvfMode};
    let mut a = Svf::new();
    let mut b = Svf::new();
    a.set(SvfMode::HighPass, 10000.0, 0.541_196, 0.0, SR);
    b.set(SvfMode::HighPass, 10000.0, 1.306_563, 0.0, SR);
    x.iter().map(|&v| b.process(a.process(v))).collect()
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

/// No clicks: the 8 ms dip (`fire`) on a sustained 1 kHz sine stays within −50 dB of HF; the reset fade
/// and the bow decay too (measured).
#[test]
fn no_clicks() {
    let x: Vec<f32> = (0..n_of(3.0)).map(|i| 0.25 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / SR).sin()).collect();
    let burst = |y: &[f32], from: f32, to: f32| -> f32 {
        let h = hf(y);
        let steady = peak(&h[n_of(0.9)..n_of(1.0)]);
        20.0 * ((peak(&h[n_of(from)..n_of(to)]) - steady).max(1e-9) / peak(y)).log10()
    };
    let mut s = with(&[("rise", 50.0)]);
    let mut y = x.clone();
    for (k, c) in y.chunks_mut(64).enumerate() {
        s.set_param("fire", if k * 64 >= n_of(1.5) { 1.0 } else { 0.0 });
        s.process(&mut [c]);
    }
    let b = burst(&y, 1.45, 1.7);
    println!("fire dip: HF burst {b:.1} dB");
    assert!(b <= -50.0);
    // bow decay and reset fade
    let mut s = with(&[("rise", 50.0), ("decay", 400.0)]);
    let (y, _) = run(&mut s, &x);
    let b = burst(&y, 1.0, 2.0);
    let b0 = burst(&y, 0.0, 0.5);
    println!("bow: HF burst {b:.1} dB (rise {b0:.1} dB)");
    assert!(b <= -60.0);
    let mut z = x.clone();
    // a note stopped by hand: −40 dB over 60 ms, which resets the swell
    for (i, v) in z.iter_mut().enumerate().skip(n_of(1.5)) {
        *v *= 10f32.powf(-2.0 * ((i - n_of(1.5)) as f32 / (0.06 * SR)).min(1.0));
    }
    let mut s = with(&[("rise", 50.0)]);
    let (y, _) = run(&mut s, &z);
    let b = burst(&y, 1.5, 2.0);
    println!("reset fade: HF burst {b:.1} dB");
    assert!(b <= -50.0);
}

/// Bow: `decay` 400 ms on an 8 s note: one attack-decay, then Spent: no onset until a pick.
#[test]
fn bow_then_spent() {
    let x = strings(8.0, &[(196.0, 2.0), (246.9, 2.0)], &[(0.1, 0, Act::Pluck(0.3)), (6.0, 1, Act::Pluck(0.3))]);
    let mut s = with(&[("rise", 200.0), ("decay", 400.0)]);
    let mut y = x.clone();
    let mut ev = vec![];
    let mut states = vec![];
    for c in y.chunks_mut(64) {
        s.process(&mut [c]);
        s.take_events(&mut ev);
        if states.last() != Some(&s.state()) {
            states.push(s.state());
        }
    }
    print("bow", &ev);
    println!("{states:?}");
    let on = accepted(&ev);
    assert_eq!(on.len(), 2);
    assert!((ms_of(on[1].at) - 6000.0).abs() < 2.0);
    assert_eq!(&states[..4], &[State::Idle, State::Rise, State::Decay, State::Idle]);
    let quiet = n_of(1.0)..n_of(5.9);
    assert!(peak(&y[quiet]) == 0.0);
}

/// Latency: `lookahead` 3 / 10 / 25 ms reported = measured; a change crossfades without a click.
#[test]
fn lookahead_latency() {
    let x = pink(n_of(2.0), -20.0, 5);
    for la in 1..4 {
        let mut s = with(&[("lookahead", la as f32)]);
        let lat = s.latency();
        assert_eq!(lat, (LOOKAHEAD_MS[la] * 0.001 * SR).round() as usize);
        let (y, _) = run(&mut s, &x);
        for i in n_of(1.0)..x.len() {
            assert_eq!(y[i], x[i - lat], "la {la} at {i}");
        }
    }
    let x: Vec<f32> = (0..n_of(2.0)).map(|i| 0.25 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / SR).sin()).collect();
    for (from, to) in [(0.0, 3.0), (3.0, 0.0), (1.0, 2.0)] {
        let mut s = with(&[("lookahead", from), ("rise", 20.0)]);
        let mut y = x.clone();
        for (k, c) in y.chunks_mut(64).enumerate() {
            if k * 64 == n_of(1.0) / 64 * 64 {
                s.set_param("lookahead", to);
            }
            s.process(&mut [c]);
        }
        assert_eq!(s.latency(), (LOOKAHEAD_MS[to as usize] * 0.001 * SR).round() as usize);
        let h = hf(&y);
        let steady = peak(&h[n_of(0.5)..n_of(0.9)]);
        let b = 20.0 * ((peak(&h[n_of(0.99)..n_of(1.1)]) - steady).max(1e-9) / peak(&y)).log10();
        println!("lookahead {from} → {to}: HF burst {b:.1} dB");
        assert!(b <= -50.0);
    }
}

/// Key: `source` Key with nothing connected behaves as Main; a key drives detection.
#[test]
fn key_input() {
    let x = strings(2.0, &[(196.0, 3.0)], &[(0.1, 0, Act::Pluck(0.3)), (0.9, 0, Act::Pluck(0.3))]);
    let mut a = swell();
    let mut b = with(&[("source", 1.0)]);
    let (ya, _) = run(&mut a, &x);
    let (yb, _) = run(&mut b, &x);
    assert_eq!(ya, yb);
    // the key carries the picks, the main a steady pad: the pad swells on the key's picks
    let pad: Vec<f32> = (0..x.len()).map(|i| 0.2 * (2.0 * std::f32::consts::PI * 300.0 * i as f32 / SR).sin()).collect();
    let mut s = with(&[("source", 1.0)]);
    let mut y = pad.clone();
    let mut ev = vec![];
    for (c, k) in y.chunks_mut(64).zip(x.chunks(64)) {
        s.process_with_key(&mut [c], Some(&[k]));
        s.take_events(&mut ev);
    }
    print("key", &ev);
    let on = accepted(&ev);
    assert_eq!(on.len(), 2);
    assert!((ms_of(on[1].at) - 900.0).abs() < 1.5);
}

/// Mono and stereo: one gain for both channels, from their mean.
#[test]
fn stereo_one_gain() {
    let x = strings(1.0, &[(196.0, 3.0)], &[(0.1, 0, Act::Pluck(0.3)), (0.5, 0, Act::Pluck(0.3))]);
    let mut m = swell();
    let (ym, _) = run(&mut m, &x);
    let mut s = Swell::new();
    s.prepare(SR, 64, 2);
    s.reset();
    let (mut l, mut r) = (x.clone(), x.clone());
    for (a, b) in l.chunks_mut(64).zip(r.chunks_mut(64)) {
        s.process(&mut [a, b]);
    }
    assert_eq!(l, ym);
    assert_eq!(r, ym);
}

/// CPU per sample by `legato` (not a check): `cargo test --release -p gfx-swell cpu -- --ignored --nocapture`.
#[test]
#[ignore]
fn cpu_by_legato() {
    let x = pink(n_of(10.0), -20.0, 13);
    for legato in [0.0, 50.0] {
        let mut s = with(&[("legato", legato)]);
        let mut y = x.clone();
        let t = std::time::Instant::now();
        for c in y.chunks_mut(128) {
            s.process(&mut [c]);
        }
        println!("legato {legato}: {:.1} ns / sample", t.elapsed().as_secs_f64() * 1e9 / x.len() as f64);
    }
}

/// Detector traces for tuning (not a check): `cargo test -p gfx-swell diag -- --ignored --nocapture`.
#[test]
#[ignore]
fn diag_traces() {
    use gfx_dsp::onset::{Flux, PitchJump};
    let show_c = |name: &str, x: &[f32], from: f32, to: f32| {
        let mut s = swell();
        let mut lines = vec![];
        for (i, &v) in x.iter().enumerate() {
            s.level.next(v, -66.0);
            let j = s.pitch.next(v, PitchJump::clarity_for(0.5));
            let t = i as f32 / SR;
            if (s.pitch.fresh() || j.is_some()) && t >= from && t <= to {
                let (hz, cl) = s.pitch.analysis();
                lines.push(format!("{:.1}:{}/{:.2}{}", t * 1000.0, hz.map_or("-".into(), |h| format!("{h:.1}")), cl, if j.is_some() { "!" } else { "" }));
            }
        }
        println!("C {name}: {}", lines.join(" "));
    };
    let x = moving_string(1.6, &|t| st(82.4, if t > 0.4 { 100.0 * (2.0 * std::f32::consts::PI * 7.0 * (t - 0.4)).sin() } else { 0.0 }));
    show_c("vib 82", &x, 0.40, 0.45);
    show_c("vib 82 late", &x, 1.19, 1.22);
    for (hz, c) in [(82.4, 100.0), (82.4, -100.0), (110.0, 100.0), (110.0, -100.0)] {
        let x = strings(1.2, &[(hz, 3.0)], &[(0.1, 0, Act::Pluck(0.3)), (0.6, 0, Act::Hz(st(hz, c)))]);
        show_c(&format!("legato {hz} {c}"), &x, 0.59, 0.625);
    }
    let x = strings(6.6, &[(196.0, 2.0), (246.9, 2.0)], &[(0.1, 0, Act::Pluck(0.3)), (6.0, 1, Act::Pluck(0.3))]);
    show_c("let-ring 196 + 246.9", &x, 5.99, 6.2);
    let x = strings(1.2, &[(330.0, 3.0)], &[(0.1, 0, Act::Pluck(0.3)), (0.6, 0, Act::Hz(st(330.0, 100.0)))]);
    show_c("330 legato", &x, 0.59, 0.63);
    let show_b = |name: &str, x: &[f32], legato: f32| {
        let mut s = swell();
        let mut worst = (0.0f32, 0.0f32, 0.0f32);
        let mut fires = vec![];
        for (i, &v) in x.iter().enumerate() {
            s.level.next(v, -66.0);
            let f = s.flux.next(v, s.level.phrase_db(), Flux::delta_for(legato));
            let (d, thr) = s.flux.last();
            let t = i as f32 / SR;
            if t > 0.3 && d - thr > worst.0 - worst.1 {
                worst = (d, thr, t);
            }
            if f {
                fires.push(format!("{:.1}", t * 1000.0));
            }
        }
        println!("B {name}: fires {fires:?}; worst after 0.3 s: D {:.1} thr {:.1} at {:.3} s", worst.0, worst.1, worst.2);
    };
    let tuning = [82.41, 110.0, 146.83, 196.0, 246.94, 329.63];
    let voices: Vec<(f32, f32)> = tuning.iter().map(|&f| (f, 3.0)).collect();
    let evs: Vec<(f32, usize, Act)> = (0..6).map(|k| (0.1 + 0.012 * k as f32, k, Act::Pluck(0.12))).collect();
    show_b("strum decay", &strings(3.0, &voices, &evs), 0.5);
    show_b("pink -50", &pink(n_of(6.0), -50.0, 3), 0.75);
    show_b("pink -50 @50", &pink(n_of(6.0), -50.0, 3), 0.5);
    let x = strings(
        1.2,
        &[(110.0, 3.0), (138.6, 3.0), (164.8, 3.0), (146.8, 3.0), (185.0, 3.0), (220.0, 3.0)],
        &[(0.1, 0, Act::Pluck(0.2)), (0.1, 1, Act::Pluck(0.2)), (0.1, 2, Act::Pluck(0.2)), (0.6, 3, Act::Finger(0.2)), (0.6, 4, Act::Finger(0.2)), (0.6, 5, Act::Finger(0.2))],
    );
    show_b("new chord", &x, 0.5);
}

/// The detectors at 44.1 and 96 kHz: picks, a semitone legato, and the negatives (vibrato, a decaying
/// note, pink noise).
#[test]
fn other_sample_rates() {
    for sr in [44100.0f32, 96000.0] {
        let n = |t: f32| (t * sr) as usize;
        let render = |x: &[f32]| -> Vec<Event> {
            let mut s = Swell::new();
            s.prepare(sr, 64, 1);
            s.reset();
            let mut y = x.to_vec();
            let mut ev = vec![];
            for c in y.chunks_mut(64) {
                s.process(&mut [c]);
                s.take_events(&mut ev);
            }
            accepted(&ev)
        };
        let string = |len: f32, f: &dyn Fn(f32) -> f32, picks: &[f32]| -> Vec<f32> {
            let mut k = Ks::at_rate(f(0.0), 3.0, 7, sr);
            (0..n(len))
                .map(|i| {
                    let t = i as f32 / sr;
                    if picks.iter().any(|&p| n(p) == i) {
                        k.pluck(0.3);
                    }
                    k.set_hz(f(t));
                    k.next()
                })
                .collect()
        };
        let at = |e: &Event| e.at as f32 / sr;
        // picks every 400 ms
        let on = render(&string(1.5, &|_| 196.0, &[0.1, 0.5, 0.9, 1.3]));
        assert_eq!(on.len(), 4, "{sr}: {on:?}");
        // a semitone legato at 330 and 110 Hz
        for hz in [330.0, 110.0] {
            let on = render(&string(1.2, &|t| if t < 0.6 { hz } else { st(hz, 100.0) }, &[0.1]));
            assert_eq!(on.len(), 2, "{sr} {hz}: {on:?}");
            assert!(at(&on[1]) > 0.6 && at(&on[1]) < 0.63, "{sr} {hz}: {on:?}");
        }
        // vibrato ±100 cents at 7 Hz, a decaying note
        let on = render(&string(2.0, &|t| st(220.0, if t > 0.4 { 100.0 * (2.0 * std::f32::consts::PI * 7.0 * (t - 0.4)).sin() } else { 0.0 }), &[0.1]));
        assert_eq!(on.len(), 1, "{sr} vibrato: {on:?}");
        let on = render(&string(6.0, &|_| 110.0, &[0.1]));
        assert_eq!(on.len(), 1, "{sr} decay: {on:?}");
        // pink noise at −20 dBFS: once, at the start
        let mut r = Rng::new(4);
        let mut b = 0.0f32;
        let x: Vec<f32> = (0..n(4.0)).map(|_| { b = 0.98 * b + 0.02 * r.bipolar(); b * 3.0 + 0.03 * r.bipolar() }).collect();
        assert_eq!(render(&x).len(), 1, "{sr} noise");
    }
}
