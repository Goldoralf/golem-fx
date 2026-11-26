// SPDX-License-Identifier: GPL-3.0-or-later
//! The seven netlists (README §2.1, §2.11), each from one published clone schematic named in its
//! comment.
//! Every circuit has the same inputs: u₀ = the signal at the input jack (0 dBFS = 1 V), u₁ = the
//! supply, u₂ = the bias (supply / 2, an ideal reference: the divider's follower is impedance only).

use gfx_dsp::circuit_nl::*;
use gfx_dsp::pot::Taper;

pub const IN_SIG: usize = 0;
pub const IN_VS: usize = 1;
pub const IN_VB: usize = 2;

/// What the effect needs to drive one circuit.
#[derive(Clone, Debug)]
pub struct Circuit {
    pub net: Netlist,
    /// The output (the level pot's wiper, fully up, or the 1 MΩ load).
    pub out: usize,
    /// The pots on `gain`: (pot, reversed).
    pub gain: Vec<(usize, bool)>,
    /// The pots on `tone_stack`: (pot, reversed): 0 % = darkest on every circuit.
    pub tone: Vec<(usize, bool)>,
    /// The clipping devices `diodes` swaps, with the circuit's own model and the count per side.
    pub clippers: Vec<(usize, ClipperModel, u32)>,
    /// The op-amps `opamp` swaps, with the circuit's own part.
    pub opamps: Vec<(usize, OpAmpModel)>,
    /// The `hp` switch (a variable resistor: R when on, open when off).
    pub hp: Option<(usize, f64)>,
    /// The `fat` switch (a variable resistor in series with a capacitor: R when on, FAT_OFF when off).
    pub fat: Option<(usize, f64)>,
    /// The pots on `contour`: (pot, reversed).
    pub contour: Vec<(usize, bool)>,
    /// The circuit's own supply (V).
    pub supply: f64,
    /// Half-band stages at 44.1 / 48 kHz (one fewer per doubling of the host rate).
    pub stages: usize,
    /// The rate of the fast variant, which the swaps in `fast_opamps` / `fast_diodes` run on (a
    /// faster part aliases more: README §2.5, §2.11 "a swap that misses −65 dB runs at 16×").
    pub ideal_stages: usize,
    /// The `opamp` options that run on the fast variant (when its rate differs).
    pub fast_opamps: Vec<usize>,
    /// The `diodes` options that run on the fast variant (when its rate differs).
    pub fast_diodes: Vec<usize>,
    /// The element mark and the node where the post-clipper network starts (`bass_keep`'s twin).
    pub twin: (usize, Node),
    /// The small-signal polarity of the pre-clipper path (the twin's sign).
    pub polarity: f64,
}

/// The booster's NPN (2SC2240-class low-noise small-signal part) [U: generic constants, hFE in the
/// datasheet's GR range].
pub const Q_2SC2240: BjtModel = BjtModel { npn: true, is: 2e-14, bf: 400.0, br: 4.0, leak: 1.0, eg: 1.11, xti: 3.0, tnom: 25.0, nf: 1.0, nr: 1.0 };

/// The MOSFET clipper side: the 2N7000's body diode with its diode-connected channel in parallel.
pub fn mosfet_side(count: u32) -> ClipSide {
    ClipSide { j: D_2N7000_BODY, count, extra: None, channel: Some(CH_2N7000) }
}

/// A clipper that does not conduct (`diodes` None, and the placeholder before the part is set).
pub fn lifted() -> ClipperModel {
    ClipperModel::NONE
}

/// **Op-Amp Dist** — Aion FX *Helios* documentation, checked against GGG *Rodent* (the LM308 /
/// 1N914 original). Stage 1: the input network and the LM308 gain stage (100 kΩ log distortion pot
/// as a rheostat, 100 pF across, two RC legs); cut at the op-amp's output (rule a). Stage 2: 4.7 µF,
/// 1 kΩ, the 1N914 pair to ground, the filter (100 kΩ log pot reversed + 1.5 kΩ into 3.3 nF), the
/// JFET follower (ideal, its 22 nF and 1 MΩ kept), 1 µF into the 100 kΩ volume pot fully up.
pub fn opamp_dist() -> Circuit {
    let mut n = Netlist::new(3);
    let sig = n.input(IN_SIG);
    let _vs = n.input(IN_VS);
    let vb = n.input(IN_VB);
    let a = n.named("in_bias");
    let plus = n.named("plus");
    let minus = n.named("minus");
    let out = n.named("opamp_out");
    let l1 = n.node();
    let l2 = n.node();
    n.c(sig, a, 22e-9);
    n.r(a, vb, 1e6);
    n.r(a, plus, 1e3);
    n.c(plus, 0, 1e-9);
    let op = n.opamp(plus, minus, out, LM308, (0.0, 1.0), 0.0);
    let gain = n.rheostat(out, minus, 100e3, Taper::Log, 0.5);
    // README §2.2 feared a 4 ns mode here (100 pF on the 43 Ω legs) that the trapezoidal rule would
    // ring at half the rate: not measured (tests::stiff_modes_stay_quiet), so it stays trapezoidal
    n.c(out, minus, 100e-12);
    n.r(minus, l1, 560.0);
    n.c(l1, 0, 4.7e-6);
    n.r(minus, l2, 47.0);
    n.c(l2, 0, 2.2e-6);
    n.cut(out);
    let mark = n.mark();
    let c = n.node();
    let d = n.named("diodes");
    n.c(out, c, 4.7e-6);
    n.r(c, d, 1e3);
    let dio = n.clipper(d, 0, ClipperModel::pair(D_1N914, 1));
    let x = n.node();
    let f = n.named("filter");
    let tone = n.rheostat(d, x, 100e3, Taper::Log, 0.5);
    n.r(x, f, 1.5e3);
    n.c(f, 0, 3.3e-9);
    let g = n.named("gate");
    n.c(f, g, 22e-9);
    n.r(g, vb, 1e6);
    let s = n.node();
    n.buffer(g, s);
    let o = n.named("out");
    n.c(s, o, 1e-6);
    n.r(o, 0, 100e3);
    let out_k = n.output(o);
    Circuit {
        net: n,
        out: out_k,
        gain: vec![(gain, false)],
        tone: vec![(tone, true)],
        clippers: vec![(dio, ClipperModel::pair(D_1N914, 1), 1)],
        opamps: vec![(op, LM308)],
        hp: None,
        supply: 9.0,
        stages: 3,
        ideal_stages: 4,
        fast_opamps: vec![5],
        fast_diodes: vec![],
        twin: (mark, out),
        polarity: 1.0,
        fat: None,
        contour: vec![],
    }
}

/// **Booster Dist** — Aion FX *Comet* v1.1 documentation, the dual-op-amp topology with the pre-1994
/// values (470 nF coupling, 1 µF, 250 pF). Stage 1: the input follower (ideal, its 1 kΩ, 47 nF and
/// 470 kΩ bias kept; 47 nF into the booster [U: the inter-stage coupling value]) and the common-emitter
/// booster (10 kΩ collector, 22 Ω emitter, 470 kΩ ∥ 250 pF collector → base, 100 kΩ base → ground),
/// 470 nF into 100 kΩ to the bias at the op-amp's + input: the cut (rule b — not at the collector).
/// Stage 2: the LM358-class op-amp, the 100 kΩ linear distortion pot as a three-terminal divider
/// (output → pot → 4.7 kΩ + 1 µF to ground, wiper to −), 250 pF output → wiper; cut at its output.
/// Stage 3: 470 nF, 2.2 kΩ into the 1N4148 pair with 10 nF across, the tone stack (low-pass arm
/// 6.8 kΩ / 100 nF, high-pass arm 22 nF / 6.8 kΩ / 2.2 kΩ, crossfaded by the 20 kΩ linear pot) [U: the
/// arms' exact wiring], the 100 kΩ level pot fully up, the output follower (ideal) into 1 MΩ.
pub fn booster_dist() -> Circuit {
    let mut n = Netlist::new(3);
    let sig = n.input(IN_SIG);
    let vs = n.input(IN_VS);
    let vb = n.input(IN_VB);
    // the input follower
    let a = n.node();
    let g = n.named("follower_gate");
    n.r(sig, a, 1e3);
    n.c(a, g, 47e-9);
    n.r(g, vb, 470e3);
    let e1 = n.node();
    n.buffer(g, e1);
    // the booster
    let b = n.named("booster_base");
    let c = n.named("booster_collector");
    let e = n.node();
    n.c(e1, b, 47e-9);
    n.r(b, 0, 100e3);
    n.r(c, b, 470e3);
    n.c(c, b, 250e-12);
    n.r(vs, c, 10e3);
    n.r(e, 0, 22.0);
    n.bjt(c, b, e, Q_2SC2240);
    let plus = n.named("opamp_plus");
    n.c(c, plus, 470e-9);
    n.r(plus, vb, 100e3);
    n.cut(plus);
    // the op-amp stage
    let minus = n.named("minus");
    let out = n.named("opamp_out");
    let op = n.opamp(plus, minus, out, LM358, (0.0, 1.0), 0.0);
    let leg = n.node();
    let lc = n.node();
    let gain = n.pot(out, minus, leg, 100e3, Taper::Linear, 0.5);
    n.c(out, minus, 250e-12);
    n.r(leg, lc, 4.7e3);
    n.c(lc, 0, 1e-6);
    n.cut(out);
    // clipper, tone stack, level, output
    let mark = n.mark();
    let x = n.node();
    let d = n.named("diodes");
    n.c(out, x, 470e-9);
    n.r(x, d, 2.2e3);
    let dio = n.clipper(d, 0, ClipperModel::pair(D_1N914, 1));
    n.c(d, 0, 10e-9);
    let lp = n.named("tone_lp");
    n.r(d, lp, 6.8e3);
    n.c(lp, 0, 100e-9);
    let hp = n.named("tone_hp");
    let hp2 = n.node();
    n.c(d, hp, 22e-9);
    n.r(hp, 0, 6.8e3);
    n.r(hp, hp2, 2.2e3);
    let w = n.named("tone_wiper");
    let tone = n.pot(lp, w, hp2, 20e3, Taper::Linear, 0.5);
    n.r(w, 0, 100e3); // the level pot, fully up
    let s = n.node();
    n.buffer(w, s);
    let o = n.named("out");
    n.c(s, o, 1e-6);
    n.r(o, 0, 1e6);
    let out_k = n.output(o);
    Circuit {
        net: n,
        out: out_k,
        gain: vec![(gain, false)],
        tone: vec![(tone, false)],
        clippers: vec![(dio, ClipperModel::pair(D_1N914, 1), 1)],
        opamps: vec![(op, LM358)],
        hp: None,
        supply: 9.0,
        stages: 3,
        ideal_stages: 3,
        fast_opamps: vec![5],
        fast_diodes: vec![],
        twin: (mark, out),
        polarity: -1.0,
        fat: None,
        contour: vec![],
    }
}

/// **Blend OD** — Aion FX *Refractor* documentation, original values (input bias 1 MΩ, 1N34A).
/// Stage 1: the input buffer (ideal; 100 nF, 1 MΩ to the bias kept). Stage 2: the TL072 gain stage
/// on plain 9 V, 422 kΩ ∥ 390 pF feedback, 15 kΩ ∥ 82 nF + 2 kΩ + gang A (100 kΩ linear, rheostat:
/// 100 kΩ at minimum gain) in the leg to the bias; its + input from the buffer through 100 nF and
/// 10 kΩ with 1 MΩ to the bias [U]. Stage 3: 1 µF, 1 kΩ, the 1N34A pair to ground, 1 µF, 47 kΩ into
/// the summing node; gang B fading the full-range clean network (27 kΩ; 12 kΩ + 27 nF; 22 kΩ + 2.2 nF
/// to the diode side of the 47 kΩ) to nothing at maximum gain; the fixed low-passed path (1.5 kΩ,
/// 1 µF to the bias, 15 kΩ); the inverting summing amp (TL072 on the pumped +18 / −9 V,
/// 392 kΩ ∥ 820 pF). Then the treble stage (inverting, unity, 10 kΩ linear pot and 3.9 nF; it stays
/// linear on 27 V: an ideal op-amp) and the 10 kΩ output pot fully up. The buffer feeds both the gain
/// stage and the sum: a directed graph, cut at the buffer's and the gain stage's outputs (rule a).
pub fn blend_od() -> Circuit {
    let mut n = Netlist::new(3);
    let sig = n.input(IN_SIG);
    let _vs = n.input(IN_VS);
    let vb = n.input(IN_VB);
    let g = n.named("buffer_in");
    n.c(sig, g, 100e-9);
    n.r(g, vb, 1e6);
    let buf = n.named("buffer_out");
    n.buffer(g, buf);
    n.cut(buf);
    // the gain stage
    let pi = n.node();
    let plus = n.named("gain_plus");
    n.c(buf, pi, 100e-9);
    n.r(pi, plus, 10e3);
    n.r(plus, vb, 1e6);
    let minus = n.named("gain_minus");
    let out = n.named("gain_out");
    let op1 = n.opamp(plus, minus, out, TL072, (0.0, 1.0), 0.0);
    n.r(out, minus, 422e3);
    n.c(out, minus, 390e-12);
    let l1 = n.node();
    let l2 = n.node();
    n.r(minus, l1, 15e3);
    n.c(minus, l1, 82e-9);
    n.r(l1, l2, 2e3);
    let gang_a = n.rheostat(l2, vb, 100e3, Taper::Linear, 0.5);
    n.cut(out);
    // clipper, clean feeds, sum
    let mark = n.mark();
    let x = n.node();
    let d = n.named("diodes");
    n.c(out, x, 1e-6);
    n.r(x, d, 1e3);
    let dio = n.clipper(d, 0, ClipperModel::pair(D_1N34A, 1));
    let y = n.named("clip_out");
    n.c(d, y, 1e-6);
    let sum = n.named("sum");
    n.r(y, sum, 47e3);
    let wb = n.named("gang_b");
    let gang_b = n.pot(buf, wb, vb, 100e3, Taper::Linear, 0.5);
    n.r(wb, sum, 27e3);
    let k1 = n.node();
    n.r(wb, k1, 12e3);
    n.c(k1, sum, 27e-9);
    let k2 = n.node();
    n.r(wb, k2, 22e3);
    n.c(k2, y, 2.2e-9);
    let z = n.node();
    n.r(buf, z, 1.5e3);
    n.c(z, vb, 1e-6);
    n.r(z, sum, 15e3);
    let so = n.named("sum_out");
    let op2 = n.opamp(vb, sum, so, TL072, (-1.0, 2.0), 0.0);
    n.r(so, sum, 392e3);
    n.c(so, sum, 820e-12);
    // treble (linear) and output
    let tm = n.named("treble_minus");
    let to = n.named("treble_out");
    n.r(so, tm, 10e3);
    n.r(to, tm, 10e3);
    n.opamp_ideal(vb, tm, to);
    let tw = n.node();
    let treble = n.pot(to, tw, so, 10e3, Taper::Linear, 0.5);
    n.c(tw, tm, 3.9e-9);
    let o = n.named("out");
    n.c(to, o, 4.7e-6);
    n.r(o, 0, 10e3);
    let out_k = n.output(o);
    Circuit {
        net: n,
        out: out_k,
        gain: vec![(gang_a, true), (gang_b, false)],
        tone: vec![(treble, false)],
        clippers: vec![(dio, ClipperModel::pair(D_1N34A, 1), 1)],
        opamps: vec![(op1, TL072), (op2, TL072)],
        hp: None,
        supply: 9.0,
        stages: 3,
        ideal_stages: 3,
        fast_opamps: vec![5],
        fast_diodes: vec![],
        twin: (mark, out),
        polarity: 1.0,
        fat: None,
        contour: vec![],
    }
}

/// **MOSFET OD** — PedalParts *Disorder* documentation, v1.4 (1 MΩ log drive, 10 kΩ tone, 47 nF, no
/// germanium). No input buffer: 22 nF, 10 kΩ, 470 kΩ to the bias. Stage 1: TL082, drive pot (1 MΩ log,
/// rheostat) + 18 kΩ with 220 pF across, leg 2.2 kΩ + 100 nF; cut at its output (rule a). Stage 2:
/// 10 kΩ into the two 2N7000 back to back to the bias, 1 nF across, then 10 kΩ to stage 2's + input:
/// the cut (rule b). Stage 3: TL082, 150 kΩ ∥ 220 pF over 39 kΩ + 100 nF; the passive output: 1 µF,
/// 33 kΩ (the HP switch parallels 22 kΩ), the shunt 47 nF + the 10 kΩ tone rheostat, the 500 kΩ volume
/// pot fully up, into a 1 MΩ load.
pub fn mosfet_od() -> Circuit {
    let mut n = Netlist::new(3);
    let sig = n.input(IN_SIG);
    let _vs = n.input(IN_VS);
    let vb = n.input(IN_VB);
    let a = n.node();
    let plus = n.named("plus1");
    n.c(sig, a, 22e-9);
    n.r(a, plus, 10e3);
    n.r(plus, vb, 470e3);
    let minus = n.named("minus1");
    let out = n.named("out1");
    let op1 = n.opamp(plus, minus, out, TL072, (0.0, 1.0), 0.0);
    let f = n.node();
    n.r(out, f, 18e3);
    let drive = n.rheostat(f, minus, 1e6, Taper::Log, 0.5);
    n.c(out, minus, 220e-12);
    let l = n.node();
    n.r(minus, l, 2.2e3);
    n.c(l, 0, 100e-9);
    n.cut(out);
    // the clipper
    let d = n.named("diodes");
    n.r(out, d, 10e3);
    let own = ClipperModel { pos: Some(mosfet_side(1)), neg: Some(mosfet_side(1)) };
    let dio = n.clipper(d, vb, own);
    n.c(d, vb, 1e-9);
    let p2 = n.named("plus2");
    n.r(d, p2, 10e3);
    n.cut(p2);
    // stage 2 and the output network
    let mark = n.mark();
    let m2 = n.named("minus2");
    let o2 = n.named("out2");
    let op2 = n.opamp(p2, m2, o2, TL072, (0.0, 1.0), 0.0);
    n.r(o2, m2, 150e3);
    n.c(o2, m2, 220e-12);
    let l2 = n.node();
    n.r(m2, l2, 39e3);
    n.c(l2, 0, 100e-9);
    let q = n.node();
    n.c(o2, q, 1e-6);
    let r = n.named("tone_node");
    n.r(q, r, 33e3);
    let hp = n.var(q, r, 1e9, 22e3, 1e9);
    let t = n.node();
    n.c(r, t, 47e-9);
    let tone = n.rheostat(t, 0, 10e3, Taper::Linear, 0.5);
    n.r(r, 0, 500e3); // the volume pot, fully up
    n.r(r, 0, 1e6); // the next device
    let out_k = n.output(r);
    Circuit {
        net: n,
        out: out_k,
        gain: vec![(drive, false)],
        tone: vec![(tone, false)],
        clippers: vec![(dio, own, 1)],
        opamps: vec![(op1, TL072), (op2, TL072)],
        hp: Some((hp, 22e3)),
        supply: 9.0,
        stages: 3,
        ideal_stages: 3,
        fast_opamps: vec![5],
        fast_diodes: vec![],
        twin: (mark, p2),
        polarity: 1.0,
        fat: None,
        contour: vec![],
    }
}

// ---------------------------------------------------------------------------------------------
// Added 2026-11-26 (roadmap 0.8, item 3; README §2.11)

/// 2SK184-GR / 2SK209-GR class N-JFET [U: generic constants, README §2.11].
pub const JF_2SK184: JfetModel = JfetModel { n_channel: true, vto: -0.45, beta: 18e-3, lambda: 4e-3, is: 1e-14, n: 1.0, vtotc: 0.0, betatce: 0.0, tnom: 25.0, soft: 0.0 };
/// The discrete op-amp's input pair as one port: the 2SK184 pair on its 4.7 kΩ tail, the tail
/// following the common mode (0.93 mA at the DC point: the seed).
pub const PAIR_2SK184: PairModel = PairModel { beta: 18e-3, i_tail: 0.93e-3, tail_r: 4.7e3, vto: -0.45 };
/// 2SA1335-GR / 2N3906 class PNP, the discrete op-amp's class-A output [U: generic constants].
pub const Q_2SA1335: BjtModel = BjtModel { npn: false, is: 2e-14, bf: 300.0, br: 4.0, leak: 1.0, eg: 1.11, xti: 3.0, tnom: 25.0, nf: 1.0, nr: 1.0 };
/// 1N4001-class rectifier used as a clipper (Is 14 nA, N 1.98) [U].
pub const D_1N4001: Junction = Junction::silicon(1.4e-8, 1.98);
/// The Plexi Dist's unmarked germanium signal diode: the generic germanium constants (as 1N34A) [U].
pub const D_GE_SIGNAL: Junction = D_1N34A;
/// LM833: 7 V/µs, 15 MHz, ≈ 1.5 V from each rail [S, datasheet; U on 9 V].
pub const LM833: OpAmpModel = OpAmpModel { sr: 7e6, gbw: 15e6, a0: 1e5, sat_hi: 1.5, sat_lo: 1.5, square_law: false };

/// `fat` off: the 83 nF behind 22 MΩ (README §2.11: it follows a moved bias in 1.8 s, as behind a
/// real switch's pull-down, and the off position is the stock circuit within 0.1 dB at 20 Hz).
pub const FAT_OFF: f64 = 22e6;
/// `fat` on.
pub const FAT_ON: f64 = R_MIN;

/// An emitter follower biased at `ie` (A), linearized: an ideal buffer, r_e = V_T / I_E in series,
/// r_π = β r_e from base to emitter (the base current). The followers whose signal is far below
/// their bias (the gyrators' Q8 and Q2, README §2.11) are linear stamps; their DC offset is blocked
/// by the gyrators' capacitors.
fn follower(n: &mut Netlist, b: Node, e: Node, ie: f64, beta: f64) {
    let re = thermal_voltage(25.0) / ie;
    let x = n.node();
    n.buffer(b, x);
    n.r(x, e, re);
    n.r(b, e, beta * re);
}

/// One discrete op-amp (README §2.11): the 2SK184 pair (its + input `plus`, − input `minus`), R6
/// 2.2 kΩ from VA to the + device's drain, the PNP (base on that drain, emitter at VA, collector =
/// `out`, 2.2 kΩ to ground) and its Miller capacitor `cc`. `transistor_level`: the two JFETs on the
/// 4.7 kΩ tail (the offline reference) instead of the pair port.
fn discrete_opamp(n: &mut Netlist, va: Node, plus: Node, minus: Node, out: Node, cc: f64, transistor_level: bool) {
    let d = n.node();
    if transistor_level {
        let s = n.node();
        n.jfet(d, plus, s, JF_2SK184);
        n.jfet(va, minus, s, JF_2SK184);
        n.r(s, 0, 4.7e3);
    } else {
        n.jfet_pair(plus, minus, d, PAIR_2SK184);
    }
    n.r(va, d, 2.2e3);
    n.bjt(out, d, va, Q_2SA1335);
    // trapezoidal, as every capacitor here: backward Euler on this one or on the 47 / 100 pF across
    // the feedback removes a +4 dB excess in 0.4 – 0.5 fs (tests::stiff_modes_stay_quiet, a band the
    // decimator removes) but costs 3 – 6 dB of alias and 0.4 dB at 8 kHz (README, As coded)
    n.c(d, out, cc);
    n.r(out, 0, 2.2e3);
}

/// **Discrete OD** — Aion FX *Sapphire* documentation v1.0.1 (stock values: no Bass pot, the 250 kΩ
/// dual gang, the 1N914s). u₁ = VA (≈ 0.9 × the jack's supply, after the 1N5817 and the capacitance
/// multiplier), u₂ = VB = VA / 2 (buffered: ideal). Q1 (input follower) ideal with its 10 kΩ, 47 nF,
/// 1 MΩ and 100 nF / 220 kΩ kept. Stage 1: discrete op-amp 1 (47 pF), gang 1 (250 kΩ log rheostat) +
/// 22 kΩ with 47 pF across, leg 1.5 kΩ + 150 nF; the fixed Fender stack (100 kΩ slope, 220 pF + 330 kΩ,
/// 100 nF, 1 MΩ, 47 nF + 15 kΩ) and the two series 1N914 pairs to ground, 2.2 nF into Q5's gate
/// (1 MΩ to VB): the cut (rule b, the pair draws no gate current). Stage 2: discrete op-amp 2
/// (100 pF), gang 2 + 33 kΩ with 100 pF across, leg 2.2 kΩ + 1 µF; the 5 kHz trim (5.6 kΩ ∥ 6.8 nF,
/// 5.6 nF), 18 nF (`fat`: ∥ 83 nF behind a switch resistor) into the 10 kΩ linear tone pot, 18 nF to
/// ground, the level pot **fixed at 50 %** (100 kΩ log: 90.9 kΩ / 9.09 kΩ; README §2.1, the IC stage
/// stays linear), 47 nF into IC1B's + input (470 kΩ to VB): the cut (rule b). Stage 3, linear: IC1B
/// ideal with 6.8 kΩ ∥ 2.2 nF, its gyrator (56 nF, 56 nF, 470 kΩ, Q8 linearized at 0.31 mA, 10 kΩ,
/// 1.2 kΩ), 1 µF, Q9 ideal (100 kΩ), 1 µF into 100 kΩ, 1 kΩ, a 1 MΩ load. D6 / D7 across IC1B's
/// inputs never conduct in a linear stage: left out.
pub fn discrete_od() -> Circuit {
    discrete_od_with(false)
}

/// The Discrete OD with its discrete op-amps at transistor level (the offline reference, README §5).
pub fn discrete_od_with(transistor_level: bool) -> Circuit {
    let mut n = Netlist::new(3);
    let sig = n.input(IN_SIG);
    let va = n.input(IN_VS);
    let vb = n.input(IN_VB);
    let a = n.node();
    let g1 = n.named("q1_gate");
    n.r(sig, a, 10e3);
    n.c(a, g1, 47e-9);
    n.r(g1, vb, 1e6);
    let s1 = n.named("q1_source");
    n.buffer(g1, s1);
    let g2 = n.named("q2_gate");
    n.c(s1, g2, 100e-9);
    n.r(g2, vb, 220e3);
    // stage 1
    let g3 = n.named("stage1_minus");
    let o1 = n.named("stage1_out");
    discrete_opamp(&mut n, va, g2, g3, o1, 47e-12, transistor_level);
    let x1 = n.node();
    let gang1 = n.rheostat(o1, x1, 250e3, Taper::Log, 0.5);
    n.r(x1, g3, 22e3);
    n.c(g3, o1, 47e-12);
    let y1 = n.node();
    n.c(g3, y1, 150e-9);
    n.r(y1, 0, 1.5e3);
    // the fixed tone stack (a Fender TMB with its pots as resistors) and the clipper
    let sx = n.named("stack_x");
    let sy = n.named("diodes");
    let sz = n.node();
    let c6 = n.node();
    n.r(o1, sx, 100e3);
    n.c(o1, c6, 220e-12);
    n.r(c6, sy, 330e3);
    n.c(sx, sy, 100e-9);
    n.c(sx, sz, 47e-9);
    n.r(sz, 0, 15e3);
    n.r(sy, sz, 1e6);
    let dio = n.clipper(sy, 0, ClipperModel::pair(D_1N914, 2));
    let g5 = n.named("q5_gate");
    n.c(sy, g5, 2.2e-9);
    n.r(g5, vb, 1e6);
    n.cut(g5);
    // stage 2
    let g6 = n.named("stage2_minus");
    let o2 = n.named("stage2_out");
    discrete_opamp(&mut n, va, g5, g6, o2, 100e-12, transistor_level);
    let fb2 = n.node();
    let gang2 = n.rheostat(o2, fb2, 250e3, Taper::Log, 0.5);
    n.r(fb2, g6, 33e3);
    n.c(g6, o2, 100e-12);
    let y2 = n.node();
    n.r(g6, y2, 2.2e3);
    n.c(y2, 0, 1e-6);
    // trim, tone, level (the twin starts here: stage 2's output drives it)
    let mark = n.mark();
    let t = n.named("trim");
    n.r(o2, t, 5.6e3);
    n.c(o2, t, 6.8e-9);
    n.c(t, 0, 5.6e-9);
    let p3 = n.named("tone_hi");
    n.c(t, p3, 18e-9);
    let fm = n.named("fat");
    n.c(t, fm, 83e-9);
    let fat = n.var(fm, p3, FAT_OFF, FAT_ON, FAT_OFF);
    let p1 = n.named("tone_lo");
    let tw = n.named("tone_wiper");
    let tone = n.pot(p1, tw, p3, 10e3, Taper::Linear, 0.5);
    n.c(p1, 0, 18e-9);
    let vw = n.named("level_wiper");
    let lv = gfx_dsp::pot::a_taper(LEVEL_POT);
    n.r(tw, vw, 100e3 * (1.0 - lv));
    n.r(vw, 0, 100e3 * lv);
    let p = n.named("ic_plus");
    n.c(vw, p, 47e-9);
    n.r(p, vb, 470e3);
    n.cut(p);
    // stage 3: IC1B with its gyrator (linear), Q9, the output
    let m = n.named("ic_minus");
    let o3 = n.named("ic_out");
    n.opamp_ideal(p, m, o3);
    n.c(o3, m, 2.2e-9);
    n.r(o3, m, 6.8e3);
    let gg = n.named("gyrator");
    let b8 = n.named("q8_base");
    let e8 = n.named("q8_emitter");
    n.c(m, gg, 56e-9);
    n.c(gg, b8, 56e-9);
    n.r(b8, vb, 470e3);
    follower(&mut n, b8, e8, 0.31e-3, 400.0);
    n.r(e8, 0, 10e3);
    n.r(gg, e8, 1.2e3);
    let b9 = n.node();
    n.c(o3, b9, 1e-6);
    n.r(b9, vb, 100e3);
    let e9 = n.node();
    n.buffer(b9, e9);
    let q = n.node();
    n.c(e9, q, 1e-6);
    n.r(q, 0, 100e3);
    let o = n.named("out");
    n.r(q, o, 1e3);
    n.r(o, 0, 1e6);
    let out_k = n.output(o);
    Circuit {
        net: n,
        out: out_k,
        gain: vec![(gang1, false), (gang2, false)],
        tone: vec![(tone, false)],
        clippers: vec![(dio, ClipperModel::pair(D_1N914, 2), 2)],
        opamps: vec![],
        hp: None,
        supply: 8.1,
        stages: 3,
        ideal_stages: 3,
        fast_opamps: vec![5],
        fast_diodes: vec![],
        twin: (mark, o2),
        polarity: 1.0,
        fat: Some((fat, FAT_ON)),
        contour: vec![],
    }
}

/// The Discrete OD's level pot, fixed (rotation): noon, where the IC stage stays linear for any
/// stage-2 output (README §2.1, review 2026-11-26).
pub const LEVEL_POT: f64 = 0.5;

/// **Studio OD** — Aion FX *Andromeda* documentation v1.1.0, the stock ODR-1 (its Bass pot fully up =
/// 0 Ω, D6 / D7 jumpered). u₁ = VA (the jack's supply − 0.35 V: 1N5817 and 47 Ω), u₂ = VB = VA / 2
/// (15 kΩ / 15 kΩ with 47 µF: an AC ground). Stage 1: 33 kΩ into Q1 (ideal follower, 1 MΩ gate
/// resistor kept), 68 nF, 2.7 kΩ, 22 nF to ground, 10 kΩ to VB at IC1A's + input; IC1A (4558) with
/// 1.8 kΩ + the 250 kΩ log gain pot, 120 pF and a 1N914 pair in its feedback, legs 1.5 kΩ + 2.2 µF and
/// 820 Ω + 82 nF to ground; cut at its output (rule a). Stage 2: 2.2 µF, 12 kΩ, 2.7 nF, the hard 1N914
/// pair to VB; the passive network (39 kΩ; 82 nF into 1 nF ∥ 12 kΩ; 10 kΩ; 43 kΩ to VB); IC2A
/// (linear: ideal) with 20 kΩ ∥ 560 pF over 10 kΩ to VB, the Spectrum pot (25 kΩ linear) between 22 nF
/// ∥ 5.1 kΩ from its + input and 1.2 kΩ to its − input, its wiper into 27 nF and 100 nF into the
/// gyrator (8.2 nF, 150 kΩ, Q2 linearized at 1.0 mA, 3.3 kΩ, 2.2 kΩ); cut at IC2A's output (rule a).
/// Stage 3: 4.7 kΩ, 8.2 nF into IC2B (4558: it rail-clips on low notes), 22 kΩ ∥ 4.7 nF ∥ (82 nF +
/// 5.1 kΩ) over 1.2 kΩ + 1 µF; 2.2 µF into the 50 kΩ level pot fully up, 470 nF into the follower
/// (ideal, 150 kΩ to VB), 1 µF into 150 kΩ.
pub fn studio_od() -> Circuit {
    let mut n = Netlist::new(3);
    let sig = n.input(IN_SIG);
    let _va = n.input(IN_VS);
    let vb = n.input(IN_VB);
    let gq = n.named("q1_gate");
    n.r(sig, gq, 33e3);
    n.r(gq, 0, 1e6);
    let sq = n.named("q1_source");
    n.buffer(gq, sq);
    let a1 = n.node();
    let p1 = n.named("ic1a_plus");
    n.c(sq, a1, 68e-9);
    n.r(a1, p1, 2.7e3);
    n.c(p1, 0, 22e-9);
    n.r(p1, vb, 10e3);
    let m1 = n.named("ic1a_minus");
    let o1 = n.named("ic1a_out");
    let op1 = n.opamp(p1, m1, o1, RC4558, (0.0, 1.0), 0.0);
    let b5 = n.node();
    n.r(m1, b5, 1.5e3);
    n.c(b5, 0, 2.2e-6);
    let c4 = n.node();
    n.r(m1, c4, 820.0);
    n.c(c4, 0, 82e-9);
    n.c(m1, o1, 120e-12);
    let fbd = n.clipper(m1, o1, ClipperModel::pair(D_1N914, 1));
    let g6 = n.node();
    n.r(m1, g6, 1.8e3);
    let gain = n.rheostat(g6, o1, 250e3, Taper::Log, 0.5);
    n.cut(o1);
    // the hard clipper to the bias
    let c6 = n.node();
    let d = n.named("diodes");
    n.c(o1, c6, 2.2e-6);
    n.r(c6, d, 12e3);
    n.c(d, 0, 2.7e-9);
    let hard = n.clipper(d, vb, ClipperModel::pair(D_1N914, 1));
    // the network and the Spectrum stage (the twin starts at the clip node)
    let mark = n.mark();
    let e = n.node();
    let f = n.node();
    let mm = n.named("ic2a_plus");
    n.r(d, e, 39e3);
    n.c(e, f, 82e-9);
    n.c(f, 0, 1e-9);
    n.r(f, 0, 12e3);
    n.r(e, mm, 10e3);
    n.r(mm, vb, 43e3);
    let pp = n.named("spectrum_plus");
    n.c(mm, pp, 22e-9);
    n.r(mm, pp, 5.1e3);
    let y = n.named("spectrum_wiper");
    let p3 = n.named("spectrum_minus");
    let spectrum = n.pot(pp, y, p3, 25e3, Taper::Linear, 0.5);
    let m2 = n.named("ic2a_minus");
    n.r(p3, m2, 1.2e3);
    n.c(y, 0, 27e-9);
    let x = n.named("gyrator");
    n.c(y, x, 100e-9);
    let b2 = n.named("q2_base");
    let e2 = n.named("q2_emitter");
    n.c(x, b2, 8.2e-9);
    n.r(b2, vb, 150e3);
    follower(&mut n, b2, e2, 1.0e-3, 400.0);
    n.r(e2, 0, 3.3e3);
    n.r(x, e2, 2.2e3);
    let o2 = n.named("ic2a_out");
    n.opamp_ideal(mm, m2, o2);
    n.r(m2, vb, 10e3);
    n.r(o2, m2, 20e3);
    n.c(o2, m2, 560e-12);
    n.cut(o2);
    // IC2B, the bass shelf (it clips), the level, the follower
    let n5 = n.named("ic2b_plus");
    n.r(o2, n5, 4.7e3);
    n.c(n5, 0, 8.2e-9);
    let m3 = n.named("ic2b_minus");
    let o3 = n.named("ic2b_out");
    let op2 = n.opamp(n5, m3, o3, RC4558, (0.0, 1.0), 0.0);
    let l3 = n.node();
    n.r(m3, l3, 1.2e3);
    n.c(l3, 0, 1e-6);
    n.r(o3, m3, 22e3);
    n.c(o3, m3, 4.7e-9);
    let k3 = n.node();
    n.c(o3, k3, 82e-9);
    n.r(k3, m3, 5.1e3);
    let lv = n.named("level");
    n.c(o3, lv, 2.2e-6);
    n.r(lv, vb, 50e3);
    let p4 = n.node();
    n.c(lv, p4, 470e-9);
    n.r(p4, vb, 150e3);
    let o4 = n.node();
    n.buffer(p4, o4);
    let o = n.named("out");
    n.c(o4, o, 1e-6);
    n.r(o, 0, 150e3);
    let out_k = n.output(o);
    Circuit {
        net: n,
        out: out_k,
        gain: vec![(gain, false)],
        tone: vec![(spectrum, false)],
        clippers: vec![(fbd, ClipperModel::pair(D_1N914, 1), 1), (hard, ClipperModel::pair(D_1N914, 1), 1)],
        opamps: vec![(op1, RC4558), (op2, RC4558)],
        hp: None,
        supply: 9.0 - STUDIO_DROP,
        stages: 2,
        ideal_stages: 2,
        fast_opamps: vec![5],
        fast_diodes: vec![],
        twin: (mark, d),
        polarity: 1.0,
        fat: None,
        contour: vec![],
    }
}

/// The Studio OD's supply drop (1N5817 and 47 Ω at ≈ 5 mA): VA = the jack's supply − 0.35 V.
pub const STUDIO_DROP: f64 = 0.35;
/// The Plexi Dist's (the 1N5817): VA = the jack's supply − 0.30 V (README §2.11's DC point, 8.7 V).
pub const PLEXI_DROP: f64 = 0.30;
/// The Plexi Dist's `supply` cap (decided 2026-11-26: −65.9 dB of alias at 150 %, −60.5 at 200 %).
pub const PLEXI_SUPPLY_MAX: f64 = 1.5;

/// The Plexi Dist's clipper: germanium + 1N4001 one way, 1N4001 + 1N4148 the other, to ground.
pub const PLEXI_CLIP: ClipperModel = ClipperModel {
    pos: Some(ClipSide { j: D_GE_SIGNAL, count: 1, extra: Some(D_1N4001), channel: None }),
    neg: Some(ClipSide { j: D_1N4001, count: 1, extra: Some(D_1N914), channel: None }),
};

/// **Plexi Dist** — PedalPCB *Ultisol Distortion* documentation (rev. 8/19/20), as drawn: LM833,
/// master fully up. u₁ = VA (the jack's supply − 0.30 V), u₂ = VREF = VA / 2 (10 kΩ / 10 kΩ + 47 µF:
/// an AC ground). The input's 100 pF and 2.2 MΩ hang on the ideal source: left out. Stage 1 (IC1.1
/// and IC2.1: the 1 nF across the gain pot couples their loops, no cut between them): 10 nF into
/// IC1.1's + input (1 MΩ to VREF); IC1.1 with 3.3 kΩ + 47 nF to ground, 100 pF across, 4.7 kΩ to the
/// gain pot (100 kΩ reverse log, wiper on IC1.1's output, the far end into 68 nF + 4.7 kΩ to IC2.1's
/// − input), 1 nF across the pot; IC2.1 inverting, 680 kΩ ∥ 68 pF; cut at its output (rule a).
/// Stage 2: 100 nF, 4.7 kΩ, the asymmetric clipper to ground; 3 kΩ, the tone (47 nF + 100 kΩ log
/// rheostat to ground), 220 nF, 47 kΩ into IC2.2 (inverting, 220 kΩ ∥ 470 pF: it rail-clips at 9 V);
/// cut at its output. Stage 3, linear: the bridged-T contour (10 kΩ + 10 kΩ, 6.8 nF across, 4.7 nF
/// and 33 nF + the 50 kΩ log contour rheostat to ground at the middle, 4.7 nF at the end), 470 nF,
/// IC1.2 (unity inverting, 100 kΩ ∥ 220 pF: ideal), 1 kΩ, 4.7 µF into the 10 kΩ master fully up and a
/// 1 MΩ load.
pub fn plexi_dist() -> Circuit {
    let mut n = Netlist::new(3);
    let sig = n.input(IN_SIG);
    let _va = n.input(IN_VS);
    let vb = n.input(IN_VB);
    let p1 = n.named("ic11_plus");
    n.c(sig, p1, 10e-9);
    n.r(p1, vb, 1e6);
    let m1 = n.named("ic11_minus");
    let o1 = n.named("ic11_out");
    let op1 = n.opamp(p1, m1, o1, LM833, (0.0, 1.0), 0.0);
    let l1 = n.node();
    n.r(m1, l1, 3.3e3);
    n.c(l1, 0, 47e-9);
    n.c(m1, o1, 100e-12);
    let nn = n.named("gain_start");
    n.r(m1, nn, 4.7e3);
    let gg = n.named("gain_end");
    let gain = n.pot(nn, o1, gg, 100e3, Taper::RevLog, 0.5);
    n.c(nn, gg, 1e-9);
    let g5 = n.node();
    n.c(gg, g5, 68e-9);
    let m2 = n.named("ic21_minus");
    n.r(g5, m2, 4.7e3);
    let o2 = n.named("ic21_out");
    let op2 = n.opamp(vb, m2, o2, LM833, (0.0, 1.0), 0.0);
    n.r(o2, m2, 680e3);
    n.c(o2, m2, 68e-12);
    n.cut(o2);
    // the clipper, the tone, IC2.2
    let c8 = n.node();
    let d = n.named("diodes");
    n.c(o2, c8, 100e-9);
    n.r(c8, d, 4.7e3);
    let dio = n.clipper(d, 0, PLEXI_CLIP);
    let mark = n.mark();
    let t = n.named("tone");
    n.r(d, t, 3e3);
    let tt = n.node();
    n.c(t, tt, 47e-9);
    let tone = n.rheostat(tt, 0, 100e3, Taper::Log, 0.5);
    let c10 = n.node();
    n.c(t, c10, 220e-9);
    let m3 = n.named("ic22_minus");
    n.r(c10, m3, 47e3);
    let o3 = n.named("ic22_out");
    let op3 = n.opamp(vb, m3, o3, LM833, (0.0, 1.0), 0.0);
    n.r(o3, m3, 220e3);
    n.c(o3, m3, 470e-12);
    n.cut(o3);
    // the contour (bridged T), IC1.2, the master
    let k = n.named("contour_mid");
    let kc = n.node();
    let l = n.named("contour_out");
    n.r(o3, k, 10e3);
    n.c(k, 0, 4.7e-9);
    n.c(k, kc, 33e-9);
    let contour = n.rheostat(kc, 0, 50e3, Taper::Log, 0.7);
    n.r(k, l, 10e3);
    n.c(o3, l, 6.8e-9);
    n.c(l, 0, 4.7e-9);
    let c16 = n.node();
    n.c(l, c16, 470e-9);
    let m4 = n.named("ic12_minus");
    n.r(c16, m4, 100e3);
    let o4 = n.named("ic12_out");
    n.opamp_ideal(vb, m4, o4);
    n.r(o4, m4, 100e3);
    n.c(o4, m4, 220e-12);
    let r15 = n.node();
    n.r(o4, r15, 1e3);
    let o = n.named("out");
    n.c(r15, o, 4.7e-6);
    n.r(o, 0, 10e3); // the master, fully up
    n.r(o, 0, 1e6); // the next device
    let out_k = n.output(o);
    Circuit {
        net: n,
        out: out_k,
        gain: vec![(gain, false)],
        tone: vec![(tone, false)],
        clippers: vec![(dio, PLEXI_CLIP, 2)],
        opamps: vec![(op1, LM833), (op2, LM833), (op3, LM833)],
        hp: None,
        supply: 9.0 - PLEXI_DROP,
        stages: 3,
        // `diodes` Germanium aliases −61 dB at 8× (−65 dB bound): it runs at 16×
        ideal_stages: 4,
        fast_opamps: vec![],
        fast_diodes: vec![2],
        twin: (mark, d),
        polarity: -1.0,
        fat: None,
        contour: vec![(contour, false)],
    }
}

/// The supply the netlist sees (its u₁, VA) at the jack's `v_jack` (V: 9 V × `supply`): the
/// regulator or the Schottky drop of each circuit, the Plexi Dist's cap at 150 %.
pub fn rail(k: usize, v_jack: f64) -> f64 {
    match k {
        4 => 0.9 * v_jack,
        5 => v_jack - STUDIO_DROP,
        6 => v_jack.min(9.0 * PLEXI_SUPPLY_MAX) - PLEXI_DROP,
        _ => v_jack,
    }
}
