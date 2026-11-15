// SPDX-License-Identifier: GPL-3.0-or-later
//! The five netlists (README §2.1), each from one published clone schematic named in its comment.
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
    /// The circuit's own supply (V).
    pub supply: f64,
    /// Half-band stages at 44.1 / 48 kHz (one fewer per doubling of the host rate).
    pub stages: usize,
    /// The same with `opamp` Ideal (a faster op-amp aliases more: README §2.5).
    pub ideal_stages: usize,
    /// The element mark and the node where the post-clipper network starts (`bass_keep`'s twin).
    pub twin: (usize, Node),
    /// The small-signal polarity of the pre-clipper path (the twin's sign).
    pub polarity: f64,
}

/// The booster's NPN (2SC2240-class low-noise small-signal part) [U: generic constants, hFE in the
/// datasheet's GR range].
pub const Q_2SC2240: BjtModel = BjtModel { npn: true, is: 2e-14, bf: 400.0, br: 4.0, leak: 1.0, eg: 1.11, xti: 3.0, tnom: 25.0 };

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
        twin: (mark, out),
        polarity: 1.0,
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
        twin: (mark, out),
        polarity: -1.0,
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
        twin: (mark, out),
        polarity: 1.0,
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
        twin: (mark, p2),
        polarity: 1.0,
    }
}
