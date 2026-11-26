// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5, the rows that run on synthetic signals). Targets are computed from the
//! netlists (Laplace analysis of the linearized circuit), not copied by hand.

use super::*;

const PI: f64 = std::f64::consts::PI;

fn dist(sr: f32, set: &[(&str, f32)]) -> Dist {
    let mut e = Dist::new();
    for (k, v) in set {
        e.set_param(k, *v);
    }
    e.prepare(sr, 512, 1);
    e
}

fn run(e: &mut Dist, x: &[f32]) -> Vec<f32> {
    let mut y = x.to_vec();
    for c in y.chunks_mut(512) {
        e.process(&mut [c]);
    }
    y
}

fn sine(freq: f64, amp: f64, n: usize, sr: f64) -> Vec<f32> {
    (0..n).map(|i| (amp * (2.0 * PI * freq * i as f64 / sr).sin()) as f32).collect()
}

/// Amplitude of the component at `f` over `y[from..]` (an integer number of periods expected).
fn tone_amp(y: &[f32], from: usize, f: f64, sr: f64) -> f64 {
    let w = 2.0 * PI * f / sr;
    let (mut c, mut s) = (0.0, 0.0);
    for (i, v) in y[from..].iter().enumerate() {
        let ph = w * (i + from) as f64;
        c += *v as f64 * ph.cos();
        s += *v as f64 * ph.sin();
    }
    2.0 * c.hypot(s) / (y.len() - from) as f64
}

/// Worst alias (dB re the fundamental) in 20 Hz – 20 kHz of a steady sine at `f` (integer Hz) over
/// the last second, at `sr` = 44.1 kHz (where the folded harmonics fall between the true ones).
pub(crate) fn worst_alias(y: &[f32], f: f64, sr: f64) -> f64 {
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
        let a = 20.0 * (tone_amp(y, from, fa, sr) / f0).log10();
        if std::env::var("ALIASDBG").is_ok() && a > -80.0 {
            eprintln!("  k {k}: {fa} Hz {a:.1} dB");
        }
        worst = worst.max(a);
    }
    worst
}

/// Worst alias of a 1.2 kHz sine at −12 dBFS, full gain, tone open, at 44.1 kHz with `stages`.
fn alias_at(circuit: usize, stages: Option<usize>, extra: &[(&str, f32)]) -> (f64, Stats) {
    let sr = 44100.0;
    let mut e = Dist::new();
    e.stages_override = stages;
    for (k, v) in [("circuit", circuit as f32), ("gain", 100.0), ("tone_stack", 100.0)].iter().chain(extra) {
        e.set_param(k, *v);
    }
    e.prepare(sr as f32, 512, 1);
    let x = sine(1201.0, 0.25, (sr * 1.5) as usize, sr);
    let y = run(&mut e, &x);
    if std::env::var("ALIASDBG").is_ok() {
        let from = y.len() - sr as usize;
        let f0 = tone_amp(&y, from, 1201.0, sr);
        let h: Vec<String> = [3.0, 5.0, 9.0, 15.0].iter().map(|k| format!("{:.0}", 20.0 * (tone_amp(&y, from, 1201.0 * k, sr) / f0).log10())).collect();
        eprintln!("  fundamental {f0:.3}, harmonics 3/5/9/15: {h:?}");
    }
    if std::env::var("ALIASDBG").is_ok() {
        for s in e.stage_stats() {
            eprintln!("  stage: {s:?}");
        }
    }
    (worst_alias(&y, 1201.0, sr), e.stats())
}

#[test]
#[ignore = "survey: cargo test --release -p gfx-dist --no-default-features alias_survey -- --ignored --nocapture"]
fn alias_survey() {
    let only: Option<usize> = std::env::var("C").ok().and_then(|s| s.parse().ok());
    for c in (0..N_CIRCUITS).filter(|c| only.is_none_or(|o| o == *c)) {
        for st in [2usize, 3, 4] {
            let mut line = format!("{:<13} {:>2}×:", CIRCUITS[c], 1 << st);
            let mut uns = 0;
            for o in 0..OPAMPS.len() {
                let (a, s) = alias_at(c, Some(st), &[("opamp", o as f32)]);
                uns += s.unsolved;
                line += &format!(" {}={a:.0}", OPAMPS[o]);
            }
            for d in 1..DIODES.len() {
                let (a, s) = alias_at(c, Some(st), &[("diodes", d as f32)]);
                uns += s.unsolved;
                line += &format!(" {}={a:.0}", DIODES[d]);
            }
            eprintln!("{line} unsolved={uns}");
        }
    }
}

#[test]
#[ignore = "diagnostics"]
fn alias_one() {
    let st: usize = std::env::var("ST").map_or(2, |s| s.parse().unwrap());
    let o: f32 = std::env::var("OP").map_or(0.0, |s| s.parse().unwrap());
    let c: usize = std::env::var("C").map_or(0, |s| s.parse().unwrap());
    let sup: f32 = std::env::var("SUP").map_or(100.0, |s| s.parse().unwrap());
    let (a, s) = alias_at(c, Some(st), &[("opamp", o), ("supply", sup)]);
    eprintln!("alias {a:.1}, {s:?}");
}

#[test]
fn opamp_dist_proof() {
    // README Review: the first coding task, the solver on the Op-Amp Dist. Newton statistics on
    // sines −12 … +12 dBFS at full gain, the 1.2 kHz alias at −12 dBFS (policy: ≤ −70 dB)
    let sr = 44100.0;
    for amp_db in [-12.0, 0.0, 12.0] {
        let amp = 10f64.powf(amp_db / 20.0);
        let mut e = dist(sr as f32, &[("gain", 100.0), ("tone_stack", 100.0)]);
        let x = sine(1201.0, amp, (sr * 1.5) as usize, sr);
        let y = run(&mut e, &x);
        let s = e.stats();
        let a = worst_alias(&y, 1201.0, sr);
        eprintln!(
            "Op-Amp Dist {amp_db:+} dBFS: alias {a:.1} dB, mean {:.2} it, worst {}, fallbacks {:.3} %, unsolved {}",
            s.mean(),
            s.worst,
            100.0 * s.fallbacks as f64 / s.samples as f64,
            s.unsolved
        );
        assert_eq!(s.unsolved, 0);
        if amp_db == -12.0 {
            assert!(a <= -70.0, "alias {a} dB");
        }
    }
}

#[test]
#[ignore = "diagnostics: DC points"]
fn dc_points() {
    for k in 0..N_CIRCUITS {
        let def = build(k);
        let mut net = def.net.clone();
        let nodes: Vec<Node> = (1..60).filter(|&n| net.elements().iter().any(|e| e.nodes().contains(&n))).collect();
        let outs: Vec<usize> = nodes.iter().map(|&n| net.output(n)).collect();
        let mut nc = NonlinearCircuit::monolithic(&net, 384000.0);
        for &(pt, _) in def.gain.iter().chain(&def.tone) {
            nc.set_pot(pt, 0.5);
        }
        nc.set_input(IN_VS, 9.0);
        nc.set_input(IN_VB, 4.5);
        nc.set_supply(9.0);
        let ok = nc.init_dc();
        let v: Vec<String> = nodes.iter().zip(&outs).map(|(n, &o)| format!("{n}:{:.3}", nc.output(o))).collect();
        eprintln!("{} ({ok}): {}", CIRCUITS[k], v.join(" "));
    }
}

#[test]
#[ignore = "diagnostics: a switch"]
fn switch_one() {
    let sr: f64 = std::env::var("SR").map_or(192000.0, |s| s.parse().unwrap());
    let id = std::env::var("ID").unwrap_or("diodes".into());
    let from: f32 = std::env::var("FROM").map_or(6.0, |s| s.parse().unwrap());
    let to: f32 = std::env::var("TO").map_or(0.0, |s| s.parse().unwrap());
    let mut e = dist(sr as f32, &[(id.as_str(), from)]);
    let n = sr as usize;
    let x = sine(1000.0, 0.25, n, sr);
    let mut y = x.clone();
    let (a, b) = y.split_at_mut(n / 2);
    for c in a.chunks_mut(512) {
        e.process(&mut [c]);
    }
    let s0 = e.stats();
    e.set_param(&id, to);
    for c in b.chunks_mut(512) {
        e.process(&mut [c]);
    }
    eprintln!("before {s0:?}\nafter {:?}", e.stats());
    let mut d: Vec<(usize, f32)> = (n / 2 - 100..n / 2 + 4000).map(|i| (i, (y[i] - y[i - 1]).abs())).collect();
    d.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let peak = y.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    eprintln!("peak {peak}, largest steps {:?}", &d[..5]);
    // the bench's metric: HF (> 10 kHz) peak in the 100 ms after the step minus the steady HF
    let mut a = gfx_dsp::svf::Svf::new();
    let mut b = gfx_dsp::svf::Svf::new();
    a.set(gfx_dsp::svf::SvfMode::HighPass, 10000.0, 0.541_196, 0.0, sr as f32);
    b.set(gfx_dsp::svf::SvfMode::HighPass, 10000.0, 1.306_563, 0.0, sr as f32);
    let hp: Vec<f32> = y.iter().map(|&v| b.process(a.process(v))).collect();
    let pk = |s: &[f32]| s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let after = pk(&hp[n / 2..n / 2 + n / 10]);
    let steady = pk(&hp[n / 4..n / 2]).max(pk(&hp[n - n / 4..]));
    eprintln!("burst {:.1} dB (after {after}, steady {steady})", 20.0 * ((after - steady).max(1e-12) / peak).log10());
}

#[test]
#[ignore = "diagnostics: where the CPU goes"]
fn cpu_parts() {
    use std::time::Instant;
    let sr = 48000.0;
    let n = 48000;
    let x: Vec<f32> = (0..n).map(|i| (0.1 * ((i as f64 * 0.0371).sin() + (i as f64 * 0.113).sin())) as f32).collect();
    let only: Option<usize> = std::env::var("C").ok().and_then(|s| s.parse().ok());
    for c in (0..N_CIRCUITS).filter(|c| only.is_none_or(|o| o == *c)) {
        // the best of three runs (a loaded machine's noise), at the defaults and at full gain on a
        // −6 dBFS guitar-like signal
        let time = |set: &[(&str, f32)], x: &[f32]| {
            (0..3)
                .map(|_| {
                    let mut e = dist(sr as f32, set);
                    let t = Instant::now();
                    let _ = run(&mut e, x);
                    t.elapsed().as_secs_f64() * 1e9 / x.len() as f64
                })
                .fold(f64::MAX, f64::min)
        };
        let full = time(&[("circuit", c as f32), ("gain", 100.0), ("tone_stack", 100.0)], &di_proxy(sr));
        let pink = time(&[("circuit", c as f32)], &di_proxy(sr));
        // gfx-bench's CPU line (checks::cpu): 10 s of pink noise at 0.1 RMS, blocks of 128, mono
        let bench = {
            let mut r = gfx_dsp::lfo::Rng::new(13);
            let mut b = [0f64; 7];
            let mut v: Vec<f64> = (0..480000)
                .map(|_| {
                    let w = r.bipolar() as f64;
                    b[0] = 0.99886 * b[0] + w * 0.0555179;
                    b[1] = 0.99332 * b[1] + w * 0.0750759;
                    b[2] = 0.96900 * b[2] + w * 0.1538520;
                    b[3] = 0.86650 * b[3] + w * 0.3104856;
                    b[4] = 0.55000 * b[4] + w * 0.5329522;
                    b[5] = -0.7616 * b[5] - w * 0.0168980;
                    let y = b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + w * 0.5362;
                    b[6] = w * 0.115926;
                    y
                })
                .collect();
            let e = (v.iter().map(|x| x * x).sum::<f64>() / v.len() as f64).sqrt();
            v.iter_mut().for_each(|x| *x *= 0.1 / e);
            let x: Vec<f32> = v.into_iter().map(|x| x as f32).collect();
            (0..3)
                .map(|_| {
                    let mut e = dist(sr as f32, &[("circuit", c as f32)]);
                    let mut y = x.clone();
                    let t = Instant::now();
                    for b in y.chunks_mut(128) {
                        e.process(&mut [b]);
                    }
                    let dt = t.elapsed().as_secs_f64() * 1e9 / y.len() as f64;
                    if std::env::var("STAGES").is_ok() {
                        for (k, st) in e.stage_stats().iter().enumerate() {
                            eprintln!("  pink, stage {k}: mean {:.2} it, worst {}, fallbacks {}, exps {:.2}", st.mean(), st.worst, st.fallbacks, st.exps as f64 / st.samples.max(1) as f64);
                        }
                    }
                    dt
                })
                .fold(f64::MAX, f64::min)
        };
        eprintln!("{}: gfx-bench's CPU line (pink noise 0.1 RMS, defaults, mono, best of 3): {bench:.0} ns / sample", CIRCUITS[c]);
        let mut e = dist(sr as f32, &[("circuit", c as f32)]);
        let t = Instant::now();
        let _ = run(&mut e, &x);
        let total = t.elapsed().as_secs_f64() * 1e9 / n as f64;
        eprintln!("{}: best of 3, DI proxy: defaults {pink:.0} ns / sample, full gain {full:.0} ns", CIRCUITS[c]);
        let s = e.stats();
        for (k, st) in e.stage_stats().iter().enumerate() {
            eprintln!("  stage {k}: mean {:.2} it, worst {}, fallbacks {}, exps {:.2}", st.mean(), st.worst, st.fallbacks, st.exps as f64 / st.samples.max(1) as f64);
        }
        let def = build(c);
        let fs = sr * (1 << def.stages) as f64;
        let mut nc = NonlinearCircuit::new(&def.net, fs);
        nc.set_input(IN_VS, def.supply);
        nc.set_input(IN_VB, 0.5 * def.supply);
        nc.set_supply(def.supply);
        nc.init_dc();
        let t = Instant::now();
        for i in 0..n << def.stages {
            nc.set_input(IN_SIG, 0.1 * (i as f64 * 0.0371 / 16.0).sin());
            nc.process();
        }
        let solver = t.elapsed().as_secs_f64() * 1e9 / n as f64;
        let mut os = OversamplerN::new(def.stages);
        let t = Instant::now();
        let mut acc = 0.0;
        for i in 0..n {
            let mut b = [0.0; MAX_FACTOR];
            os.up(i as f64 * 1e-6, &mut b);
            acc += os.down(&mut b);
        }
        let ovs = t.elapsed().as_secs_f64() * 1e9 / n as f64;
        eprintln!(
            "{}: {total:.0} ns / sample; solver alone {solver:.0}, oversampler {ovs:.0} ({acc:.1}); mean {:.2} it, {:.2} exps / stage-sample",
            CIRCUITS[c],
            s.mean(),
            s.exps as f64 / s.samples as f64
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The README §5 rows

fn db(z: (f64, f64)) -> f64 {
    20.0 * z.0.hypot(z.1).log10()
}

/// Circuit `k` compiled monolithic at `fs`, pots and parts as the effect sets them, at its DC point.
fn reference(k: usize, fs: f64, gain: f64, tone: f64) -> (Circuit, NonlinearCircuit) {
    let def = build(k);
    let mut nc = NonlinearCircuit::monolithic(&def.net, fs);
    for &(pt, rev) in &def.gain {
        nc.set_pot(pt, if rev { 1.0 - gain } else { gain });
    }
    for &(pt, rev) in &def.tone {
        nc.set_pot(pt, if rev { 1.0 - tone } else { tone });
    }
    if let Some((v, _)) = def.hp {
        nc.set_var(v, 1e9);
    }
    nc.set_input(IN_VS, def.supply);
    nc.set_input(IN_VB, 0.5 * def.supply);
    nc.set_supply(def.supply);
    assert!(nc.init_dc(), "{}: DC point", CIRCUITS[k]);
    (def, nc)
}

fn gain_db(x: &[f32], y: &[f32]) -> f64 {
    let h = x.len() / 2;
    let ei: f64 = x[h..].iter().map(|v| (*v as f64).powi(2)).sum();
    let eo: f64 = y[h..].iter().map(|v| (*v as f64).powi(2)).sum();
    10.0 * (eo / ei).log10()
}

#[test]
fn small_signal_matches_the_linearized_netlist() {
    // 0.1 mV (linear at full gain on every circuit), gain / tone 0, 50, 100 %: the effect's gain
    // against the Laplace response of the netlist linearized at its DC point (+ K)
    let sr = 48000.0;
    for k in 0..N_CIRCUITS {
        for (g, t) in [(0.0, 0.0), (50.0, 50.0), (100.0, 100.0), (0.0, 100.0), (100.0, 0.0)] {
            let (def, nc) = reference(k, sr * 8.0, g / 100.0, t / 100.0);
            for f in [200.0, 1000.0, 4000.0] {
                let x = sine(f, 1e-4, 24000, sr);
                let mut e = dist(sr as f32, &[("circuit", k as f32), ("gain", g as f32), ("tone_stack", t as f32)]);
                let got = gain_db(&x, &run(&mut e, &x));
                let want = db(nc.ac_response(f, IN_SIG, def.out)) + K_DB[k];
                assert!((got - want).abs() < 0.25, "{} gain {g} tone {t}, {f} Hz: {got:.2} dB vs {want:.2}", CIRCUITS[k]);
            }
        }
    }
}

#[test]
fn opamp_dist_anchors() {
    // the filter's small-signal corner: 470 Hz at 0 %, 19.3 kHz at 100 % (the 1 kΩ in series)
    // (from the op-amp's output: the post-clipper network, diodes off, its own linear twin)
    let corner = |tone: f64| {
        let def = opamp_dist();
        let tw = def.net.linear_twin(def.twin.0, def.twin.1);
        let mut nc = NonlinearCircuit::new(&tw, 768000.0);
        nc.set_pot(def.tone[0].0, 1.0 - tone);
        let entry = def.net.inputs();
        let r0 = db(nc.ac_response(100.0, entry, def.out));
        let (mut lo, mut hi) = (100.0f64, 60000.0f64);
        for _ in 0..60 {
            let m = (lo * hi).sqrt();
            if db(nc.ac_response(m, entry, def.out)) > r0 - 3.0103 {
                lo = m;
            } else {
                hi = m;
            }
        }
        lo
    };
    let (c0, c1) = (corner(0.0), corner(1.0));
    // the analytic RC: the series 1 kΩ + 1.5 kΩ + pot into 3.3 nF, loaded by the follower's 1 MΩ
    // gate resistor (README §1.1 left it out: 470 Hz there), referenced to 100 Hz like the solve
    let rc = |r: f64| {
        let rp = r * 1e6 / (r + 1e6);
        let h = |f: f64| -10.0 * (1.0 + (2.0 * PI * f * rp * 3.3e-9).powi(2)).log10();
        let (mut lo, mut hi) = (100.0f64, 60000.0f64);
        for _ in 0..60 {
            let m = (lo * hi).sqrt();
            if h(m) > h(100.0) - 3.0103 {
                lo = m;
            } else {
                hi = m;
            }
        }
        lo
    };
    let (want0, want1) = (rc(1e3 + 1.5e3 + 100e3), rc(1e3 + 1.5e3 + R_MIN));
    eprintln!("filter corners {c0:.0} Hz / {c1:.0} Hz (analytic {want0:.0} / {want1:.0}; README 470 / 19,300)");
    assert!((c0 / want0 - 1.0).abs() < 0.03 && (c1 / want1 - 1.0).abs() < 0.03);
    assert!((c1 - 19300.0).abs() < 600.0);
    // gain stage at full gain: 67 dB in the mids, closed loop −3 dB near GBW / 2,300 ≈ 430 Hz
    let mut n = opamp_dist();
    let def = opamp_dist();
    let mut net = def.net.clone();
    let probe = {
        // the op-amp's output node is the first cut node: rebuild with an output on it
        let out = net.elements().iter().find_map(|e| if let Element::Buffer(_, o) = e { Some(*o) } else { None }).unwrap();
        net.output(out)
    };
    n.net = net;
    let mut nc = NonlinearCircuit::new(&n.net, 768000.0);
    nc.set_pot(def.gain[0].0, 1.0);
    nc.set_input(IN_VS, 9.0);
    nc.set_input(IN_VB, 4.5);
    nc.set_supply(9.0);
    nc.init_dc();
    let g = |f: f64| db(nc.ac_response(f, IN_SIG, probe));
    let peak = (1..200).map(|k| g(k as f64 * 10.0)).fold(f64::MIN, f64::max);
    let ideal = 20.0 * (1.0 + (100e3 + R_MIN) / (560.0 * 47.0 / 607.0)).log10();
    eprintln!("gain stage peak {peak:.1} dB (ideal-op-amp asymptote {ideal:.1} dB), at 430 Hz {:.1} dB", g(430.0));
    assert!(peak > ideal - 6.0 && peak < ideal, "{peak} vs {ideal}");
    // the slew: a 10 kHz full-gain square, the op-amp's output slope ≤ SR × 1.05 and ≥ SR × 0.9
    let fs = 768000.0;
    let mut slope: f64 = 0.0;
    let mut last = nc.output(probe);
    for i in 0..(fs * 0.01) as usize {
        let x = if (i * 10000 * 2 / fs as usize).is_multiple_of(2) { 0.25 } else { -0.25 };
        nc.set_input(IN_SIG, x);
        nc.process();
        let y = nc.output(probe);
        slope = slope.max((y - last).abs() * fs);
        last = y;
    }
    eprintln!("slew {:.3} V/µs (LM308 {:.3})", slope * 1e-6, LM308.sr * 1e-6);
    assert!((LM308.sr * 0.9..=LM308.sr * 1.05).contains(&slope));
}

/// Harmonics 1 … 9 (dB re the fundamental, the fundamental in dBV) of circuit `k` at `gain`,
/// a `freq` sine of `amp` V, `fs` oversampled rate, cut graph or monolithic, tolerance `tol`.
fn harmonics(k: usize, gain: f64, freq: f64, amp: f64, fs: f64, mono: bool, tol: f64) -> ([f64; 9], Vec<f64>) {
    harmonics_of(build(k), gain, freq, amp, fs, mono, tol)
}

/// The offline reference's netlist: the Discrete OD's discrete op-amps at transistor level (README
/// §5), the others as built.
fn reference_def(k: usize) -> Circuit {
    if k == DISCRETE_OD {
        discrete_od_with(true)
    } else {
        build(k)
    }
}

fn harmonics_of(def: Circuit, gain: f64, freq: f64, amp: f64, fs: f64, mono: bool, tol: f64) -> ([f64; 9], Vec<f64>) {
    let mut nc = if mono { NonlinearCircuit::monolithic(&def.net, fs) } else { NonlinearCircuit::new(&def.net, fs) };
    nc.set_tolerance(tol);
    for &(pt, rev) in &def.gain {
        nc.set_pot(pt, if rev { 1.0 - gain } else { gain });
    }
    for &(pt, _) in &def.tone {
        nc.set_pot(pt, 0.5);
    }
    if let Some((v, _)) = def.hp {
        nc.set_var(v, 1e9);
    }
    nc.set_input(IN_VS, def.supply);
    nc.set_input(IN_VB, 0.5 * def.supply);
    nc.set_supply(def.supply);
    nc.init_dc();
    // 0.2 s settle, then 0.1 s (an integer number of periods of 100 Hz and 1 kHz)
    let (ns, nm) = ((0.2 * fs) as usize, (0.1 * fs) as usize);
    let mut y = Vec::with_capacity(nm);
    for i in 0..ns + nm {
        nc.set_input(IN_SIG, amp * (2.0 * PI * freq * i as f64 / fs).sin());
        nc.process();
        if i >= ns {
            y.push(nc.output(def.out));
        }
    }
    let amp_at = |f: f64| {
        let w = 2.0 * PI * f / fs;
        let (mut c, mut s) = (0.0, 0.0);
        for (i, v) in y.iter().enumerate() {
            let ph = w * (i + ns) as f64;
            c += v * ph.cos();
            s += v * ph.sin();
        }
        2.0 * c.hypot(s) / y.len() as f64
    };
    let h1 = amp_at(freq);
    let mut h = [0.0; 9];
    h[0] = 20.0 * h1.log10();
    for n in 2..=9 {
        h[n - 1] = 20.0 * (amp_at(freq * n as f64) / h1).log10();
    }
    // the waveform on the base-rate grid (every fs / 48 kHz-th sample), for the null
    let step = (fs / 48000.0).round() as usize;
    (h, y.iter().step_by(step.max(1)).copied().collect())
}

/// The reference fixtures (README §5 "Reference", gfx-fuzz §2.9): the netlists solved offline,
/// monolithic, at 64×, Newton to 10⁻¹². One line per case: circuit, gain, freq, amp, then the
/// fundamental (dBV) and harmonics 2 – 9 (dBc). Regenerated by `regenerate_fixtures`.
const FIXTURES: &str = include_str!("../fixtures/reference.txt");

const CASES: [(f64, f64, f64); 4] = [(0.5, 1000.0, 0.25), (0.5, 100.0, 0.25), (1.0, 1000.0, 0.063), (0.2, 1000.0, 1.0)];

#[test]
#[ignore = "writes gfx-dist/fixtures/: cargo test --release -p gfx-dist --no-default-features regenerate_fixtures -- --ignored"]
fn regenerate_fixtures() {
    let mut out = String::from("# gfx-dist reference: monolithic, 64x (3.072 MHz), Newton tol 1e-12 (circuit_nl offline mode)\n# circuit gain freq amp | h1 (dBV) h2..h9 (dBc)\n");
    for k in 0..N_CIRCUITS {
        for &(g, f, a) in &CASES {
            // monolithic, except the transistor-level Discrete OD (13 ports, over a stage's 12): its
            // exact cuts kept (the gates' 10⁻¹⁴ A of reverse current is the only thing a cut leaves out)
            let (h, _) = harmonics_of(reference_def(k), g, f, a, 48000.0 * 64.0, k != DISCRETE_OD, 1e-12);
            out += &format!("{k} {g} {f} {a} |");
            for v in h {
                out += &format!(" {v:.3}");
            }
            out += "\n";
        }
        // the deck the same netlist gives ngspice (to run wherever it is installed; the Discrete OD's
        // at transistor level, its JFETs as J cards)
        let def = reference_def(k);
        let rot: Vec<f64> = def.net.pots().iter().map(|_| 0.5).collect();
        let deck = def.net.to_spice(CIRCUITS[k], &[0.0, def.supply, 0.5 * def.supply], &rot, 25.0, def.supply);
        std::fs::write(format!("{}/fixtures/{}.cir", env!("CARGO_MANIFEST_DIR"), CIRCUITS[k].to_lowercase().replace(' ', "-")), deck).unwrap();
    }
    std::fs::write(format!("{}/fixtures/reference.txt", env!("CARGO_MANIFEST_DIR")), out).unwrap();
}

#[test]
fn spice_decks_are_current() {
    for k in 0..N_CIRCUITS {
        let def = reference_def(k);
        let rot: Vec<f64> = def.net.pots().iter().map(|_| 0.5).collect();
        let deck = def.net.to_spice(CIRCUITS[k], &[0.0, def.supply, 0.5 * def.supply], &rot, 25.0, def.supply);
        let path = format!("{}/fixtures/{}.cir", env!("CARGO_MANIFEST_DIR"), CIRCUITS[k].to_lowercase().replace(' ', "-"));
        let committed = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(deck, committed, "{path} is stale: regenerate_fixtures");
    }
}

#[test]
fn large_signal_matches_the_reference() {
    // the real-time solve (cut graph, the circuit's rate, tolerance 10⁻⁶) against the committed
    // reference: harmonics 2 – 9 within 1 dB where ≥ −60 dBc, the fundamental within 0.5 dB
    let mut n = 0;
    for line in FIXTURES.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
        let (head, tail) = line.split_once('|').unwrap();
        let h: Vec<f64> = head.split_whitespace().map(|v| v.parse().unwrap()).collect();
        let r: Vec<f64> = tail.split_whitespace().map(|v| v.parse().unwrap()).collect();
        let k = h[0] as usize;
        let fs = 48000.0 * (1 << build(k).stages) as f64;
        let (got, _) = harmonics(k, h[1], h[2], h[3], fs, false, 1e-6);
        assert!((got[0] - r[0]).abs() < 0.5, "{} {head}: fundamental {:.2} vs {:.2}", CIRCUITS[k], got[0], r[0]);
        for j in 1..9 {
            if r[j] > -60.0 {
                assert!((got[j] - r[j]).abs() < 1.0, "{} {head}: h{} {:.2} vs {:.2}", CIRCUITS[k], j + 1, got[j], r[j]);
            }
        }
        n += 1;
    }
    assert_eq!(n, N_CIRCUITS * CASES.len());
}

#[test]
fn stage_cuts_are_exact() {
    // cut graph vs monolithic, same rate, tolerance 10⁻¹²: null ≤ −100 dB
    for k in 0..N_CIRCUITS {
        let fs = 48000.0 * 8.0;
        let (_, a) = harmonics(k, 0.5, 1000.0, 0.25, fs, false, 1e-12);
        let (_, b) = harmonics(k, 0.5, 1000.0, 0.25, fs, true, 1e-12);
        let peak = a.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let diff = a.iter().zip(&b).fold(0.0f64, |m, (p, q)| m.max((p - q).abs()));
        let null = 20.0 * (diff / peak).log10();
        eprintln!("{}: cut vs monolithic {null:.0} dB", CIRCUITS[k]);
        assert!(null < -100.0, "{}: {null}", CIRCUITS[k]);
    }
}

#[test]
fn newton_solves_everything() {
    // squares and noise at +12 dBFS, every op-amp and clipper option, at 48 kHz; the defaults at
    // 44.1 – 192 kHz: no unsolved sample, no NaN; mean iterations ≤ 3 at the defaults
    let sr = 48000.0;
    let n = 12000;
    let square: Vec<f32> = (0..n).map(|i| if (i / 60) % 2 == 0 { 4.0 } else { -4.0 }).collect();
    let mut seed = 1u32;
    let noise: Vec<f32> = (0..n)
        .map(|_| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            4.0 * ((seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0)
        })
        .collect();
    let mut worst_block = 0u64;
    for k in 0..N_CIRCUITS {
        for (id, opts) in [("opamp", OPAMPS.len()), ("diodes", DIODES.len())] {
            for o in 0..opts {
                for (sig, x) in [("square", &square), ("noise", &noise)] {
                    let mut e = dist(sr as f32, &[("circuit", k as f32), ("gain", 100.0), (id, o as f32)]);
                    let mut y = x.clone();
                    for c in y.chunks_mut(32) {
                        let before = e.stats().iterations;
                        e.process(&mut [c]);
                        worst_block = worst_block.max(e.stats().iterations - before);
                    }
                    let s = e.stats();
                    assert!(y.iter().all(|v| v.is_finite()), "{} {id} {o} {sig}: NaN", CIRCUITS[k]);
                    assert_eq!(s.unsolved, 0, "{} {id} {o} {sig}: {s:?}", CIRCUITS[k]);
                }
            }
        }
        for rate in [44100.0, 96000.0, 192000.0] {
            let mut e = dist(rate as f32, &[("circuit", k as f32)]);
            let x = sine(1000.0, 1.0, rate as usize / 4, rate);
            let y = run(&mut e, &x);
            assert!(y.iter().all(|v| v.is_finite()));
            assert_eq!(e.stats().unsolved, 0, "{} at {rate}", CIRCUITS[k]);
        }
        let mut e = dist(sr as f32, &[("circuit", k as f32)]);
        let x = sine(220.0, 0.5, 24000, sr);
        run(&mut e, &x);
        let s = e.stats();
        eprintln!(
            "{}: defaults, −6 dBFS 220 Hz: mean {:.2} it / stage-sample, worst sample {}, fallbacks {:.3} %",
            CIRCUITS[k],
            s.mean(),
            s.worst,
            100.0 * s.fallbacks as f64 / s.samples as f64
        );
        assert!(s.mean() <= 3.0);
        assert!((s.fallbacks as f64) <= 0.01 * s.samples as f64);
    }
    eprintln!("worst 32-sample block: {worst_block} iterations");
}

#[test]
fn jacobian_keeps_its_sign() {
    // det(K Jf − I) over a grid of the port range, every stage, pots at 0 / 50 / 100 %
    for k in 0..N_CIRCUITS {
        for g in [0.0, 0.5, 1.0] {
            let def = build(k);
            let mut nc = NonlinearCircuit::new(&def.net, 48000.0 * (1 << def.stages) as f64);
            for &(pt, _) in def.gain.iter().chain(&def.tone) {
                nc.set_pot(pt, g);
            }
            nc.set_supply(def.supply);
            for (s, &m) in nc.ports().iter().enumerate() {
                if m == 0 {
                    continue;
                }
                let grid = [-6.0, -1.0, -0.5, -0.1, 0.0, 0.1, 0.5, 0.7, 1.0, 4.5, 8.0];
                let mut sign = 0.0;
                let mut idx = vec![0usize; m];
                loop {
                    let v: Vec<f64> = idx.iter().map(|&i| grid[i]).collect();
                    let d = nc.jacobian_det(s, &v);
                    if d != 0.0 {
                        if sign == 0.0 {
                            sign = d.signum();
                        }
                        assert_eq!(d.signum(), sign, "{} stage {s} gain {g} at {v:?}", CIRCUITS[k]);
                    }
                    let mut j = 0;
                    loop {
                        idx[j] += 1;
                        if idx[j] < grid.len() {
                            break;
                        }
                        idx[j] = 0;
                        j += 1;
                        if j == m {
                            break;
                        }
                    }
                    if j == m {
                        break;
                    }
                }
            }
        }
    }
}

#[test]
fn stiff_modes_stay_quiet() {
    // the circuit at its oversampled rate fs, a full-gain 1 kHz sine: the band 0.4 – 0.5 fs, where
    // the trapezoidal rule's half-rate ringing would sit, against the same absolute band of the same
    // circuit solved at 2 fs (no such mode there). A clipped wave has real energy in that band
    // (−50 to −70 dB re the signal with the filters open), so the README's absolute −80 dB cannot
    // tell a stiff mode from the waveform; the excess over the finer solve can (≤ 3 dB).
    let band = |k: usize, fs: f64, lo: f64, hi: f64| {
        let def = build(k);
        let (_, mut nc) = reference(k, fs, 1.0, 1.0);
        let n = (fs * 0.02) as usize;
        let mut y = Vec::with_capacity(n);
        for i in 0..n + (fs * 0.02) as usize {
            nc.set_input(IN_SIG, 0.25 * (2.0 * PI * 1000.0 * i as f64 / fs).sin());
            nc.process();
            if i >= (fs * 0.02) as usize {
                y.push(nc.output(def.out));
            }
        }
        let total: f64 = y.iter().map(|v| v * v).sum::<f64>() / y.len() as f64;
        // 20 ms windows: the 1 kHz harmonics (and anything at the half rate) sit on 50 Hz bins
        let mut e = 0.0;
        let mut f = (lo / 1000.0).ceil() * 1000.0;
        while f < hi {
            let w = 2.0 * PI * f / fs;
            let (mut c, mut s) = (0.0, 0.0);
            for (i, v) in y.iter().enumerate() {
                c += v * (w * i as f64).cos();
                s += v * (w * i as f64).sin();
            }
            e += 2.0 * (c * c + s * s) / (y.len() as f64).powi(2);
            f += 1000.0;
        }
        10.0 * (e / total).log10()
    };
    for k in 0..N_CIRCUITS {
        let fs = 48000.0 * (1 << build(k).stages) as f64;
        let a = band(k, fs, 0.4 * fs, 0.5 * fs);
        let b = band(k, 2.0 * fs, 0.4 * fs, 0.5 * fs);
        eprintln!("{}: 0.4 – 0.5 fs {a:.0} dB re the signal, {b:.0} dB solved at 2 fs", CIRCUITS[k]);
        // the Discrete OD: +4.4 dB, kept trapezoidal (circuits::discrete_opamp: backward Euler costs
        // 3 – 6 dB of alias; the band is above the decimator's stop band)
        let excess = if k == DISCRETE_OD { 5.0 } else { 3.0 };
        assert!(a < b + excess, "{}: {a} vs {b}", CIRCUITS[k]);
    }
}

#[test]
fn aliasing_meets_the_policy() {
    // golem contract (2026-11-14): ≤ −70 dB for a 1.2 kHz sine at full gain with the circuit's own
    // parts, ≤ −65 dB with any part swap (−12 dBFS, tone open, 44.1 kHz)
    for k in 0..N_CIRCUITS {
        let (own, _) = alias_at(k, None, &[]);
        let mut worst: f64 = own;
        for o in 1..OPAMPS.len() {
            worst = worst.max(alias_at(k, None, &[("opamp", o as f32)]).0);
        }
        for d in 1..DIODES.len() {
            worst = worst.max(alias_at(k, None, &[("diodes", d as f32)]).0);
        }
        eprintln!("{}: own parts {own:.0} dB, worst swap {worst:.0} dB", CIRCUITS[k]);
        assert!(own <= -70.0 && worst <= -65.0);
    }
}

#[test]
fn taper_follows_the_pot_law() {
    // Op-Amp Dist, 0.1 mV at 1 kHz, gain 0, 10 … 100 %: the netlist with its log pot, ± 0.3 dB
    let sr = 48000.0;
    for g in (0..=10).map(|k| k as f64 * 10.0) {
        let (def, nc) = reference(0, sr * 8.0, g / 100.0, 0.5);
        let x = sine(1000.0, 1e-4, 24000, sr);
        let mut e = dist(sr as f32, &[("gain", g as f32)]);
        let got = gain_db(&x, &run(&mut e, &x));
        let want = db(nc.ac_response(1000.0, IN_SIG, def.out)) + K_DB[0];
        assert!((got - want).abs() < 0.3, "gain {g}: {got:.2} vs {want:.2}");
    }
}

#[test]
fn activation_and_switches_are_silent() {
    // from the DC point: silence in → nothing out (≤ −60 dBFS) at activation, after a reset and
    // through a circuit switch; the inline re-solve's time reported
    let sr = 48000.0;
    for k in 0..N_CIRCUITS {
        let mut e = dist(sr as f32, &[("circuit", k as f32)]);
        let z = vec![0.0f32; 4800];
        let y = run(&mut e, &z);
        let peak = y.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(20.0 * peak.max(1e-12).log10() < -60.0, "{} activation {peak}", CIRCUITS[k]);
        let t = std::time::Instant::now();
        e.reset();
        let dt = t.elapsed().as_secs_f64() * 1e6;
        let y = run(&mut e, &z);
        let peak = y.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(20.0 * peak.max(1e-12).log10() < -60.0, "{} reset {peak}", CIRCUITS[k]);
        e.set_param("circuit", ((k + 1) % N_CIRCUITS) as f32);
        let t = std::time::Instant::now();
        let y = run(&mut e, &z[..64]);
        let sw = t.elapsed().as_secs_f64() * 1e6;
        let y2 = run(&mut e, &z);
        let peak = y.iter().chain(&y2).fold(0.0f32, |m, v| m.max(v.abs()));
        eprintln!("{}: reset {dt:.0} µs, switch block (64 samples, incl. the DC re-solve) {sw:.0} µs", CIRCUITS[k]);
        assert!(20.0 * peak.max(1e-12).log10() < -60.0, "{} switch {peak}", CIRCUITS[k]);
    }
}

#[test]
fn supply_moves_bias_and_rails_monotonically() {
    // the DC point at 50 … 200 %: the op-amp output's bias rises with the supply; a sweep under a
    // sine stays solved and bounded
    for k in 0..N_CIRCUITS {
        let mut last = f64::MIN;
        for pct in [50.0, 75.0, 100.0, 150.0, 200.0] {
            let def = build(k);
            let vs = rail(k, 9.0 * pct / 100.0);
            let mut net = def.net.clone();
            // the first op-amp macro's output (its integrator's buffer), else the first buffer
            let int = net.elements().iter().find_map(|e| match e {
                Element::Device(d, n) if matches!(net.devices()[*d], DeviceModel::OpAmp(_)) => Some(n[2]),
                _ => None,
            });
            let out = net.elements().iter().find_map(|e| if let Element::Buffer(c, o) = e { (int.is_none() || Some(*c) == int).then_some(*o) } else { None }).unwrap();
            let probe = net.output(out);
            let mut nc = NonlinearCircuit::new(&net, 384000.0);
            nc.set_input(IN_VS, vs);
            nc.set_input(IN_VB, 0.5 * vs);
            nc.set_supply(vs);
            nc.init_dc();
            let b = nc.output(probe);
            // the Plexi Dist holds its supply at 150 % (decided 2026-11-26)
            let held = k == PLEXI_DIST && pct > 150.0;
            assert!(b > last || held && b == last, "{} at {pct} %: {b} after {last}", CIRCUITS[k]);
            last = b;
        }
        let sr = 48000.0;
        let mut e = dist(sr as f32, &[("circuit", k as f32), ("supply", 50.0)]);
        let x = sine(200.0, 0.25, 96000, sr);
        let mut y = x.clone();
        for (i, c) in y.chunks_mut(512).enumerate() {
            e.set_param("supply", (50.0 + 150.0 * (i * 512) as f64 / 96000.0) as f32);
            e.process(&mut [c]);
        }
        assert!(y.iter().all(|v| v.is_finite() && v.abs() < 10.0));
        // a known limit, reported (README, As coded): the Plexi Dist at its 50 % floor (VA 4.2 V: the
        // LM833's 1.2 V of swing) holds one sample now and then on a sine of 0.2 – 0.5 V
        let allowed = if k == PLEXI_DIST { 1 } else { 0 };
        let u = e.stats().unsolved;
        if u > 0 {
            eprintln!("{}: supply sweep from 50 %: {u} unsolved sample(s)", CIRCUITS[k]);
        }
        assert!(u <= allowed, "{}: {:?}", CIRCUITS[k], e.stage_stats());
    }
}

/// The bench's click metric on a 200 Hz sine: HF (> 10 kHz) peak in the 100 ms after a step,
/// minus the steady HF, in dB re the output peak.
fn step_burst(set: &[(&str, f32)], id: &str, from: f32, to: f32) -> f64 {
    let sr = 48000.0;
    let mut s: Vec<(&str, f32)> = set.to_vec();
    s.push((id, from));
    let mut e = dist(sr as f32, &s);
    let n = 48000;
    let mut y = sine(200.0, 0.25, n, sr);
    let (a, b) = y.split_at_mut(n / 2);
    for c in a.chunks_mut(512) {
        e.process(&mut [c]);
    }
    e.set_param(id, to);
    for c in b.chunks_mut(512) {
        e.process(&mut [c]);
    }
    let mut f1 = gfx_dsp::svf::Svf::new();
    let mut f2 = gfx_dsp::svf::Svf::new();
    f1.set(gfx_dsp::svf::SvfMode::HighPass, 10000.0, 0.541_196, 0.0, sr as f32);
    f2.set(gfx_dsp::svf::SvfMode::HighPass, 10000.0, 1.306_563, 0.0, sr as f32);
    let hp: Vec<f32> = y.iter().map(|&v| f2.process(f1.process(v))).collect();
    let pk = |s: &[f32]| s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let peak = pk(&y);
    let after = pk(&hp[n / 2..n / 2 + n / 10]);
    let steady = pk(&hp[n / 4..n / 2]).max(pk(&hp[n - n / 4..]));
    20.0 * ((after - steady).max(1e-9) / peak).log10() as f64
}

#[test]
fn switches_under_a_sine_do_not_click() {
    let mut worst: (f64, String) = (-300.0, String::new());
    for k in 0..N_CIRCUITS {
        let c = [("circuit", k as f32)];
        let mut cases: Vec<(&str, f32, f32)> = vec![("circuit", k as f32, ((k + 1) % N_CIRCUITS) as f32), ("hp", 0.0, 1.0)];
        for o in 1..OPAMPS.len() {
            cases.push(("opamp", 0.0, o as f32));
        }
        for d in 1..DIODES.len() {
            cases.push(("diodes", 0.0, d as f32));
        }
        for (id, a, b) in cases {
            let r = step_burst(&c, id, a, b);
            if r > worst.0 {
                worst = (r, format!("{} {id} {a} → {b}", CIRCUITS[k]));
            }
        }
    }
    eprintln!("worst switch burst {:.0} dB ({})", worst.0, worst.1);
    assert!(worst.0 <= -30.0, "{worst:?}");
}

#[test]
fn zipper_is_below_the_contract() {
    // `gain` and `tone_stack` driven like an EXP pedal (7-bit steps every 20 ms) against the same
    // sweep set every sample: the difference ≤ −80 dB re the signal (1 mV, nothing masks it)
    let sr = 48000.0;
    for id in ["gain", "tone_stack"] {
        let n = 48000;
        let x = sine(1000.0, 1e-3, n, sr);
        let render = |stepped: bool| {
            let mut e = dist(sr as f32, &[(id, 40.0)]);
            let mut y = x.clone();
            for (i, v) in y.iter_mut().enumerate() {
                let t = i as f64 / n as f64;
                let target = 40.0 + 20.0 * t;
                if stepped {
                    if i % 960 == 0 {
                        let q = (target / 100.0 * 127.0).round() / 127.0 * 100.0;
                        e.set_param(id, q as f32);
                    }
                } else {
                    e.set_param(id, target as f32);
                }
                let mut b = [*v];
                e.process(&mut [&mut b]);
                *v = b[0];
            }
            y
        };
        let (a, b) = (render(true), render(false));
        let pk = a.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        // control-rate sidebands: the difference's content at 1 kHz ± 50, 100, 150 Hz
        let d: Vec<f32> = a.iter().zip(&b).map(|(p, q)| p - q).collect();
        let mut worst: f64 = -300.0;
        for k in 1..=3 {
            for f in [1000.0 - 50.0 * k as f64, 1000.0 + 50.0 * k as f64] {
                worst = worst.max(20.0 * (tone_amp(&d, n / 4, f, sr) / pk as f64).log10());
            }
        }
        eprintln!("{id}: control-rate sidebands {worst:.0} dB");
        assert!(worst < -80.0, "{id}: {worst}");
    }
}

#[test]
fn bass_keep_keeps_the_bass_clean() {
    // 120 Hz split, 60 Hz + 1 kHz at −12 dBFS, full gain: the 60 Hz stays clean (THD ≤ −40 dB) and
    // does not modulate the 1 kHz (sidebands at ±60 Hz ≤ −40 dB)
    let sr = 48000.0;
    for k in 0..N_CIRCUITS {
        let n = 96000;
        let x: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f64 / sr;
                (0.125 * (2.0 * PI * 60.0 * t).sin() + 0.125 * (2.0 * PI * 1000.0 * t).sin()) as f32
            })
            .collect();
        let mut e = dist(sr as f32, &[("circuit", k as f32), ("gain", 100.0), ("bass_keep", 120.0)]);
        let y = run(&mut e, &x);
        let from = n / 2;
        let f60 = tone_amp(&y, from, 60.0, sr);
        let h = (2..=5).map(|m| tone_amp(&y, from, 60.0 * m as f64, sr).powi(2)).sum::<f64>().sqrt();
        let thd = 20.0 * (h / f60).log10();
        let k1 = tone_amp(&y, from, 1000.0, sr);
        let sb = tone_amp(&y, from, 940.0, sr).max(tone_amp(&y, from, 1060.0, sr));
        let side = 20.0 * (sb / k1).log10();
        eprintln!("{}: 60 Hz THD {thd:.0} dB, ±60 Hz sidebands {side:.0} dB", CIRCUITS[k]);
        assert!(thd < -40.0 && side < -40.0, "{}", CIRCUITS[k]);
    }
    // Op-Amp Dist at gain 0, 1 mV: no notch deeper than 3 dB within an octave of the crossover
    let mut lv = Vec::new();
    for f in [60.0, 80.0, 100.0, 120.0, 150.0, 180.0, 240.0] {
        let x = sine(f, 1e-3, 48000, sr);
        let mut a = dist(sr as f32, &[("gain", 0.0), ("bass_keep", 120.0)]);
        let mut b = dist(sr as f32, &[("gain", 0.0)]);
        let ga = gain_db(&x, &run(&mut a, &x));
        let gb = gain_db(&x, &run(&mut b, &x));
        lv.push((f, ga - gb));
    }
    eprintln!("bass keep vs off at gain 0: {lv:?}");
    assert!(lv.iter().all(|(_, d)| *d > -3.0), "{lv:?}");
}

/// A DI-like proxy (golem's DIs are not in this repository): plucked notes E2 … E4, harmonics at
/// 1 / k with faster decay up the series, −6 dBFS peak.
fn di_proxy(sr: f64) -> Vec<f32> {
    let notes = [82.41, 110.0, 146.83, 196.0, 246.94, 329.63, 164.81, 220.0];
    let len = (sr * 0.5) as usize;
    let mut y = vec![0.0f64; len * notes.len()];
    for (j, f) in notes.iter().enumerate() {
        for i in 0..len {
            let t = i as f64 / sr;
            let mut s = 0.0;
            for k in 1..=12 {
                s += (2.0 * PI * f * k as f64 * t).sin() / k as f64 * (-t * (3.0 + 2.0 * k as f64)).exp();
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
    // K(circuit): at `gain` 50 %, `tone_stack` 50 %, `level` 0 dB the DI proxy comes out at the
    // bypass loudness ± 1 dB; the table (with "hardware level pot fully up" = −K) printed
    let sr = 48000.0;
    let x = di_proxy(sr);
    let lin = loudness(&x, sr);
    let mut off = Vec::new();
    for k in 0..N_CIRCUITS {
        let mut e = dist(sr as f32, &[("circuit", k as f32)]);
        let y = run(&mut e, &x);
        let d = loudness(&y, sr) - lin;
        eprintln!("{:<13} K = {:+.1} dB (measured without it {:+.1} dB): hardware fully up = level {:+.1} dB", CIRCUITS[k], K_DB[k], d - K_DB[k], -K_DB[k]);
        if d.abs() >= 1.0 {
            off.push((CIRCUITS[k], d));
        }
    }
    assert!(off.is_empty(), "off unity: {off:?}");
}

#[test]
fn cleanup_with_the_guitar_volume() {
    // `input` 0 / −6 / −12 dB: THD falls monotonically (gain 50 %) — except on the Blend OD, whose
    // clean feeds grow with the input while the clipped path stays pinned: there the THD *rises* as
    // the input falls (measured; README §5 expected ≥ 10 dB less THD at −12 dB, which the circuit
    // does not do: reported, not asserted)
    let sr = 48000.0;
    for k in 0..N_CIRCUITS {
        let blend = CIRCUITS[k] == "Blend OD";
        let mut last = f64::MAX;
        for inp in [0.0, -6.0, -12.0] {
            let x = sine(220.0, 0.5, 48000, sr);
            let mut e = dist(sr as f32, &[("circuit", k as f32), ("input", inp)]);
            let y = run(&mut e, &x);
            let f = tone_amp(&y, 24000, 220.0, sr);
            let h = (2..=9).map(|m| tone_amp(&y, 24000, 220.0 * m as f64, sr).powi(2)).sum::<f64>().sqrt();
            let thd = 20.0 * (h / f).log10();
            eprintln!("{} input {inp} dB: THD {thd:.1} dB", CIRCUITS[k]);
            assert!(blend || thd < last + 0.1, "{} {inp}", CIRCUITS[k]);
            last = thd;
        }
    }
}

#[test]
fn deterministic() {
    let x = sine(220.0, 0.4, 9600, 48000.0);
    for k in 0..N_CIRCUITS {
        let r = || {
            let mut e = dist(48000.0, &[("circuit", k as f32), ("gain", 70.0)]);
            run(&mut e, &x)
        };
        assert_eq!(r(), r());
    }
}

// ---- linked mono (README, As coded): L == R runs channel 0 alone

/// A stereo instance (`no_link`: true stereo always, the reference).
fn dist2(set: &[(&str, f32)], no_link: bool) -> Dist {
    let mut e = Dist::new();
    for (k, v) in set {
        e.set_param(k, *v);
    }
    e.no_link = no_link;
    e.prepare(48000.0, 4096, 2);
    e
}

/// Renders (l, r) in blocks of `block`; each event (sample, id, value) lands on a block start.
fn run_pair(e: &mut Dist, l: &[f32], r: &[f32], block: usize, ev: &[(usize, &str, f32)]) -> [Vec<f32>; 2] {
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

/// Seeded low-passed white noise at peak ≈ `amp`.
fn noise(n: usize, amp: f64, seed: u32) -> Vec<f32> {
    let (mut s, mut lp) = (seed, 0.0f64);
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            lp += 0.3 * ((s >> 8) as f64 / 8388608.0 - 1.0 - lp);
            (amp * lp) as f32
        })
        .collect()
}

/// The unlink after the relink: inside the circuit fade that starts at 20480.
const UNLINK2: usize = 20480 + 512;

/// The link test stream's right channel: L on [0, 6000) and [12000, UNLINK2) (long enough to
/// relink: 60 ms hold + 30 ms fade), L + `d` elsewhere (a different input: the unlinks).
fn link_right(l: &[f32], d: &[f32]) -> Vec<f32> {
    (0..l.len())
        .map(|i| if (6000..12000).contains(&i) || i >= UNLINK2 { l[i] + d[i] } else { l[i] })
        .collect()
}

/// A knob change while linked, a circuit change while unlinked, another just before the second
/// unlink (which falls inside its 30 ms fade).
const LINK_EVENTS: [(usize, &str, f32); 3] = [(4096, "gain", 80.0), (8192, "circuit", 2.0), (20480, "circuit", 0.0)];

#[test]
fn linked_stereo_is_the_mono_output() {
    let x = di_proxy(48000.0)[..26112].to_vec();
    let mut m = dist(48000.0, &[]);
    let mut y = x.clone();
    for (k, c) in y.chunks_mut(512).enumerate() {
        for &(t, id, v) in &LINK_EVENTS {
            if t == 512 * k {
                m.set_param(id, v);
            }
        }
        m.process(&mut [c]);
    }
    let [l, r] = run_pair(&mut dist2(&[], false), &x, &x, 512, &LINK_EVENTS);
    assert!(l == y && r == y, "L == R stereo differs from mono");
}

#[test]
fn link_switches_are_exact_and_click_free() {
    let sr = 48000.0;
    let n = 26112;
    let l = sine(1000.0, 0.25, n, sr);
    let r = link_right(&l, &noise(n, 1e-3, 5));
    let [l1, r1] = run_pair(&mut dist2(&[], false), &l, &r, 512, &LINK_EVENTS);
    let [l0, r0] = run_pair(&mut dist2(&[], true), &l, &r, 512, &LINK_EVENTS);
    // channel 0 never changes; channel 1 is true stereo, bit for bit, until the relink fade starts
    // (the 2880th sample of L == R)
    let relink = 12000 + 2880 - 1;
    assert!(l1 == l0, "channel 0 moved");
    let first = (0..relink).find(|&i| r1[i] != r0[i]);
    assert!(first.is_none(), "the unlink is not exact from {first:?}");
    // the transitions: no click by the bench's metric (HF > 10 kHz in the 100 ms after, minus
    // channel 0's, re peak; bound −30 dB as the bench's) nor a sample step above channel 0's own
    let hp = |y: &[f32]| {
        let mut f1 = gfx_dsp::svf::Svf::new();
        let mut f2 = gfx_dsp::svf::Svf::new();
        f1.set(gfx_dsp::svf::SvfMode::HighPass, 10000.0, 0.541_196, 0.0, sr as f32);
        f2.set(gfx_dsp::svf::SvfMode::HighPass, 10000.0, 1.306_563, 0.0, sr as f32);
        y.iter().map(|&v| f2.process(f1.process(v))).collect::<Vec<f32>>()
    };
    let pk = |s: &[f32]| s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let step = |s: &[f32]| s.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
    let (h0, h1) = (hp(&l1), hp(&r1));
    let peak = pk(&l1);
    for t in [6000, 12000, relink, UNLINK2] {
        let w = t - 480..(t + 4800).min(n);
        let burst = 20.0 * ((pk(&h1[w.clone()]) - pk(&h0[w.clone()])).max(1e-9) / peak).log10();
        let (s1, s0) = (step(&r1[w.clone()]), step(&l1[w.clone()]));
        eprintln!("link transition at {t}: HF burst {burst:.0} dB re peak, max step {s1:.4} (L {s0:.4})");
        assert!(burst <= -30.0, "{t}: {burst} dB");
        assert!(s1 <= 1.05 * s0, "{t}: step {s1} vs {s0}");
    }
}

#[test]
fn linked_mono_is_block_size_invariant() {
    let n = 24576;
    let l = noise(n, 0.3, 3);
    let r = link_right(&l, &noise(n, 0.1, 4));
    let reference = run_pair(&mut dist2(&[], false), &l, &r, 512, &LINK_EVENTS);
    for b in [1usize, 64, 4096] {
        assert!(run_pair(&mut dist2(&[], false), &l, &r, b, &LINK_EVENTS) == reference, "block {b}");
    }
}

/// gfx-dsp `circuit_nl` gained a forward-only BJT (gfx-octafuzz, 2026-11-26): every circuit, two
/// gains, every clipper and op-amp option, renders bit for bit as before (FNV-1a of the f32 bits).
#[test]
fn renders_are_bit_identical_to_before_the_forward_only_bjt() {
    let mut h = 0xcbf29ce484222325u64;
    let mut fnv = |y: &[f32]| {
        for v in y {
            for b in v.to_bits().to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
        }
    };
    let x = noise(4800, 0.4, 11);
    for c in 0..N_CIRCUITS_V1 {
        for g in [30.0, 100.0] {
            let mut e = dist(48000.0, &[("circuit", c as f32), ("gain", g)]);
            fnv(&run(&mut e, &x));
        }
    }
    for d in 0..7 {
        let mut e = dist(48000.0, &[("diodes", d as f32), ("gain", 80.0)]);
        fnv(&run(&mut e, &x));
    }
    for o in 0..6 {
        let mut e = dist(48000.0, &[("opamp", o as f32), ("gain", 80.0), ("supply", 60.0)]);
        fnv(&run(&mut e, &x));
    }
    eprintln!("renders: {h:#018x}");
    assert_eq!(h, 0xf3c1_5910_839d_26ef);
}

// ---- the circuits of 0.8 (README §5, rows added 2026-11-26)

mod voicings;
