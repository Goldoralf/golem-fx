// SPDX-License-Identifier: GPL-3.0-or-later
//! The netlists (README §2.3), the guitar in front of each (README §2.2, gfx-dsp `pickup`). Every
//! circuit has the same inputs: u₀ = the coil's EMF (rebuilt from the DI by the pickup's EMF filter),
//! u₁ = the battery's open-circuit voltage (negative for the PNP germanium face: positive ground).
//! The battery reaches the circuit through `sag` (0 – 2 kΩ) and a 47 µF reservoir.

use gfx_dsp::circuit_nl::*;
use gfx_dsp::pickup::Guitar;
use gfx_dsp::pot::Taper;

pub const IN_SIG: usize = 0;
pub const IN_VS: usize = 1;

/// The battery's largest internal resistance (`sag` 100 %).
pub const SAG_MAX: f64 = 2e3;
/// The supply's reservoir capacitor.
pub const RESERVOIR: f64 = 47e-6;
/// The buffer (`buffer` on): 1 MΩ in, 100 Ω out.
pub const BUFFER_IN: f64 = gfx_dsp::pickup::INTERFACE_LOAD;
pub const BUFFER_OUT: f64 = 100.0;

/// What the effect needs to drive one compiled circuit.
#[derive(Clone, Debug)]
pub struct Circuit {
    pub net: Netlist,
    /// Output index: the volume pot fully up.
    pub out: usize,
    /// Output indices of the meters: the trimmed transistor's collector, the circuit's supply.
    pub bias_meter: usize,
    pub supply_meter: usize,
    /// The `fuzz` pot (rotation = `fuzz`).
    pub fuzz: usize,
    /// The guitar's volume pot (rotation = `guitar_vol`).
    pub volume: usize,
    /// Si Four: the tone pot (rotation 0 = the bass end) and the `mids` resistor.
    pub tone: Option<usize>,
    pub mids: Option<usize>,
    /// The trimmer (variable resistor) and its stock value (Ω).
    pub bias: (usize, f64),
    /// The battery's internal resistance.
    pub sag: usize,
    /// The germanium transistors `leakage` scales: (device, its model at 100 %).
    pub ge: Vec<(usize, BjtModel)>,
    /// The supply's sign (−1: PNP, positive ground).
    pub supply_sign: f64,
}

// ---------------------------------------------------------------------------------------------
// Devices

/// DAFx-17 (Holmes, Holters & van Walstijn, table 1), AC128, Ebers–Moll, at its 22.8 °C
/// (V_T = 25.5 mV): Is 23.75 µA, βF 44.90, βR 4.568, NF 1.168, NR 1.171. Kept for the tests: two of
/// them make a dead pedal (README §2.3).
pub const AC128_RAW: BjtModel = BjtModel {
    npn: false,
    is: 23.75e-6,
    bf: 44.90,
    br: 4.568,
    leak: 1.0,
    eg: 0.67,
    xti: 3.0,
    tnom: 22.8,
    nf: 1.168,
    nr: 1.171,
};

/// A selected germanium part: the measured AC128's N, βR and law, with the βF and open-base
/// collector current I_CEO (at 22.8 °C) a builder selects for (README §2.3).
pub const fn graded(bf: f64, iceo: f64) -> BjtModel {
    let is = iceo / (1.0 + (bf + 1.0) / AC128_RAW.br);
    BjtModel {
        bf,
        is,
        ..AC128_RAW
    }
}

/// The graded pair (README §2.3, a voicing start [U]): Q1 βF 75, I_CEO 80 µA; Q2 βF 120, 200 µA.
pub const GE_Q1: BjtModel = graded(75.0, 80e-6);
pub const GE_Q2: BjtModel = graded(120.0, 200e-6);

/// BC108-class silicon (βF ≈ 300, Is ≈ 2·10⁻¹⁴ A [U]).
pub const SI_BC108: BjtModel = BjtModel {
    npn: true,
    is: 2e-14,
    bf: 300.0,
    br: 5.0,
    leak: 1.0,
    eg: 1.12,
    xti: 3.0,
    tnom: 25.0,
    nf: 1.0,
    nr: 1.0,
};

/// Junction capacitances (C_cb, C_eb): DAFx-17's AC128 C_cb and OC44 C_eb for germanium; 5 / 10 pF
/// for silicon [U].
pub const GE_CAPS: (f64, f64) = (100e-12, 410e-12);
pub const SI_CAPS: (f64, f64) = (5e-12, 10e-12);

/// The face's trimmer (Q2's collector resistor) for 4.5 V at 25 °C, 9 V: solved by
/// `tests::trimmers_give_4v5` (bisection on the DC point) and hard-coded.
pub const R3_GE: f64 = 13.02e3;
pub const R3_SI: f64 = 6.23e3;
/// The muff's first clipper's base resistor (its stock value: the muff is not re-trimmed).
pub const R_MUFF_BIAS: f64 = 100e3;

// ---------------------------------------------------------------------------------------------
// The guitar and the supply

/// Guitar → (buffer) → the pedal's input node. Returns (input node, volume pot).
fn front(n: &mut Netlist, g: &Guitar, buffer: bool) -> (Node, usize) {
    let emf = n.input(IN_SIG);
    let s = g.stamp(n, emf, 1.0);
    if buffer {
        n.r(s.jack, 0, BUFFER_IN);
        let b = n.named("buffer");
        n.buffer(s.jack, b);
        // an ideal source's output: an exact cut (the guitar is its own stage, linear)
        n.cut(b);
        let i = n.named("in");
        n.r(b, i, BUFFER_OUT);
        (i, s.volume)
    } else {
        (s.jack, s.volume)
    }
}

/// Battery → `sag` → the reservoir: the circuit's supply node, and the sag resistor.
fn supply(n: &mut Netlist) -> (Node, usize) {
    let vs = n.input(IN_VS);
    let vcc = n.named("vcc");
    let sag = n.var(vs, vcc, R_MIN, R_MIN, SAG_MAX);
    n.c(vcc, 0, RESERVOIR);
    (vcc, sag)
}

fn caps(n: &mut Netlist, c: Node, b: Node, e: Node, (cbc, cbe): (f64, f64)) {
    n.c(c, b, cbc);
    n.c(b, e, cbe);
}

/// **The face** (Ge Pair, Si Pair) — the two-transistor shunt-series feedback fuzz [C, matches
/// BYOD's netlist]: 2.2 µF in; Q1 collector 33 kΩ; Q2 collector: the 470 Ω tap from the supply, then
/// the trimmer (stock 8.2 kΩ, trimmed per device set); 100 kΩ Q2 emitter → Q1 base; the 1 kΩ linear
/// fuzz pot in Q2's emitter with 20 µF on its wiper; 10 nF out into the 500 kΩ volume pot fully up.
pub fn face(q: [BjtModel; 2], jc: (f64, f64), r3: f64, g: &Guitar, buffer: bool) -> Circuit {
    let mut n = Netlist::new(2);
    let (inp, volume) = front(&mut n, g, buffer);
    let (vcc, sag) = supply(&mut n);
    let b1 = n.named("q1_base");
    let c1 = n.named("q1_collector");
    let c2 = n.named("q2_collector");
    let e2 = n.named("q2_emitter");
    let o = n.named("tap");
    let w = n.named("fuzz_wiper");
    let out = n.named("out");
    n.c(inp, b1, 2.2e-6);
    n.r(c1, vcc, 33e3);
    n.r(vcc, o, 470.0);
    let bias = n.var(o, c2, r3, 0.5 * r3, 2.0 * r3);
    n.r(e2, b1, 100e3);
    let fuzz = n.pot(0, w, e2, 1e3, Taper::Linear, 0.7);
    n.c(w, 0, 20e-6);
    n.c(o, out, 10e-9);
    n.r(out, 0, 500e3);
    let d1 = n.bjt(c1, b1, 0, q[0]);
    let d2 = n.bjt(c2, c1, e2, q[1]);
    caps(&mut n, c1, b1, 0, jc);
    caps(&mut n, c2, c1, e2, jc);
    let ge = if q[0].eg < 1.0 {
        vec![(d1, q[0]), (d2, q[1])]
    } else {
        Vec::new()
    };
    Circuit {
        out: n.output(out),
        bias_meter: n.probe(c2),
        supply_meter: n.probe(vcc),
        net: n,
        fuzz,
        volume,
        tone: None,
        mids: None,
        bias: (bias, r3),
        sag,
        ge,
        supply_sign: if q[0].npn { 1.0 } else { -1.0 },
    }
}

pub fn ge_pair(g: &Guitar, buffer: bool) -> Circuit {
    face([GE_Q1, GE_Q2], GE_CAPS, R3_GE, g, buffer)
}

pub fn si_pair(g: &Guitar, buffer: bool) -> Circuit {
    face([SI_BC108, SI_BC108], SI_CAPS, R3_SI, g, buffer)
}

/// The muff's tone stack variants (`stack`): (low-pass R, low-pass C, high-pass C, high-pass R), or
/// None for Bypass. Values from Kit Rae's version notes (bigmuffpage.com, parts 1 – 3) where the text
/// gives them, C8 / C9 its "standard" 10 nF / 4 nF:
/// - Classic (1970s NYC): 39 kΩ low-pass, 22 kΩ high-pass (the low-pass value often quoted for the
///   1970s units [U]; the high-pass 22 kΩ the common value);
/// - Early (1974 Ram's Head, "violet"): "input and tone resistors enlarged to 39k": 39 / 39 kΩ;
/// - Triangle (the "standard" 1971 V1): "33k resistors at R5 and R8": 33 / 33 kΩ;
/// - Russian (Green Civil War): "low pass resistor at R8: almost all were 20k", high-pass 22 kΩ.
pub const STACKS: [Option<(f64, f64, f64, f64)>; 5] = [
    Some((39e3, 10e-9, 4e-9, 22e3)),
    Some((39e3, 10e-9, 4e-9, 39e3)),
    Some((33e3, 10e-9, 4e-9, 33e3)),
    Some((20e3, 10e-9, 4e-9, 22e3)),
    None,
];

/// The muff's silicon (the review prototype's: Is 2·10⁻¹⁴ A, βF 300, βR 5 [U]).
pub const SI_MUFF: BjtModel = SI_BC108;

/// **The muff** (Si Four) — input booster, two clipping stages (1N914 pair and 470 pF collector →
/// base), the passive tone stack, the output booster; values from the review's prototype (typical
/// NYC values [C / U]): booster 39 kΩ in, 100 kΩ / 470 kΩ ∥ 470 pF bias, 10 kΩ collector, 100 Ω
/// emitter; the 100 kΩ sustain pot (linear [U]) between booster and first clipper; clippers 10 kΩ in,
/// 100 kΩ / 470 kΩ ∥ 470 pF, 10 kΩ collector, 150 Ω emitter, the diodes through 100 nF; the stack
/// with the 100 kΩ linear tone pot and `mids` (0 – 22 kΩ in series with the low-pass capacitor);
/// the output booster 390 kΩ / 100 kΩ, 10 kΩ, 2.7 kΩ; 100 nF into the 100 kΩ volume pot fully up.
pub fn si_four(stack: usize, g: &Guitar, buffer: bool) -> Circuit {
    let mut n = Netlist::new(2);
    let (inp, volume) = front(&mut n, g, buffer);
    let (vcc, sag) = supply(&mut n);
    let q = SI_MUFF;
    // input booster
    let x1 = n.node();
    let b1 = n.named("q1_base");
    let c1 = n.named("q1_collector");
    let e1 = n.node();
    n.c(inp, x1, 100e-9);
    n.r(x1, b1, 39e3);
    n.r(b1, 0, 100e3);
    n.r(c1, b1, 470e3);
    n.c(c1, b1, 470e-12);
    n.r(vcc, c1, 10e3);
    n.r(e1, 0, 100.0);
    n.bjt(c1, b1, e1, q);
    // sustain
    let st = n.node();
    let sw = n.named("sustain");
    n.c(c1, st, 100e-9);
    let fuzz = n.pot(0, sw, st, 100e3, Taper::Linear, 0.7);
    // two clippers
    let mut input = sw;
    let mut bias = (0, R_MUFF_BIAS);
    let mut c2 = 0;
    for k in 0..2 {
        let x = n.node();
        let b = n.named(if k == 0 { "q2_base" } else { "q3_base" });
        let c = n.named(if k == 0 {
            "q2_collector"
        } else {
            "q3_collector"
        });
        let e = n.node();
        let d = n.node();
        n.c(input, x, 100e-9);
        n.r(x, b, 10e3);
        if k == 0 {
            bias.0 = n.var(b, 0, R_MUFF_BIAS, 0.5 * R_MUFF_BIAS, 2.0 * R_MUFF_BIAS);
            c2 = c;
        } else {
            n.r(b, 0, 100e3);
        }
        n.r(c, b, 470e3);
        n.c(c, b, 470e-12);
        n.c(c, d, 100e-9);
        n.clipper(d, b, ClipperModel::pair(D_1N914, 1));
        n.r(vcc, c, 10e3);
        n.r(e, 0, 150.0);
        n.bjt(c, b, e, q);
        input = c;
    }
    // tone stack
    let (tone, mids, w) = match STACKS[stack.min(4)] {
        Some((rl, cl, ch, rh)) => {
            let a = n.named("stack_lp");
            let m = n.node();
            let bb = n.named("stack_hp");
            let w = n.named("stack_wiper");
            n.r(input, a, rl);
            let mids = n.var(a, m, R_MIN, R_MIN, 22e3);
            n.c(m, 0, cl);
            n.c(input, bb, ch);
            n.r(bb, 0, rh);
            let tone = n.pot(a, w, bb, 100e3, Taper::Linear, 0.5);
            (Some(tone), Some(mids), w)
        }
        None => (None, None, input),
    };
    // output booster
    let b4 = n.named("q4_base");
    let c4 = n.named("q4_collector");
    let e4 = n.node();
    let out = n.named("out");
    n.c(w, b4, 100e-9);
    n.r(vcc, b4, 390e3);
    n.r(b4, 0, 100e3);
    n.r(vcc, c4, 10e3);
    n.r(e4, 0, 2.7e3);
    n.bjt(c4, b4, e4, q);
    n.c(c4, out, 100e-9);
    n.r(out, 0, 100e3);
    Circuit {
        out: n.output(out),
        bias_meter: n.probe(c2),
        supply_meter: n.probe(vcc),
        net: n,
        fuzz,
        volume,
        tone,
        mids,
        bias,
        sag,
        ge: Vec::new(),
        supply_sign: 1.0,
    }
}
