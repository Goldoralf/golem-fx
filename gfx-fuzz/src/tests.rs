// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5, the rows that run on synthetic signals).

use super::*;
use gfx_dsp::pickup::SINGLE_COIL;

const PI: f64 = std::f64::consts::PI;

/// The DC collector voltage (magnitude) of a circuit's trimmed transistor.
fn dc_bias(def: &Circuit, celsius: f64, volts: f64, bias_r: Option<f64>, leak: f64) -> f64 {
    let mut nc = NonlinearCircuit::new(&def.net, 384000.0);
    nc.set_temperature(celsius);
    nc.set_input(IN_VS, def.supply_sign * volts);
    nc.set_pot(def.fuzz, 0.7);
    if let Some(r) = bias_r {
        nc.set_var(def.bias.0, r);
    }
    for &(d, m) in &def.ge {
        nc.set_device(d, DeviceModel::Bjt(BjtModel { leak, ..m }));
    }
    assert!(nc.init_dc(), "DC point");
    nc.output(def.bias_meter).abs()
}

fn face_with(q: [BjtModel; 2], r3: f64) -> Circuit {
    let jc = if q[0].eg < 1.0 { GE_CAPS } else { SI_CAPS };
    face(q, jc, r3, &SINGLE_COIL, false)
}

#[test]
fn trimmers_give_4v5() {
    // the hard-coded trimmers put Q2's collector at 4.5 V at 25 °C, 9 V; recomputed by bisection
    for (name, q, r3) in [
        ("Ge", [GE_Q1, GE_Q2], R3_GE),
        ("Si", [SI_BC108, SI_BC108], R3_SI),
    ] {
        let (mut lo, mut hi): (f64, f64) = (1e3, 60e3);
        for _ in 0..40 {
            let mid: f64 = (lo * hi).sqrt();
            // a larger collector resistor lowers |V(c2)|
            if dc_bias(&face_with(q, mid), 25.0, 9.0, None, 1.0) > 4.5 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let v = dc_bias(&face_with(q, r3), 25.0, 9.0, None, 1.0);
        eprintln!(
            "{name}: trimmer for 4.5 V = {:.0} Ω (hard-coded {r3:.0} Ω gives {v:.3} V)",
            lo
        );
        assert!((v - 4.5).abs() < 0.05, "{name}: {v}");
    }
}

#[test]
fn operating_points_follow_the_design() {
    let ge = face_with([GE_Q1, GE_Q2], R3_GE);
    let si = face_with([SI_BC108, SI_BC108], R3_SI);
    // temperature: Ge 15 → 25 → 35 °C: 3.3 → 4.5 → 6.7 V ± 0.3 V; Si moves < 0.4 V
    let t: Vec<f64> = [15.0, 25.0, 35.0]
        .iter()
        .map(|&c| dc_bias(&ge, c, 9.0, None, 1.0))
        .collect();
    let ts: Vec<f64> = [15.0, 25.0, 35.0]
        .iter()
        .map(|&c| dc_bias(&si, c, 9.0, None, 1.0))
        .collect();
    eprintln!("temperature 15 / 25 / 35 °C: Ge {t:.2?}, Si {ts:.2?}");
    for (g, w) in t.iter().zip([3.3, 4.5, 6.7]) {
        assert!((g - w).abs() < 0.3, "Ge {t:?}");
    }
    assert!((ts[2] - ts[0]).abs() < 0.4, "Si {ts:?}");
    // leakage 0 / 100 / 400 %: 3.0 / 4.5 / 7.6 V ± 0.3 V
    let l: Vec<f64> = [0.0, 0.5, 1.0, 2.0, 4.0]
        .iter()
        .map(|&k| dc_bias(&ge, 25.0, 9.0, None, k))
        .collect();
    eprintln!("leakage 0 / 50 / 100 / 200 / 400 %: {l:.2?}");
    for (g, w) in [l[0], l[2], l[4]].iter().zip([3.0, 4.5, 7.6]) {
        assert!((g - w).abs() < 0.3, "leakage {l:?}");
    }
    // the trimmer ±100 %: ≈ 6.6 V / ≤ 0.8 V on both faces
    for (name, d, r) in [("Ge", &ge, R3_GE), ("Si", &si, R3_SI)] {
        let up = dc_bias(d, 25.0, 9.0, Some(0.5 * r), 1.0);
        let dn = dc_bias(d, 25.0, 9.0, Some(2.0 * r), 1.0);
        eprintln!("{name} bias +100 % {up:.2} V, −100 % {dn:.2} V");
        assert!((up - 6.6).abs() < 0.4 && dn <= 0.8, "{name}: {up} / {dn}");
    }
    // supply: trimmed Si at 4.5 V ≤ 0.8 V (the gate); Ge 2.6 ± 0.3 V; Si 18 V
    let s45 = dc_bias(&si, 25.0, 4.5, None, 1.0);
    let g45 = dc_bias(&ge, 25.0, 4.5, None, 1.0);
    let s18 = dc_bias(&si, 25.0, 18.0, None, 1.0);
    eprintln!("4.5 V supply: Si {s45:.2} V, Ge {g45:.2} V; 18 V: Si {s18:.2} V");
    assert!(s45 <= 0.8, "{s45}");
    assert!((g45 - 2.6).abs() < 0.3, "{g45}");
    // the raw DAFx-17 pair, stock 8.2 kΩ, at 25 °C: 8.2 ± 0.2 V (a dead pedal)
    let raw = dc_bias(
        &face_with([AC128_RAW, AC128_RAW], 8.2e3),
        25.0,
        9.0,
        None,
        1.0,
    );
    eprintln!("raw AC128 pair, stock: {raw:.2} V");
    assert!((raw - 8.2).abs() < 0.2, "{raw}");
    // Q2's open-base collector current at 100 %: 200 µA at the set's 22.8 °C
    let iceo = GE_Q2.iceo(22.8);
    assert!((iceo - 200e-6).abs() < 4e-6, "{iceo}");
}

fn fuzz(sr: f32, set: &[(&str, f32)]) -> Fuzz {
    let mut e = Fuzz::new();
    for (k, v) in set {
        e.set_param(k, *v);
    }
    e.prepare(sr, 512, 1);
    e
}

fn run(e: &mut Fuzz, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for c in y.chunks_mut(512) {
        e.process(&mut [c]);
    }
    y
}

fn sine(freq: f64, amp: f64, n: usize, sr: f64) -> Vec<f32> {
    (0..n)
        .map(|i| (amp * (2.0 * PI * freq * i as f64 / sr).sin()) as f32)
        .collect()
}

/// Complex amplitude of the component at `f` over `y[from..]`.
fn tone(y: &[f32], from: usize, f: f64, sr: f64) -> (f64, f64) {
    let w = 2.0 * PI * f / sr;
    let (mut c, mut s) = (0.0, 0.0);
    for (i, v) in y[from..].iter().enumerate() {
        let ph = w * (i + from) as f64;
        c += *v as f64 * ph.cos();
        s += *v as f64 * ph.sin();
    }
    let n = (y.len() - from) as f64;
    (2.0 * c / n, 2.0 * s / n)
}

fn tone_amp(y: &[f32], from: usize, f: f64, sr: f64) -> f64 {
    let (c, s) = tone(y, from, f, sr);
    c.hypot(s)
}

/// Worst alias (dB re the fundamental) in 20 Hz – 20 kHz of a steady sine at `f` over the last
/// second, at 44.1 kHz (where the folded harmonics fall between the true ones).
fn worst_alias(y: &[f32], f: f64, sr: f64) -> f64 {
    let from = y.len() - sr as usize;
    let f0 = tone_amp(y, from, f, sr);
    let mut worst: f64 = -300.0;
    for k in 2..400 {
        let fh = f * k as f64;
        if fh < sr / 2.0 {
            continue;
        }
        let m = (fh / sr).round();
        let fa = (fh - m * sr).abs();
        if !(20.0..=20000.0).contains(&fa) || (fa / f - (fa / f).round()).abs() < 1e-9 {
            continue;
        }
        worst = worst.max(20.0 * (tone_amp(y, from, fa, sr) / f0).log10());
    }
    worst
}

/// Worst alias of a −12 dBFS sine, `fuzz` 100 %, `buffer` on (README §5), model `m` at `stages`.
fn alias_at(m: usize, stages: Option<usize>, f: f64) -> (f64, Stats) {
    let sr = 44100.0;
    let mut e = Fuzz::new();
    e.stages_override = stages;
    for (k, v) in [("model", m as f32), ("fuzz", 100.0), ("buffer", 1.0)] {
        e.set_param(k, v);
    }
    e.prepare(sr as f32, 512, 1);
    let x = sine(f, 0.25, (sr * 1.5) as usize, sr);
    let y = run(&mut e, &x);
    (worst_alias(&y, f, sr), e.stats())
}

#[test]
#[ignore = "survey: cargo test --release -p gfx-fuzz --no-default-features alias_survey -- --ignored --nocapture"]
fn alias_survey() {
    for m in 0..N_MODELS {
        for st in [2usize, 3, 4] {
            let (a, s) = alias_at(m, Some(st), 1201.0);
            let (b, _) = alias_at(m, Some(st), 5001.0);
            eprintln!(
                "{:<8} {:>2}×: 1.2 kHz {a:.1} dB, 5 kHz {b:.1} dB; mean iterations {:.2}, worst {}, fallbacks {}, unsolved {}",
                MODELS[m],
                1 << st,
                s.mean(),
                s.worst,
                s.fallbacks,
                s.unsolved
            );
        }
    }
}

#[test]
fn aliasing_meets_the_policy() {
    // README §5 / contract: −12 dBFS 1.2 kHz, `fuzz` 100 %, `buffer` on, each model at its factor:
    // worst alias ≤ −65 dB (target −70); and no unsolved sample
    for m in 0..N_MODELS {
        let (a, s) = alias_at(m, None, 1201.0);
        eprintln!("{}: 1.2 kHz worst alias {a:.1} dB, {s:?}", MODELS[m]);
        assert!(a <= -65.0, "{}: {a}", MODELS[m]);
        assert_eq!(s.unsolved, 0);
    }
}

#[test]
fn polarity_is_normalised() {
    // every model, 1 mV, `mix` 100 %: the output in phase with the dry path (±90°), at 300 Hz:
    // at the README's 1 kHz the muff's own stack and coupling network turn the phase by ≈ 90 – 110°
    // (small_signal_phase_per_model), whatever the sign
    let sr = 48000.0;
    for m in 0..N_MODELS {
        for buffer in [0.0, 1.0] {
            let mut e = fuzz(sr as f32, &[("model", m as f32), ("buffer", buffer)]);
            let x = sine(300.0, 1e-3, 24000, sr);
            let y = run(&mut e, &x);
            // against the dry path (`mix` 0: the input through the same oversampler's identity)
            let mut d = fuzz(sr as f32, &[("model", m as f32), ("mix", 0.0)]);
            let xd = run(&mut d, &x);
            let (a, b) = (tone(&xd, 12000, 300.0, sr), tone(&y, 12000, 300.0, sr));
            let ph = (b.1.atan2(b.0) - a.1.atan2(a.0) + 3.0 * PI).rem_euclid(2.0 * PI) - PI;
            eprintln!(
                "{} buffer {buffer}: phase {:.0}°",
                MODELS[m],
                ph.to_degrees()
            );
            assert!(
                ph.abs() < PI / 2.0,
                "{} buffer {buffer}: {}°",
                MODELS[m],
                ph.to_degrees()
            );
        }
    }
}

#[test]
fn small_signal_phase_per_model() {
    // the linearized circuit's phase (buffered, `fuzz` 70 %) over the band: the sign POLARITY fixes
    for &k in &SHIPPED {
        let def = build(Config {
            model: k,
            pickup: 0,
            buffer: true,
            stack: 0,
        });
        let mut nc = NonlinearCircuit::new(&def.net, 192000.0);
        nc.set_input(IN_VS, def.supply_sign * 9.0);
        nc.set_pot(def.fuzz, 0.7);
        if let Some(t) = def.tone {
            nc.set_pot(t, 0.5);
        }
        nc.init_dc();
        let ph: Vec<f64> = [100.0, 300.0, 1000.0, 3000.0]
            .iter()
            .map(|&f| {
                let z = nc.ac_response(f, IN_SIG, def.out);
                POLARITY[k] * z.1.atan2(z.0).to_degrees()
            })
            .collect();
        eprintln!(
            "{}: phase at 100 / 300 / 1k / 3k Hz {ph:.0?}",
            KIND_NAMES[k]
        );
        assert!(ph[1].abs() < 90.0, "{}: {ph:?}", KIND_NAMES[k]);
    }
}

#[test]
fn bass_keep_has_no_notch() {
    // `bass_keep` 120 Hz, `mix` 100 %, small signal: no notch deeper than 3 dB at the crossover
    let sr = 48000.0;
    for m in 0..N_MODELS {
        let mut e = fuzz(
            sr as f32,
            &[("model", m as f32), ("bass_keep", 120.0), ("fuzz", 0.0)],
        );
        let mut r = fuzz(sr as f32, &[("model", m as f32), ("fuzz", 0.0)]);
        let x = sine(120.0, 1e-3, 48000, sr);
        let (y, yr) = (run(&mut e, &x), run(&mut r, &x));
        let (a, ar, ai) = (
            tone_amp(&y, 24000, 120.0, sr),
            tone_amp(&yr, 24000, 120.0, sr),
            tone_amp(&x, 24000, 120.0, sr),
        );
        // the split output against the smaller of its two parts (dry, fuzz)
        let d = 20.0 * (a / ar.min(ai)).log10();
        eprintln!(
            "{}: at the crossover {d:.1} dB re the smaller part",
            MODELS[m]
        );
        assert!(d > -3.0, "{}: {d}", MODELS[m]);
    }
}

/// A DI-like proxy (golem's DIs are not in this repository): plucked notes E2 … E4, harmonics at
/// 1 / k with faster decay up the series, −6 dBFS peak (gfx-dist's).
fn di_proxy(sr: f64) -> Vec<f32> {
    let notes = [82.41, 110.0, 146.83, 196.0, 246.94, 329.63, 164.81, 220.0];
    let len = (sr * 0.5) as usize;
    let mut y = vec![0.0f64; len * notes.len()];
    for (j, f) in notes.iter().enumerate() {
        for i in 0..len {
            let t = i as f64 / sr;
            let mut s = 0.0;
            for k in 1..=12 {
                s += (2.0 * PI * f * k as f64 * t).sin() / k as f64
                    * (-t * (3.0 + 2.0 * k as f64)).exp();
            }
            y[j * len + i] = s;
        }
    }
    let pk = y.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    y.iter().map(|v| (0.5 * v / pk) as f32).collect()
}

/// Loudness proxy: RMS after a 100 Hz first-order high-pass (K-weighting's low cut).
fn loudness(x: &[f32], sr: f64) -> f64 {
    let mut hp = HighPass::default();
    hp.set(100.0, sr);
    let e: f64 = x.iter().map(|&v| hp.process(v as f64).powi(2)).sum();
    10.0 * (e / x.len() as f64).log10()
}

#[test]
fn level_is_unity_at_the_defaults() {
    // K(model): at the defaults the DI proxy comes out at the bypass loudness ± 1 dB; the table
    // (with "the hardware volume pot fully up" = −K) printed
    let sr = 48000.0;
    let x = di_proxy(sr);
    let lin = loudness(&x, sr);
    let mut off = Vec::new();
    for m in 0..N_MODELS {
        let mut e = fuzz(sr as f32, &[("model", m as f32)]);
        let y = run(&mut e, &x);
        let d = loudness(&y, sr) - lin;
        let k = K_DB[SHIPPED[m]];
        eprintln!(
            "{:<8} K = {k:+.1} dB (measured without it {:+.1} dB): pot fully up = level {:+.1} dB",
            MODELS[m],
            d - k,
            -k
        );
        if d.abs() >= 1.0 {
            off.push((MODELS[m], d));
        }
        assert_eq!(e.stats().unsolved, 0);
    }
    assert!(off.is_empty(), "off unity: {off:?}");
}

/// THD (dB) of a steady 200 Hz output over its second half, and its level (dB).
fn thd(y: &[f32], sr: f64) -> (f64, f64) {
    let from = y.len() / 2;
    let f = tone_amp(y, from, 200.0, sr);
    let h = (2..=15)
        .map(|k| tone_amp(y, from, 200.0 * k as f64, sr).powi(2))
        .sum::<f64>()
        .sqrt();
    (20.0 * (h / f).log10(), 20.0 * f.log10())
}

#[test]
fn cleanup_with_the_guitar_volume() {
    // `fuzz` 100 %, 200 Hz at an EMF of 50 mV, `guitar_vol` 100 → 50 → 30 %: on the germanium face
    // THD falls while the level moves little; at each position the buffered pedal is dirtier
    // (README §5: −7 → −12 → −17 dB, buffered −2 / −7 / −10 dB [P]). The EMF is fed as the DI that
    // the guitar would give at recording (the EMF filter's inverse is the netlist's own response).
    let sr = 48000.0;
    let g = gfx_dsp::pickup::SINGLE_COIL;
    let (re, im) = g.recording(200.0);
    let gain = re.hypot(im);
    let x = sine(200.0, 0.05 * gain, 48000, sr);
    let mut rows = Vec::new();
    for vol in [100.0, 50.0, 30.0] {
        let mut r = [(0.0, 0.0); 2];
        for (k, buffer) in [0.0, 1.0].iter().enumerate() {
            let mut e = fuzz(
                sr as f32,
                &[("fuzz", 100.0), ("guitar_vol", vol), ("buffer", *buffer)],
            );
            let y = run(&mut e, &x);
            r[k] = thd(&y, sr);
        }
        eprintln!(
            "Ge Pair vol {vol} %: THD {:.1} dB at {:.1} dB | buffered THD {:.1} dB at {:.1} dB",
            r[0].0, r[0].1, r[1].0, r[1].1
        );
        rows.push(r);
    }
    for k in 1..3 {
        assert!(
            rows[k][0].0 < rows[k - 1][0].0,
            "THD does not fall: {rows:?}"
        );
    }
    for r in &rows {
        assert!(
            r[1].0 >= r[0].0 + 4.0,
            "buffered not dirtier by 4 dB: {rows:?}"
        );
    }
}

#[test]
fn starved_supply_gates_and_sag_droops() {
    // `voltage` 4 V, `bias` −40 %, `sag` 40 % on the muff and the face: no unsolved sample; `sag`
    // 100 % pulls `supply_v` down on a hard note and it recovers
    let sr = 48000.0;
    let mut e = fuzz(sr as f32, &[("sag", 100.0), ("fuzz", 100.0)]);
    let rest = e.supply_v();
    let x = sine(110.0, 0.8, 9600, sr);
    let mut low = f32::MAX;
    for c in x.chunks(480) {
        let mut b = c.to_vec();
        e.process(&mut [&mut b]);
        low = low.min(e.supply_v());
    }
    let mut back = 0.0;
    for _ in 0..40 {
        let mut b = vec![0.0f32; 480];
        e.process(&mut [&mut b]);
        back = e.supply_v();
    }
    eprintln!("sag 100 %: supply {rest:.2} V at rest, {low:.2} V under a hard note, {back:.2} V 400 ms after");
    assert!(
        low < rest - 0.05 && (back - rest).abs() < 0.05,
        "{rest} {low} {back}"
    );
    for m in 0..N_MODELS {
        let mut e = fuzz(
            sr as f32,
            &[
                ("model", m as f32),
                ("voltage", 4.0),
                ("bias", -40.0),
                ("sag", 40.0),
            ],
        );
        let y = run(&mut e, &di_proxy(sr));
        assert!(y.iter().all(|v| v.is_finite()));
        assert_eq!(e.stats().unsolved, 0, "{}", MODELS[m]);
    }
}

#[test]
fn newton_solves_the_hard_inputs() {
    // +24 dB, DC, full-scale noise, the extremes of `voltage` and `temp`: no NaN, no unsolved sample
    let sr = 44100.0;
    let n = 22050;
    let inputs: Vec<Vec<f32>> = vec![sine(1000.0, 15.0, n, sr), vec![1.0; n], pink(n, 0.7, 3)];
    for m in 0..N_MODELS {
        for set in [
            vec![],
            vec![("voltage", 18.0)],
            vec![("voltage", 3.0)],
            vec![("temp", 0.0)],
            vec![("temp", 50.0), ("leakage", 400.0)],
            vec![("buffer", 1.0), ("fuzz", 100.0)],
        ] {
            for x in &inputs {
                let mut s: Vec<(&str, f32)> = vec![("model", m as f32)];
                s.extend(set.iter().cloned());
                let mut e = fuzz(sr as f32, &s);
                let y = run(&mut e, x);
                let st = e.stats();
                assert!(y.iter().all(|v| v.is_finite()), "{} {set:?}", MODELS[m]);
                assert_eq!(st.unsolved, 0, "{} {set:?}: {st:?}", MODELS[m]);
            }
        }
    }
}

#[test]
fn muff_stack_matches_the_laplace_network() {
    // each variant, `tone` −100 / 0 / +100 %, `mids` 0 / 100 %: the netlist's small-signal response
    // from the third collector's node is the stack's own (checked through the circuit's
    // linearization: the stack's scoop at noon is 3 – 12 dB below its 100 Hz level, smaller with mids)
    for stack in 0..4 {
        let mut dips = Vec::new();
        for mids in [0.0f64, 1.0] {
            let def = si_four(stack, &gfx_dsp::pickup::SINGLE_COIL, true);
            let mut nc = NonlinearCircuit::new(&def.net, 192000.0);
            nc.set_input(IN_VS, 9.0);
            nc.set_pot(def.tone.unwrap(), 0.5);
            nc.set_var(def.mids.unwrap(), (22e3 * mids).max(R_MIN));
            nc.set_pot(def.fuzz, 0.0);
            nc.init_dc();
            let db = |f: f64| {
                let z = nc.ac_response(f, IN_SIG, def.out);
                20.0 * z.0.hypot(z.1).log10()
            };
            // the response of the whole pedal at sustain 0 still carries the stack's shape
            let l100 = db(100.0);
            let dip = (8..=40)
                .map(|k| l100 - db(k as f64 * 100.0))
                .fold(f64::MIN, f64::max);
            dips.push(dip);
        }
        eprintln!(
            "{}: scoop {:.1} dB, with mids {:.1} dB",
            STACK_NAMES[stack], dips[0], dips[1]
        );
        assert!(dips[0] > 3.0 && dips[1] < dips[0], "{dips:?}");
    }
}

#[test]
fn deterministic() {
    let x = sine(220.0, 0.4, 9600, 48000.0);
    for m in 0..N_MODELS {
        let r = || {
            let mut e = fuzz(48000.0, &[("model", m as f32), ("fuzz", 90.0)]);
            run(&mut e, &x)
        };
        assert_eq!(r(), r());
    }
}

#[test]
fn switches_converge_and_stay_finite() {
    // a model, pickup, buffer and stack change under a sine: the fade completes, no NaN, no unsolved
    let sr = 48000.0;
    let mut e = fuzz(sr as f32, &[]);
    let x = sine(220.0, 0.3, 4800, sr);
    for (id, v) in [
        ("model", 1.0),
        ("stack", 3.0),
        ("pickup", 1.0),
        ("buffer", 1.0),
        ("model", 0.0),
        ("pickup", 3.0),
    ] {
        e.set_param(id, v);
        let y = run(&mut e, &x);
        assert!(y.iter().all(|v| v.is_finite()), "{id}");
        assert_eq!(e.stats().unsolved, 0, "{id}");
    }
}

/// Seeded pink noise at an RMS of `rms` (Kellet's filter on white noise, as gfx-bench's).
fn pink(n: usize, rms: f64, seed: u32) -> Vec<f32> {
    let mut s = seed;
    let mut b = [0.0f64; 7];
    let mut v: Vec<f64> = (0..n)
        .map(|_| {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            let w = (s >> 8) as f64 / 8388608.0 - 1.0;
            b[0] = 0.99886 * b[0] + w * 0.0555179;
            b[1] = 0.99332 * b[1] + w * 0.0750759;
            b[2] = 0.96900 * b[2] + w * 0.1538520;
            b[3] = 0.86650 * b[3] + w * 0.3104856;
            b[4] = 0.55000 * b[4] + w * 0.5329522;
            b[5] = -0.7616 * b[5] - w * 0.0168980;
            let y = b.iter().sum::<f64>() + w * 0.5362;
            b[6] = w * 0.115926;
            y
        })
        .collect();
    let e = (v.iter().map(|x| x * x).sum::<f64>() / n as f64).sqrt();
    v.iter_mut().for_each(|x| *x *= rms / e);
    v.into_iter().map(|x| x as f32).collect()
}

/// The best of five timings of `f` (ns per call of `n` calls): the machine is shared.
fn best_ns(n: usize, mut f: impl FnMut()) -> f64 {
    let mut best = f64::MAX;
    for _ in 0..5 {
        let t = std::time::Instant::now();
        f();
        best = best.min(t.elapsed().as_secs_f64() * 1e9 / n as f64);
    }
    best
}

#[test]
#[ignore = "diagnostics: the solver alone"]
fn cpu_solver() {
    let _ftz = gfx_dsp::util::FtzGuard::new();
    for cfg in [
        Config {
            model: 0,
            pickup: 0,
            buffer: false,
            stack: 0,
        },
        Config {
            model: 2,
            pickup: 0,
            buffer: false,
            stack: 0,
        },
    ] {
        let def = build(cfg);
        let mut nc = NonlinearCircuit::new(&def.net, 384000.0);
        nc.set_fast_solve(std::env::var("SLOW").is_err());
        nc.set_input(IN_VS, def.supply_sign * 9.0);
        nc.set_pot(def.fuzz, 0.7);
        nc.init_dc();
        let n = 38400;
        let amp: f64 = std::env::var("AMP").map_or(0.1, |a| a.parse().unwrap());
        let mut k = 0usize;
        let ns = best_ns(n, || {
            for _ in 0..n {
                nc.set_input(IN_SIG, amp * (2.0 * PI * 200.0 * k as f64 / 384000.0).sin());
                nc.process();
                k += 1;
            }
        });
        let s = nc.stats();
        eprintln!(
            "{cfg:?}: {ns:.0} ns / solve, mean {:.2} it, {:.1} exps/sample, ports {:?}",
            s.mean(),
            s.exps as f64 / s.samples as f64,
            nc.ports()
        );
    }
}

#[test]
#[ignore = "diagnostics: the oversampler's cost"]
fn cpu_oversampler() {
    for st in [2usize, 3, 4] {
        let mut os = OversamplerN::new(st);
        let mut acc = 0.0;
        let n = 100_000;
        let id = best_ns(n, || {
            for k in 0..n {
                acc += os.identity((k as f64 * 0.01).sin());
            }
        });
        let ud = best_ns(n, || {
            for k in 0..n {
                let mut b = [0.0; MAX_FACTOR];
                os.up((k as f64 * 0.01).sin(), &mut b);
                acc += os.down(&mut b);
            }
        });
        eprintln!(
            "{}×: identity {id:.0} ns, up + down {ud:.0} ns ({acc:.1})",
            1 << st
        );
    }
}

#[test]
#[ignore = "diagnostics: a parameter step under a 1 kHz sine (ID, FROM, TO, SR)"]
fn step_probe() {
    let id = std::env::var("ID").unwrap_or("input".into());
    let from: f32 = std::env::var("FROM").map_or(12.0, |v| v.parse().unwrap());
    let to: f32 = std::env::var("TO").map_or(-12.0, |v| v.parse().unwrap());
    let sr: f64 = std::env::var("SR").map_or(96000.0, |v| v.parse().unwrap());
    let mut e = fuzz(sr as f32, &[(id.as_str(), from)]);
    let n = sr as usize;
    let x = sine(1000.0, 0.25, n, sr);
    let (a, b) = x.split_at(n / 2);
    let mut y = run(&mut e, a);
    e.set_param(&id, to);
    y.extend(run(&mut e, b));
    // the output's peak and its energy above 10 kHz per 10 ms
    let mut hp = HighPass::default();
    hp.set(10000.0, sr);
    let h: Vec<f64> = y.iter().map(|&v| hp.process(v as f64)).collect();
    let w = (sr * 0.01) as usize;
    for k in (n / 2 - 2 * w..n / 2 + 15 * w).step_by(w) {
        let pk = h[k..k + w].iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let py = y[k..k + w].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        eprintln!(
            "{:>5.0} ms: HF {:6.1} dB, peak {:6.1} dB",
            (k as f64 - n as f64 / 2.0) / sr * 1e3,
            20.0 * pk.log10(),
            20.0 * (py as f64).log10()
        );
    }
    eprintln!("{:?}", e.stats());
}

#[test]
#[ignore = "diagnostics: prepare"]
fn prepare_time() {
    let t = std::time::Instant::now();
    let _e = fuzz(48000.0, &[]);
    eprintln!("prepare: {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
}

#[test]
#[ignore = "diagnostics: CPU per model"]
fn cpu_models() {
    let sr = 48000.0;
    let x = pink(
        24000,
        std::env::var("AMP").map_or(0.1, |a| a.parse().unwrap()),
        1,
    );
    for m in 0..N_MODELS {
        for buffer in [0.0, 1.0] {
            let mut e = fuzz(sr, &[("model", m as f32), ("buffer", buffer)]);
            let ns = best_ns(x.len(), || {
                let _ = run(&mut e, &x);
            });
            let s = e.stats();
            eprintln!(
                "{:<8} buffer {buffer}: {ns:.0} ns / sample; mean iterations {:.2}, worst {}, fallbacks {}, unsolved {}",
                MODELS[m],
                s.mean(),
                s.worst,
                s.fallbacks,
                s.unsolved
            );
        }
    }
}

// ---- linked mono (README, As coded): L == R runs channel 0 alone

/// A stereo instance (`no_link`: true stereo always, the reference).
fn fuzz2(set: &[(&str, f32)], no_link: bool) -> Fuzz {
    let mut e = Fuzz::new();
    for (k, v) in set {
        e.set_param(k, *v);
    }
    e.no_link = no_link;
    e.prepare(48000.0, 4096, 2);
    e
}

/// Renders (l, r) in blocks of `block`; each event (sample, id, value) lands on a block start.
fn run_pair(
    e: &mut Fuzz,
    l: &[f32],
    r: &[f32],
    block: usize,
    ev: &[(usize, &str, f32)],
) -> [Vec<f32>; 2] {
    let (mut a, mut b) = (l.to_vec(), r.to_vec());
    let mut i = 0;
    while i < a.len() {
        for &(t, id, v) in ev {
            assert!(t % block == 0, "event {t} off the block grid {block}");
            if t == i {
                e.set_param(id, v);
            }
        }
        let m = block.min(a.len() - i);
        e.process(&mut [&mut a[i..i + m], &mut b[i..i + m]]);
        i += m;
    }
    [a, b]
}

/// The link test stream's right channel: L on [0, 6000) and [12000, 20000) (long enough to relink:
/// 60 ms hold + 30 ms fade), L + `d` elsewhere (a different input: the unlinks).
fn link_right(l: &[f32], d: &[f32]) -> Vec<f32> {
    (0..l.len())
        .map(|i| {
            if (6000..12000).contains(&i) || i >= 20000 {
                l[i] + d[i]
            } else {
                l[i]
            }
        })
        .collect()
}

/// A knob and a model change on both sides of the relink; the unlink at 20000 falls inside the
/// model's settle + fade (16384 + 90 ms).
const LINK_EVENTS: [(usize, &str, f32); 3] = [
    (4096, "fuzz", 40.0),
    (8192, "model", 1.0),
    (16384, "model", 0.0),
];

#[test]
fn linked_stereo_is_the_mono_output() {
    let x = pink(24576, 0.1, 3);
    let mut m = fuzz(48000.0, &[]);
    let mut y = x.clone();
    for (k, c) in y.chunks_mut(512).enumerate() {
        for &(t, id, v) in &LINK_EVENTS {
            if t == 512 * k {
                m.set_param(id, v);
            }
        }
        m.process(&mut [c]);
    }
    let [l, r] = run_pair(&mut fuzz2(&[], false), &x, &x, 512, &LINK_EVENTS);
    assert!(l == y && r == y, "L == R stereo differs from mono");
}

#[test]
fn link_switches_are_exact_and_click_free() {
    let sr = 48000.0;
    let n = 26112;
    let l = sine(1000.0, 0.25, n, sr);
    let r = link_right(&l, &pink(n, 1e-3, 5));
    let [l1, r1] = run_pair(&mut fuzz2(&[], false), &l, &r, 512, &LINK_EVENTS);
    let [l0, r0] = run_pair(&mut fuzz2(&[], true), &l, &r, 512, &LINK_EVENTS);
    // channel 0 never changes; channel 1 is true stereo, bit for bit, until the relink fade starts
    // (the 2880th sample of L == R)
    let relink = 12000 + 2880 - 1;
    assert!(l1 == l0, "channel 0 moved");
    let first = (0..relink).find(|&i| r1[i] != r0[i]);
    assert!(
        first.is_none(),
        "the unlink is not exact from {first:?}: {:?}",
        first.map(|i| (r1[i], r0[i]))
    );
    // the transitions: no click by the bench's metric (HF > 10 kHz in the 100 ms after, minus
    // channel 0's, re peak; bound −20 dB as the bench's) nor a sample step above channel 0's own
    let hp = |y: &[f32]| {
        let mut f1 = gfx_dsp::svf::Svf::new();
        let mut f2 = gfx_dsp::svf::Svf::new();
        f1.set(
            gfx_dsp::svf::SvfMode::HighPass,
            10000.0,
            0.541_196,
            0.0,
            sr as f32,
        );
        f2.set(
            gfx_dsp::svf::SvfMode::HighPass,
            10000.0,
            1.306_563,
            0.0,
            sr as f32,
        );
        y.iter()
            .map(|&v| f2.process(f1.process(v)))
            .collect::<Vec<f32>>()
    };
    let pk = |s: &[f32]| s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let step = |s: &[f32]| s.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
    let (h0, h1) = (hp(&l1), hp(&r1));
    let peak = pk(&l1);
    for t in [6000, 12000, relink, 20000] {
        let w = t - 480..t + 4800;
        let burst = 20.0 * ((pk(&h1[w.clone()]) - pk(&h0[w.clone()])).max(1e-9) / peak).log10();
        let (s1, s0) = (step(&r1[w.clone()]), step(&l1[w.clone()]));
        eprintln!(
            "link transition at {t}: HF burst {burst:.0} dB re peak, max step {s1:.4} (L {s0:.4})"
        );
        assert!(burst <= -20.0, "{t}: {burst} dB");
        assert!(s1 <= 1.05 * s0, "{t}: step {s1} vs {s0}");
    }
}

#[test]
fn linked_mono_is_block_size_invariant() {
    let n = 24576;
    let l = pink(n, 0.1, 3);
    let r = link_right(&l, &pink(n, 0.05, 4));
    let reference = run_pair(&mut fuzz2(&[], false), &l, &r, 512, &LINK_EVENTS);
    for b in [1usize, 64, 4096] {
        assert!(
            run_pair(&mut fuzz2(&[], false), &l, &r, b, &LINK_EVENTS) == reference,
            "block {b}"
        );
    }
}

// ---- the octave engine (gfx-octafuzz) must not move a bit of the shipped models

/// FNV-1a over the f32 bits of every sample.
fn fnv(h: &mut u64, y: &[f32]) {
    for v in y {
        for b in v.to_bits().to_le_bytes() {
            *h ^= b as u64;
            *h = h.wrapping_mul(0x100000001b3);
        }
    }
}

/// Renders of v0.1.0 + linked mono (before gfx-octafuzz's engine was added): mono per model ×
/// buffer with knob, temperature, voltage, sag, mix, bass-keep and model changes on the way; stereo
/// L ≠ R; 96 kHz.
fn reference_renders() -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    let x = pink(14400, 0.1, 7);
    let ev: [(usize, &str, f32); 8] = [
        (1536, "fuzz", 95.0),
        (3072, "tone", -40.0),
        (4608, "bias", 35.0),
        (6144, "voltage", 6.0),
        (7680, "guitar_vol", 55.0),
        (9216, "sag", 60.0),
        (10752, "mix", 70.0),
        (12288, "bass_keep", 120.0),
    ];
    for m in 0..N_MODELS {
        for buffer in [0.0, 1.0] {
            let mut e = fuzz(
                48000.0,
                &[("model", m as f32), ("buffer", buffer), ("temp", 30.0)],
            );
            let mut y = x.clone();
            for (k, c) in y.chunks_mut(512).enumerate() {
                for &(t, id, v) in &ev {
                    if t == 512 * k {
                        e.set_param(id, v);
                    }
                }
                if k == 20 {
                    e.set_param("model", (1 - m) as f32);
                }
                e.process(&mut [c]);
            }
            fnv(&mut h, &y);
        }
    }
    let d = pink(14400, 0.02, 9);
    let r: Vec<f32> = x.iter().zip(&d).map(|(a, b)| a + b).collect();
    let [l, r] = run_pair(&mut fuzz2(&[], false), &x, &r, 512, &LINK_EVENTS);
    fnv(&mut h, &l);
    fnv(&mut h, &r);
    let mut e = fuzz(96000.0, &[("fuzz", 100.0)]);
    let y = run(&mut e, &x);
    fnv(&mut h, &y);
    h
}

#[test]
fn shipped_renders_are_bit_identical() {
    let h = reference_renders();
    eprintln!("renders: {h:#018x}");
    assert_eq!(h, RENDERS_V010);
}

const RENDERS_V010: u64 = 0xfab5_9003_ec20_a70a;

// ---- Octave Up (gfx-octafuzz's engine; its behaviour rows live in gfx-octafuzz's tests)

/// Octave Up's DC point at `volts`, `bias` (−1 … 1), `celsius`: the named node voltages.
fn octave_dc(volts: f64, bias: f64, celsius: f64) -> Vec<(&'static str, f64)> {
    let mut def = octave_up(&SINGLE_COIL, true);
    let probes: Vec<(&str, usize)> = def
        .nodes
        .clone()
        .into_iter()
        .map(|(k, n)| (k, def.net.probe(n)))
        .collect();
    let mut nc = NonlinearCircuit::new(&def.net, 352800.0);
    nc.set_temperature(celsius);
    nc.set_input(IN_VS, volts);
    nc.set_pot(def.fuzz, 0.7);
    nc.set_var(def.bias.0, R2_OCT * bias.exp2());
    assert!(nc.init_dc(), "DC point");
    probes.into_iter().map(|(k, o)| (k, nc.probe(o))).collect()
}

#[test]
fn octave_up_dc_point() {
    // README §5 "Octave Up DC": 9.26 V, within 0.15 V of the prototype's 4.32 / 3.77 / 2.91, 2.33,
    // 4.99 / 1.67 V; `bias` ±100 % at 9 V: 6.28 / 4.04 V ± 0.2 V
    let want = [4.32, 3.77, 2.91, 2.33, 4.99, 1.67];
    let got = octave_dc(9.26, 0.0, 25.0);
    for ((k, v), w) in got.iter().zip(want) {
        eprintln!("{k}: {v:.3} V (prototype {w})");
        assert!((v - w).abs() < 0.15, "{k}: {v}");
    }
    for (b, w) in [
        (1.0, 6.28),
        (0.5, 5.58),
        (0.0, 4.87),
        (-0.5, 4.22),
        (-1.0, 4.04),
    ] {
        let c3 = octave_dc(9.0, b, 25.0)[4].1;
        eprintln!("bias {b:+}: Q3 C {c3:.2} V (prototype {w})");
        assert!((c3 - w).abs() < 0.2, "{b}: {c3}");
    }
    for (t, w) in [(15.0, 4.90), (35.0, 4.85)] {
        let c3 = octave_dc(9.0, 0.0, t)[4].1;
        eprintln!("{t} °C: Q3 C {c3:.2} V (prototype {w})");
        assert!((c3 - w).abs() < 0.1, "{t}: {c3}");
    }
    for (v, w) in [(4.5, 2.80), (18.0, 9.00)] {
        let c3 = octave_dc(v, 0.0, 25.0)[4].1;
        eprintln!("{v} V: Q3 C {c3:.2} V (prototype {w})");
        assert!((c3 - w).abs() < 0.15, "{v}: {c3}");
    }
}

#[test]
#[ignore = "diagnostics (O3, measured first): the Octave Up netlist alone at 8×, CPU per host sample"]
fn cpu_octave_netlist() {
    // README §2.8 decided by Raph (O3): the netlist measured first. 48 kHz host, 8× = 384 kHz, mono;
    // pink noise at 0.1 RMS (the bench's, held over 8) and a 1.2 kHz sine at −12 dBFS, `fuzz` 70 /
    // 100 %, buffer off / on. FAST=0 / QUAD=0 turn the cheap steps off
    let _ftz = gfx_dsp::util::FtzGuard::new();
    let fs = 384000.0;
    let fast = std::env::var("FAST").map_or(true, |v| v != "0");
    let quad = std::env::var("QUAD").map_or(true, |v| v != "0");
    let n = 38400;
    let pk = pink(n / 8 + 64, 0.1, 1);
    for buffer in [false, true] {
        for fz in [0.7, 1.0] {
            for sig in ["pink", "sine", "silence"] {
                let fwd = std::env::var("FWD").map_or(true, |v| v != "0");
                // STAGE2=0: stage 1 alone (the netlist stops at the tap)
                let st2 = std::env::var("STAGE2").map_or(true, |v| v != "0");
                let def = octave_up_with(&SINGLE_COIL, buffer, fwd, st2);
                let mut nc = NonlinearCircuit::new(&def.net, fs);
                nc.set_fast_solve(fast);
                nc.set_quadratic_seed(quad);
                nc.set_input(IN_VS, 9.0);
                nc.set_pot(def.fuzz, fz);
                nc.init_dc();
                let mut emf = SINGLE_COIL.emf_filter(fs, gfx_dsp::pickup::EMF_POLE_HZ);
                let mut k = 0usize;
                let mut worst_block: f64 = 0.0;
                let ns = best_ns(n, || {
                    for b in 0..n / 512 {
                        let t = std::time::Instant::now();
                        for _ in 0..512 {
                            let x = if sig == "pink" {
                                pk[k / 8] as f64
                            } else if sig == "silence" {
                                0.0
                            } else {
                                0.25 * (2.0 * PI * 1200.0 * k as f64 / fs).sin()
                            };
                            nc.set_input(IN_SIG, if buffer { x } else { emf.process(x) });
                            nc.process();
                            k += 1;
                        }
                        if b > 0 {
                            worst_block = worst_block.max(t.elapsed().as_secs_f64() * 1e9 / 512.0);
                        }
                    }
                    k = 0;
                });
                let s = nc.stats();
                let ns_ = nc.stage_count();
                eprintln!(
                    "buffer {buffer:5} fuzz {fz} {sig}: {:.2} µs / host sample ({ns:.0} ns / solve), worst block {:.2} µs; mean it {:.2} (last stages {:.2}, {:.2}), worst {}, fallbacks {}, unsolved {}, ports {:?}",
                    8.0 * ns / 1000.0,
                    8.0 * worst_block / 1000.0,
                    s.mean(),
                    nc.stage_stats(ns_.saturating_sub(2)).mean(),
                    nc.stage_stats(ns_ - 1).mean(),
                    s.worst,
                    s.fallbacks,
                    s.unsolved,
                    nc.ports()
                );
            }
        }
    }
}

/// The tap s1 of a render of Octave Up's stage 1 at `fs`: a sine at `f`, `amp` V on the DI,
/// `buffer` on, `fuzz` 100 %, `q2_forward` or not; and the circuit's own stage-2 output when
/// `stage2`. Returns (s1, out) per sample after `settle` seconds.
fn octave_render(fs: f64, f: f64, amp: f64, q2_forward: bool, secs: f64) -> (Vec<f64>, Vec<f64>) {
    let def = octave_up_with(&SINGLE_COIL, true, q2_forward, true);
    let mut nc = NonlinearCircuit::new(&def.net, fs);
    nc.set_fast_solve(true);
    nc.set_quadratic_seed(true);
    nc.set_input(IN_VS, 9.0);
    nc.set_pot(def.fuzz, 1.0);
    nc.init_dc();
    let n = (secs * fs) as usize;
    let tap = def.tap.unwrap();
    let mut s1 = Vec::with_capacity(n);
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        nc.set_input(IN_SIG, amp * (2.0 * PI * f * k as f64 / fs).sin());
        nc.process();
        s1.push(nc.output(tap));
        out.push(nc.output(def.out));
    }
    assert_eq!(nc.stats().unsolved, 0);
    (s1, out)
}

#[test]
fn octave_up_q2_forward_matches_the_full_bjt() {
    // way 2 (README §2.8): Q2 forward-only against the full Ebers–Moll Q2, 440 Hz at 0.3 V and 82 Hz
    // at 1 V, `fuzz` 100 %, buffer on: the outputs within −60 dB of their peak (the reverse
    // junction's fA dropped); Q2's base-collector voltage stays below −3 V on the full BJT
    for (f, a) in [(440.0, 0.3), (82.4, 1.0)] {
        let (_, o1) = octave_render(384000.0, f, a, true, 0.1);
        let (_, o0) = octave_render(384000.0, f, a, false, 0.1);
        let pk = o0.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let d = o1
            .iter()
            .zip(&o0)
            .fold(0.0f64, |m, (x, y)| m.max((x - y).abs()));
        eprintln!(
            "{f} Hz {a} V: forward-only Q2 vs full: {:.1} dB re peak",
            20.0 * (d / pk).log10()
        );
        assert!(d < 1e-3 * pk, "{d} vs {pk}");
    }
}

#[test]
fn octave_up_voltage_step_without_the_forward_q2() {
    // README §5 Octave Up DC: `voltage` stepped from 18 to 3 V at full speed (here instantly, the bare
    // circuit) under a 196 Hz note at 0.3 V, `fuzz` 100 %: no unsolved sample, with and without the
    // forward-only Q2
    for fwd in [true, false] {
        let fs = 384000.0;
        let def = octave_up_with(&SINGLE_COIL, true, fwd, true);
        let mut nc = NonlinearCircuit::new(&def.net, fs);
        nc.set_fast_solve(true);
        nc.set_quadratic_seed(true);
        nc.set_input(IN_VS, 18.0);
        nc.set_pot(def.fuzz, 1.0);
        nc.init_dc();
        for k in 0..(0.2 * fs) as usize {
            if k == (0.05 * fs) as usize {
                nc.set_input(IN_VS, 3.0);
            }
            nc.set_input(IN_SIG, 0.3 * (2.0 * PI * 196.0 * k as f64 / fs).sin());
            nc.process();
            assert!(nc.output(def.out).is_finite());
        }
        let s = nc.stats();
        eprintln!(
            "forward-only Q2 {fwd}: worst {}, fallbacks {}, unsolved {}",
            s.worst, s.fallbacks, s.unsolved
        );
        assert_eq!(s.unsolved, 0, "{fwd}");
    }
}

#[test]
fn octave_up_flux_linkage() {
    // README §5 Octave Up DC, the saturation diagnostic (L_m = 1 H): the winding's peak flux linkage
    // L_m · i = ∫ v(L_m) dt at 41, 82 and 196 Hz, 0.3 V, `fuzz` 100 % [P 13.3 / 8.8 / 4.2 mWb against
    // 8.25 rated]; reported, and within 25 % of the prototype's
    let fs = 384000.0;
    let rated = 11.0 * 2f64.sqrt() / (2.0 * PI * 300.0);
    for (f, want) in [(41.2, 13.3e-3), (82.4, 8.8e-3), (196.0, 4.2e-3)] {
        let mut def = octave_up_with(&SINGLE_COIL, true, true, false);
        let wl = def.nodes.iter().find(|n| n.0 == "winding_lm").unwrap().1;
        let pw = def.net.probe(wl);
        let mut nc = NonlinearCircuit::new(&def.net, fs);
        nc.set_fast_solve(true);
        nc.set_input(IN_VS, 9.0);
        nc.set_pot(def.fuzz, 1.0);
        nc.init_dc();
        let n = (0.4 * fs) as usize;
        let (mut lam, mut v1) = (0.0, 0.0);
        let mut tail = Vec::new();
        for k in 0..n {
            nc.set_input(IN_SIG, 0.3 * (2.0 * PI * f * k as f64 / fs).sin());
            nc.process();
            let v = nc.probe(pw);
            lam += 0.5 * (v + v1) / fs;
            v1 = v;
            if k > n / 2 {
                tail.push(lam);
            }
        }
        // in steady state the winding's mean current is the 33 µF's, zero: λ = ∫ v dt less its mean
        let mean = tail.iter().sum::<f64>() / tail.len() as f64;
        let pk = tail.iter().fold(0.0f64, |m, l| m.max((l - mean).abs()));
        eprintln!(
            "{f} Hz, 0.3 V: peak flux linkage {:.1} mWb ({:.2} × rated {:.2} mWb; prototype {:.1})",
            pk * 1e3,
            pk / rated,
            rated * 1e3,
            want * 1e3
        );
        assert!((pk / want - 1.0).abs() < 0.25, "{f}: {pk}");
    }
}
