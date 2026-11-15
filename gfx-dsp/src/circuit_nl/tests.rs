// SPDX-License-Identifier: GPL-3.0-or-later
//! circuit_nl's unit tests: the exp, the device laws, the linear part against the Laplace analysis,
//! the op-amp macro's slew and bandwidth, Woodbury against a rebuild, cuts against the monolithic
//! solve, the DC point, the deck.

use super::*;

fn db(z: (f64, f64)) -> f64 {
    20.0 * z.0.hypot(z.1).log10()
}

#[test]
fn fast_exp_is_accurate() {
    let mut worst: f64 = 0.0;
    let mut x = -60.0;
    while x < 60.0 {
        let r = (fast_exp(x) - x.exp()) / x.exp();
        worst = worst.max(r.abs());
        x += 0.0137;
    }
    assert!(worst < 1e-9, "relative error {worst:e}");
    // as a null in dB re the value: far below −100 dB
    assert!(20.0 * worst.log10() < -100.0);
    assert!(fast_exp(-800.0) >= 0.0 && fast_exp(800.0).is_finite());
}

#[test]
fn pnjlim_compresses_only_above_vcrit() {
    let (vt, vc) = (0.02585, 0.6);
    assert_eq!(pnjlim(0.3, -1.0, vt, vc), 0.3);
    let l = pnjlim(5.0, 0.6, vt, vc);
    assert!(l > 0.6 && l < 0.8, "{l}");
}

#[test]
fn temperature_law_is_spices() {
    // Is grows ≈ ×2.25 per +10 °C for a germanium junction (N = 1.3 here: less), silicon more
    let j = Junction::germanium(1e-6, 1.0);
    let (a, _) = j.at(25.0);
    let (b, _) = j.at(35.0);
    assert!((a - 1e-6).abs() < 1e-15);
    assert!(b / a > 2.0 && b / a < 2.6, "{}", b / a);
    let (_, nvt) = Junction::silicon(1e-14, 2.0).at(25.0);
    assert!((nvt - 2.0 * thermal_voltage(25.0)).abs() < 1e-12);
}

#[test]
fn series_strings_collapse_exactly() {
    // two identical junctions in series: the same current at twice the voltage
    let one = ClipperModel::diode(D_1N914);
    let two = ClipperModel { pos: Some(ClipSide::new(D_1N914, 2)), neg: None };
    let e1 = evaluate(&DeviceModel::Clipper(one), (0.0, 0.0), 25.0, 9.0);
    let e2 = evaluate(&DeviceModel::Clipper(two), (0.0, 0.0), 25.0, 9.0);
    let (mut i1, mut i2) = ([0.0; 1], [0.0; 1]);
    let mut j = [0.0; 1];
    e1.eval(&[0.55], &mut i1, &mut j, 1, 0);
    e2.eval(&[1.10], &mut i2, &mut j, 1, 0);
    assert!((i1[0] - i2[0]).abs() < 1e-8 * i1[0].abs(), "{} vs {}", i1[0], i2[0]);
}

/// Input → 1 kΩ → node → 100 nF to ground: an RC.
fn rc(diodes: bool) -> (Netlist, usize) {
    let mut n = Netlist::new(1);
    let i = n.input(0);
    let a = n.node();
    n.r(i, a, 1000.0);
    n.c(a, 0, 100e-9);
    if diodes {
        n.clipper(a, 0, ClipperModel::pair(D_1N914, 1));
    }
    let o = n.output(a);
    (n, o)
}

fn sine_gain(c: &mut NonlinearCircuit, f: f64, amp: f64, out: usize) -> f64 {
    let sr = c.sample_rate();
    let n = (sr * 0.2) as usize;
    let (mut ei, mut eo) = (0.0, 0.0);
    for k in 0..n {
        let x = amp * (2.0 * PI * f * k as f64 / sr).sin();
        c.set_input(0, x);
        c.process();
        if k > n / 2 {
            ei += x * x;
            eo += c.output(out).powi(2);
        }
    }
    10.0 * (eo / ei).log10()
}

#[test]
fn linear_part_matches_the_laplace_analysis() {
    let (net, o) = rc(false);
    let sr = 384000.0;
    for f in [200.0, 1591.5, 8000.0] {
        let mut c = NonlinearCircuit::new(&net, sr);
        c.init_dc();
        let fw = sr / PI * (PI * f / sr).tan();
        let want = db(c.ac_response(fw, 0, o));
        let got = sine_gain(&mut c, f, 1e-3, o);
        assert!((got - want).abs() < 0.01, "{f} Hz: {got} vs {want}");
    }
    // −3 dB at 1,591.5 Hz
    let c = NonlinearCircuit::new(&net, sr);
    assert!((db(c.ac_response(1591.55, 0, o)) + 3.0103).abs() < 0.01);
}

#[test]
fn diode_clipper_solves_every_sample_and_clips() {
    let (net, o) = rc(true);
    let mut c = NonlinearCircuit::new(&net, 384000.0);
    c.init_dc();
    let mut peak: f64 = 0.0;
    for k in 0..38400 {
        let x = 4.0 * (2.0 * PI * 1000.0 * k as f64 / 384000.0).sin();
        c.set_input(0, x);
        c.process();
        peak = peak.max(c.output(o).abs());
    }
    let s = c.stats();
    assert_eq!(s.unsolved, 0);
    assert!(peak > 0.5 && peak < 0.8, "clipped at {peak} V");
    assert!(s.mean() < 3.0, "mean iterations {}", s.mean());
}

#[test]
fn reference_tolerance_agrees_with_real_time() {
    // tolerance 1e-6 vs 1e-12: the same waveform
    let (net, o) = rc(true);
    let mut a = NonlinearCircuit::new(&net, 384000.0);
    let mut b = NonlinearCircuit::new(&net, 384000.0);
    b.set_tolerance(1e-12);
    a.init_dc();
    b.init_dc();
    let mut worst: f64 = 0.0;
    for k in 0..20000 {
        let x = 2.0 * (2.0 * PI * 700.0 * k as f64 / 384000.0).sin();
        a.set_input(0, x);
        b.set_input(0, x);
        a.process();
        b.process();
        worst = worst.max((a.output(o) - b.output(o)).abs());
    }
    // within the real-time tolerance (10⁻⁶ V, −120 dB re 1 V)
    assert!(worst < 1e-6, "{worst:e}");
}

/// A non-inverting op-amp stage (gain 1 + rf / 1k, 1k leg to a big cap), op-amp macro, then a
/// 1 kΩ + diode pair after a cut at the op-amp's output. Inputs: signal (riding on the bias), supply,
/// bias.
fn opamp_stage(model: OpAmpModel, rf: f64) -> (Netlist, usize, usize, usize) {
    let mut n = Netlist::new(3);
    let sig = n.input(0);
    let _vs = n.input(1);
    let vb = n.input(2);
    let plus = n.node();
    n.r(sig, plus, 1.0);
    let minus = n.node();
    let out = n.node();
    n.opamp(plus, minus, out, model, (0.0, 1.0), 0.0);
    let pot = n.rheostat(out, minus, rf, Taper::Linear, 1.0);
    let leg = n.node();
    n.r(minus, leg, 1000.0);
    n.c(leg, vb, 100e-6);
    let o1 = n.output(out);
    n.cut(out);
    let c = n.node();
    n.c(out, c, 10e-6);
    let d = n.node();
    n.r(c, d, 1000.0);
    n.clipper(d, 0, ClipperModel::pair(D_1N914, 1));
    let o2 = n.output(d);
    (n, o1, o2, pot)
}

fn biased(c: &mut NonlinearCircuit, x: f64) {
    c.set_input(0, 4.5 + x);
    c.set_input(1, 9.0);
    c.set_input(2, 4.5);
    c.set_supply(9.0);
    c.process();
}

#[test]
fn opamp_macro_slews_at_its_rate() {
    // unity gain (rf → R_MIN), a ±2 V square: the output slope is SR
    let (net, o1, _, _) = opamp_stage(LM308, 10.0);
    let sr = 768000.0;
    let mut c = NonlinearCircuit::new(&net, sr);
    c.set_input(0, 4.5);
    c.set_input(1, 9.0);
    c.set_input(2, 4.5);
    assert!(c.init_dc());
    let mut last = c.output(o1);
    assert!((last - 4.5).abs() < 1e-3, "DC out {last}");
    let mut slope: f64 = 0.0;
    for k in 0..(sr * 0.002) as usize {
        let x = if (k / 384) % 2 == 0 { 2.0 } else { -2.0 };
        biased(&mut c, x);
        let y = c.output(o1);
        slope = slope.max((y - last).abs() * sr);
        last = y;
    }
    let ratio = slope / LM308.sr;
    assert!(ratio > 0.97 && ratio < 1.03, "slew {:.3} V/µs", slope * 1e-6);
    assert_eq!(c.stats().unsolved, 0);
}

#[test]
fn opamp_macro_has_its_gain_bandwidth() {
    // gain 101 (40 dB): the closed loop is −3 dB at GBW / 101 ≈ 9.9 kHz for the LM308
    let (net, o1, _, _) = opamp_stage(LM308, 100e3);
    let mut c = NonlinearCircuit::new(&net, 768000.0);
    c.set_input(1, 9.0);
    c.set_input(2, 4.5);
    c.set_input(0, 4.5);
    c.init_dc();
    let lo = db(c.ac_response(500.0, 0, o1));
    let corner = db(c.ac_response(1e6 / 101.0, 0, o1));
    assert!((lo - 40.08).abs() < 0.1, "{lo}");
    assert!((lo - corner - 3.0).abs() < 0.3, "{lo} → {corner}");
    // and the simulation agrees with the analysis
    let got = sine_gain_biased(&mut c, 5000.0, 1e-4, o1);
    let want = db(c.ac_response(5000.0, 0, o1));
    assert!((got - want).abs() < 0.05, "{got} vs {want}");
}

fn sine_gain_biased(c: &mut NonlinearCircuit, f: f64, amp: f64, out: usize) -> f64 {
    let sr = c.sample_rate();
    let n = (sr * 0.1) as usize;
    let (mut ei, mut eo, mut mean) = (0.0, 0.0, 0.0);
    let mut ys = Vec::new();
    for k in 0..n {
        let x = amp * (2.0 * PI * f * k as f64 / sr).sin();
        biased(c, x);
        if k > n / 2 {
            ei += x * x;
            ys.push(c.output(out));
            mean += c.output(out);
        }
    }
    mean /= ys.len() as f64;
    for y in ys {
        eo += (y - mean).powi(2);
    }
    10.0 * (eo / ei).log10()
}

#[test]
fn cuts_are_exact() {
    // the cut graph and the monolithic solve agree to the Newton tolerance
    let (net, _, o2, _) = opamp_stage(TL072, 47e3);
    let sr = 384000.0;
    let mut a = NonlinearCircuit::new(&net, sr);
    let mut b = NonlinearCircuit::monolithic(&net, sr);
    assert_eq!(a.stage_count(), 2);
    assert_eq!(b.stage_count(), 1);
    for c in [&mut a, &mut b] {
        c.set_tolerance(1e-12);
        c.set_input(0, 4.5);
        c.set_input(1, 9.0);
        c.set_input(2, 4.5);
        c.init_dc();
    }
    let (mut worst, mut peak): (f64, f64) = (0.0, 0.0);
    for k in 0..40000 {
        let x = 0.2 * (2.0 * PI * 1000.0 * k as f64 / sr).sin();
        biased(&mut a, x);
        biased(&mut b, x);
        worst = worst.max((a.output(o2) - b.output(o2)).abs());
        peak = peak.max(a.output(o2).abs());
    }
    let null = 20.0 * (worst / peak).log10();
    assert!(null < -100.0, "cut vs monolithic {null:.1} dB");
}

#[test]
fn woodbury_matches_a_rebuild() {
    // a pot moved by Woodbury gives the circuit compiled at that value
    let sr = 384000.0;
    let (net, o1, _, pot) = opamp_stage(LM308, 100e3);
    let mut a = NonlinearCircuit::new(&net, sr);
    a.set_pot(pot, 0.25);
    let (net_b, _, _, _) = opamp_stage(LM308, 25e3);
    let b = NonlinearCircuit::new(&net_b, sr);
    for f in [100.0, 1000.0, 10000.0] {
        let ga = a.ac_response(f, 0, o1);
        let gb = b.ac_response(f, 0, o1);
        assert!((db(ga) - db(gb)).abs() < 1e-6, "{f}: {} vs {}", db(ga), db(gb));
    }
    // and in the time domain
    let mut a = NonlinearCircuit::new(&net, sr);
    let mut b = NonlinearCircuit::new(&net_b, sr);
    for c in [&mut a, &mut b] {
        c.set_input(0, 4.5);
        c.set_input(1, 9.0);
        c.set_input(2, 4.5);
    }
    a.set_pot(pot, 0.25);
    a.init_dc();
    b.init_dc();
    let mut worst: f64 = 0.0;
    for k in 0..20000 {
        let x = 0.01 * (2.0 * PI * 440.0 * k as f64 / sr).sin();
        biased(&mut a, x);
        biased(&mut b, x);
        worst = worst.max((a.output(o1) - b.output(o1)).abs());
    }
    assert!(worst < 1e-9, "{worst:e}");
}

#[test]
fn pot_ends_are_not_singular() {
    let (net, o1, _, pot) = opamp_stage(LM308, 100e3);
    let mut c = NonlinearCircuit::new(&net, 384000.0);
    c.set_input(0, 4.5);
    c.set_input(1, 9.0);
    c.set_input(2, 4.5);
    c.init_dc();
    for t in [0.0, 1.0, 0.0, 0.5] {
        c.set_pot(pot, t);
        for k in 0..2000 {
            biased(&mut c, 0.001 * (k as f64 * 0.01).sin());
            assert!(c.output(o1).is_finite());
        }
    }
    assert_eq!(c.stats().unsolved, 0);
}

#[test]
fn dc_start_is_silent() {
    // from the DC point, silence in → the outputs do not move
    let (net, o1, o2, _) = opamp_stage(LM308, 100e3);
    let mut c = NonlinearCircuit::new(&net, 384000.0);
    c.set_input(0, 4.5);
    c.set_input(1, 9.0);
    c.set_input(2, 4.5);
    c.set_supply(9.0);
    assert!(c.init_dc());
    let (y1, y2) = (c.output(o1), c.output(o2));
    let mut worst: f64 = 0.0;
    for _ in 0..10000 {
        biased(&mut c, 0.0);
        worst = worst.max((c.output(o1) - y1).abs()).max((c.output(o2) - y2).abs());
    }
    assert!(worst < 1e-6, "moved {worst:e} V");
    // a re-solve at another supply, warm-started from the seed, converges
    c.set_input(1, 13.5);
    c.set_input(2, 6.75);
    c.set_input(0, 6.75);
    c.set_supply(13.5);
    assert!(c.solve_dc());
    assert!((c.output(o1) - 6.75).abs() < 0.01, "{}", c.output(o1));
}

#[test]
fn bjt_common_emitter_biases_and_amplifies() {
    // 9 V, 1 MΩ / 100 kΩ divider... a simple CE stage: Rc 10 kΩ, Re 1 kΩ, base 82k / 10k
    let mut n = Netlist::new(2);
    let sig = n.input(0);
    let vs = n.input(1);
    let b = n.node();
    let cn = n.node();
    let e = n.node();
    let cin = n.node();
    n.r(sig, cin, 100.0);
    n.c(cin, b, 10e-6);
    n.r(vs, b, 82e3);
    n.r(b, 0, 10e3);
    n.r(vs, cn, 10e3);
    n.r(e, 0, 1e3);
    let bjt = BjtModel { npn: true, is: 1e-14, bf: 200.0, br: 4.0, leak: 1.0, eg: 1.11, xti: 3.0, tnom: 25.0, nf: 1.0, nr: 1.0 };
    n.bjt(cn, b, e, bjt);
    let oc = n.output(cn);
    let mut c = NonlinearCircuit::new(&n, 384000.0);
    c.set_input(1, 9.0);
    assert!(c.init_dc());
    let vc = c.output(oc);
    // V_B ≈ 0.96 V, V_E ≈ 0.3 V, I_C ≈ 0.3 mA, V_C ≈ 6 V
    assert!(vc > 5.0 && vc < 7.5, "collector at {vc}");
    let g = db(c.ac_response(1000.0, 0, oc));
    // ≈ −Rc / Re = 20 dB
    assert!((g - 20.0).abs() < 1.5, "{g}");
    // to_spice gives the same device
    let deck = n.to_spice("ce", &[0.0, 9.0], &[], 25.0, 9.0);
    assert!(deck.contains(".model Q") && deck.contains("NPN(IS=1e-14 BF=200 BR=4"), "{deck}");
}

#[test]
fn ebers_moll_leakage_is_the_collector_junction() {
    // open base: I_CEO = Is (1 + (βF + 1) / βR) grows with `leak`
    let mk = |leak: f64| {
        let mut n = Netlist::new(1);
        let vs = n.input(0);
        let cn = n.node();
        let b = n.node();
        n.r(vs, cn, 1e3);
        n.r(b, 0, 1e12);
        let m = BjtModel { npn: true, is: 1e-9, bf: 100.0, br: 2.0, leak, eg: 0.67, xti: 3.0, tnom: 25.0, nf: 1.0, nr: 1.0 };
        n.bjt(cn, b, 0, m);
        let o = n.output(cn);
        let mut c = NonlinearCircuit::new(&n, 48000.0);
        c.set_input(0, 9.0);
        c.init_dc();
        (9.0 - c.output(o)) / 1e3
    };
    let (i1, i2) = (mk(1.0), mk(2.0));
    assert!(i2 > 1.8 * i1, "{i1:e} → {i2:e}");
}

#[test]
fn spice_deck_has_every_card() {
    let (net, _, _, _) = opamp_stage(LM308, 100e3);
    let deck = net.to_spice("op-amp stage", &[4.5, 9.0, 4.5], &[0.5], 25.0, 9.0);
    for card in [".temp 25", "VIN0", "VIN1", "VIN2", "tanh(", ".model", ".op", ".end", "C"] {
        assert!(deck.contains(card), "missing {card} in\n{deck}");
    }
}

#[test]
fn deterministic() {
    let run = || {
        let (net, _, o2, _) = opamp_stage(TL072, 470e3);
        let mut c = NonlinearCircuit::new(&net, 384000.0);
        c.set_input(0, 4.5);
        c.set_input(1, 9.0);
        c.set_input(2, 4.5);
        c.init_dc();
        (0..20000)
            .map(|k| {
                biased(&mut c, 0.5 * (k as f64 * 0.02).sin());
                c.output(o2)
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(run(), run());
}

#[test]
fn fast_opamp_rail_to_rail_is_solved() {
    // a TL072 at gain 48 driven to both rails by a +12 dBFS square: the hard case of the review
    let (net, o1, _, _) = opamp_stage(TL072, 47e3);
    let mut c = NonlinearCircuit::new(&net, 384000.0);
    c.set_input(0, 4.5);
    c.set_input(1, 9.0);
    c.set_input(2, 4.5);
    c.init_dc();
    let (mut hi, mut lo): (f64, f64) = (0.0, 9.0);
    for k in 0..38400 {
        let x = if (k / 192) % 2 == 0 { 4.0 } else { -4.0 };
        biased(&mut c, x);
        hi = hi.max(c.output(o1));
        lo = lo.min(c.output(o1));
    }
    let s = c.stats();
    assert_eq!(s.unsolved, 0, "{s:?}");
    assert!(hi > 7.3 && hi < 7.6 && lo > 1.4 && lo < 1.7, "rails {lo} – {hi}");
    eprintln!("TL072 square: mean {:.2} it, worst {}, fallbacks {}", s.mean(), s.worst, s.fallbacks);
}

#[test]
#[ignore = "timing"]
fn timing() {
    use std::time::Instant;
    let t = Instant::now();
    let mut acc = 0.0;
    for k in 0..10_000_000 {
        acc += fast_exp(-(k as f64) * 1e-6);
    }
    eprintln!("fast_exp {:.2} ns ({acc:.1})", t.elapsed().as_secs_f64() * 1e9 / 1e7);
    let (net, o) = rc(false);
    let mut c = NonlinearCircuit::new(&net, 384000.0);
    c.init_dc();
    let t = Instant::now();
    for k in 0..1_000_000 {
        c.set_input(0, (k as f64 * 0.01).sin());
        c.process();
        acc += c.output(o);
    }
    eprintln!("RC: {:.1} ns / sample", t.elapsed().as_secs_f64() * 1e9 / 1e6);
    let t = Instant::now();
    for k in 0..1_000_000 {
        acc += (k as f64 * 0.01).sin();
    }
    eprintln!("sin: {:.1} ns / sample", t.elapsed().as_secs_f64() * 1e9 / 1e6);
    let ev = evaluate(&DeviceModel::Clipper(ClipperModel::pair(D_1N914, 1)), (0.0, 0.0), 25.0, 9.0);
    let devs = [(0usize, 0usize, ev)];
    let kk = [-1000.0];
    let nw = Newton { k: &kk, m: 1, devs: &devs, tol: 1e-6, dc: false, plain_steps: false, blk: blocks(&devs), recip: false };
    let t = Instant::now();
    let mut its = 0;
    for k in 0..1_000_000 {
        let mut v = [0.0; MAX_PORTS];
        v[0] = 0.3;
        let mut i = [0.0; MAX_PORTS];
        let p = [0.3 + 0.01 * (k % 7) as f64];
        let r = nw.solve(&p, &mut v, &mut i, 20);
        its += r.1;
        acc += v[0];
    }
    eprintln!("Newton 1-port: {:.1} ns / solve, {} it", t.elapsed().as_secs_f64() * 1e9 / 1e6, its as f64 / 1e6);
    let t = Instant::now();
    for k in 0..1_000_000 {
        let v = [0.3 + 1e-7 * k as f64; MAX_PORTS];
        let mut i = [0.0; MAX_PORTS];
        let mut j = [0.0; 4];
        ev.eval(&v, &mut i, &mut j, 1, 0);
        acc += i[0];
    }
    eprintln!("eval: {:.1} ns", t.elapsed().as_secs_f64() * 1e9 / 1e6);
    let (net, o) = rc(true);
    let mut c = NonlinearCircuit::new(&net, 384000.0);
    c.init_dc();
    let t = Instant::now();
    for k in 0..1_000_000 {
        c.set_input(0, (k as f64 * 0.01).sin());
        c.process();
        acc += c.output(o);
    }
    eprintln!("RC + pair: {:.1} ns / sample, {:?}", t.elapsed().as_secs_f64() * 1e9 / 1e6, c.stats());
    let (net, _, o2, _) = opamp_stage(LM308, 100e3);
    let mut c = NonlinearCircuit::new(&net, 384000.0);
    c.set_input(1, 9.0);
    c.set_input(2, 4.5);
    c.set_input(0, 4.5);
    c.set_supply(9.0);
    c.init_dc();
    let t = Instant::now();
    for k in 0..1_000_000 {
        c.set_input(0, 4.5 + 0.1 * (k as f64 * 0.01).sin());
        c.process();
        acc += c.output(o2);
    }
    eprintln!("op-amp + pair: {:.1} ns / sample, {:?} {acc}", t.elapsed().as_secs_f64() * 1e9 / 1e6, c.stats());
}

#[test]
fn fast_lu_agrees_with_the_plain_one() {
    // the opt-in factor / apply pair against lu_solve, pivoting forced (small diagonal)
    let mut s = 7u32;
    let mut rnd = || {
        s = s.wrapping_mul(1664525).wrapping_add(1013904223);
        (s >> 8) as f64 / 8388608.0 - 1.0
    };
    for _ in 0..50 {
        let mut a = [0.0; 25];
        for x in a.iter_mut() {
            *x = rnd();
        }
        let b0: Vec<f64> = (0..5).map(|_| rnd()).collect();
        let (mut a1, mut b1) = (a, b0.clone());
        assert!(lu_solve(&mut a1, &mut b1, 5));
        let mut a2 = a;
        let (mut perm, mut inv) = ([0usize; MAX_PORTS], [0.0; MAX_PORTS]);
        assert!(lu_factor::<5>(&mut a2, &mut perm, &mut inv));
        let mut b2 = b0.clone();
        lu_apply::<5>(&a2, &perm, &inv, &mut b2);
        for (name, x) in [("plain", &b1), ("fast", &b2)] {
            for r in 0..5 {
                let ax: f64 = (0..5).map(|c| a[r * 5 + c] * x[c]).sum();
                assert!((ax - b0[r]).abs() < 1e-9, "{name}: row {r}: {ax} vs {}", b0[r]);
            }
        }
    }
}

#[test]
#[ignore = "diagnostics: the cost of Newton's parts"]
fn newton_parts_cost() {
    let best = |n: usize, f: &mut dyn FnMut()| -> f64 {
        let mut b = f64::MAX;
        for _ in 0..5 {
            let t = std::time::Instant::now();
            f();
            b = b.min(t.elapsed().as_secs_f64() * 1e9 / n as f64);
        }
        b
    };
    let m = DeviceModel::Bjt(BjtModel { npn: false, is: 5e-6, bf: 100.0, br: 4.5, leak: 1.0, eg: 0.67, xti: 3.0, tnom: 22.8, nf: 1.168, nr: 1.171 });
    let ev = evaluate(&m, (0.0, 0.0), 25.0, 9.0);
    let mut v = [-0.2, 4.0, 0.0, 0.0];
    let mut i = [0.0; 4];
    let mut j = [0.0; 16];
    let n = 1_000_000;
    let t = best(n, &mut || {
        for k in 0..n {
            v[0] = -0.2 - 1e-9 * (k & 7) as f64;
            ev.eval(&v, &mut i, &mut j, 4, 0);
            ev.eval(&v, &mut i, &mut j, 4, 2);
        }
    });
    eprintln!("2 BJT evals: {t:.1} ns ({})", i[0]);
    let mut a0 = [0.0; 16];
    for r in 0..4 {
        for c in 0..4 {
            a0[r * 4 + c] = if r == c { 4.0 } else { 0.3 * (r + 2 * c) as f64 - 1.0 };
        }
    }
    let mut acc = 0.0;
    let t = best(n, &mut || {
        for k in 0..n {
            let mut a = a0;
            let mut b = [1.0, 2.0, 3.0, k as f64];
            lu_solve(&mut a, &mut b, 4);
            acc += b[0];
        }
    });
    eprintln!("LU 4: {t:.1} ns ({acc})");
    let mut a10 = [0.0; 100];
    for r in 0..10 {
        for c in 0..10 {
            a10[r * 10 + c] = if r == c { 4.0 } else { 0.03 * (r + 2 * c) as f64 - 0.1 };
        }
    }
    let t = best(n / 10, &mut || {
        for k in 0..n / 10 {
            let mut a = a10;
            let mut b = [1.0, 2.0, 3.0, k as f64, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
            lu_solve(&mut a, &mut b, 10);
            acc += b[0];
        }
    });
    eprintln!("LU 10: {t:.1} ns ({acc})");
}
