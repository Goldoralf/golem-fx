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
    /// The trimmer's law: R = stock × 2^(`bias_sign` · `bias`), so that + raises `bias_v` (−1 on
    /// the faces and the muff, +1 on Octave Up's R2).
    pub bias_sign: f64,
    /// Octave Up: the fuzz tap s1 = a · v(winding), `octave` 0 % (an output index); None elsewhere.
    pub tap: Option<usize>,
    /// Named nodes the tests read (Octave Up's DC point, the winding); empty elsewhere.
    pub nodes: Vec<(&'static str, Node)>,
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
        bias_sign: -1.0,
        tap: None,
        nodes: Vec::new(),
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
        bias_sign: -1.0,
        tap: None,
        nodes: Vec::new(),
    }
}

// ---------------------------------------------------------------------------------------------
// Octave Up (gfx-octafuzz's circuit, README §2.3 "Octave Up")

/// 2N4401 (Q2, Q3) [U: a common SPICE model; βF from Retrograde24's measured hFE 169 / 173].
pub const Q_2N4401: BjtModel = BjtModel {
    npn: true,
    is: 26.03e-15,
    bf: 170.0,
    br: 1.01,
    leak: 1.0,
    eg: 1.11,
    xti: 3.0,
    tnom: 25.0,
    nf: 1.0,
    nr: 1.0,
};
/// 2N5087 (Q1, PNP) [U: a common SPICE model].
pub const Q_2N5087: BjtModel = BjtModel {
    npn: false,
    is: 6.73e-15,
    bf: 254.0,
    br: 2.68,
    ..Q_2N4401
};
/// Junction capacitances (C_cb, C_eb) [U, the same models].
pub const CAPS_2N4401: (f64, f64) = (7.3e-12, 26e-12);
pub const CAPS_2N5087: (f64, f64) = (9.5e-12, 9e-12);
/// The rectifier's 1N34A: Is 0.1 µA, N 1.3 (0.30 V at 1 mA, fitted to typical figures [U]; not
/// gfx-dsp's `D_1N34A`, whose 0.2 µA is gfx-dist's choice).
pub const D_1N34A_OCT: Junction = Junction::germanium(0.1e-6, 1.3);

/// **L_m, the 42TM022's magnetising inductance on its 600 Ω winding [U]: the one number to change
/// when Raph's measurement arrives** (README O1). 1 H is the design's working value, the one the
/// Octave bench row's marks are set for; the review's estimate of the real part is 0.1 – 0.5 H
/// (gfx-octafuzz README, As coded).
pub const L_M: f64 = 1.0;
/// The 600 Ω winding's DCR (45 Ω by one distributor table [U]).
pub const DCR_600: f64 = 45.0;
/// Each half of the 1.5 kΩ winding against the 600 Ω winding: √(1500 / 600) / 2.
pub const OCT_A: f64 = 0.790_569_415_042_094_9;
/// R2, the trimmer (Q1's base resistor to ground), stock: not re-trimmed (the document's value).
pub const R2_OCT: f64 = 680e3;

/// **Octave Up**: the transformer octave (Tycobrahe-type, negative-ground conversion). Values of
/// madbean's Retrograde24 (2024), topology of PCB Guitar Mania's Octavius (2021), checked on Aion FX
/// Octagon (2024). Stage 1: the guitar, Q1 – Q3 and the 600 Ω winding as one inductor L_m behind its
/// DCR (the secondaries are open: leakage drops out), the secondary's half-winding voltage a · v(L_m)
/// through ideal buffers → s1, an exact cut. Stage 2: s1 and its mirror s3 = −s1 (an ideal inverting
/// op-amp), the two 1N34A into the 500 kΩ volume pot fully up. No output capacitor.
pub fn octave_up(g: &Guitar, buffer: bool) -> Circuit {
    octave_up_with(g, buffer, Q2_FORWARD, true)
}

/// The rectifier's load (the volume pot fully up).
pub const RECT_LOAD: f64 = 500e3;

/// Q2 (the emitter follower) as a forward-only BJT: its collector junction never conducts (README
/// §2.8, way 2: −4.9 to −5.0 V on every run; by topology, Q2's base cannot pass the supply). One
/// port fewer in stage 1 (5 instead of 6).
pub const Q2_FORWARD: bool = true;

/// [`octave_up`] with Q2 forward-only or a full Ebers–Moll BJT (the tests run both); without
/// `stage2` the netlist stops at the tap (`out` = s1: the tests' stage-1 diagnostics).
pub fn octave_up_with(g: &Guitar, buffer: bool, q2_forward: bool, stage2: bool) -> Circuit {
    let mut n = Netlist::new(2);
    let (inp, volume) = front(&mut n, g, buffer);
    let (vcc, sag) = supply(&mut n);
    let b1 = n.named("q1_base");
    let e1 = n.named("q1_emitter");
    let c1 = n.named("q1_collector");
    let e2 = n.named("q2_emitter");
    let c3 = n.named("q3_collector");
    let e3 = n.named("q3_emitter");
    let x = n.named("bias_filter");
    let y = n.named("loop");
    let f = n.named("fuzz_cap");
    let z = n.node();
    let w = n.named("winding");
    let wl = n.named("winding_lm");
    n.r(inp, 0, 1e6); // R1
    n.c(inp, b1, 100e-9); // C1
    n.c(b1, 0, 150e-12); // C2
    let bias = n.var(b1, 0, R2_OCT, 0.5 * R2_OCT, 2.0 * R2_OCT); // R2, the trimmer
    n.r(b1, x, 820e3); // R3
    n.r(x, vcc, 180e3); // R4
    n.c(x, 0, 33e-6);
    n.r(e1, y, 1e3); // R7
    n.c(y, f, 100e-6);
    // the 1 kΩ B fuzz rheostat from the capacitor to ground, (1 − fuzz) · 1 kΩ (100 % = R_MIN): the
    // pot's other half runs from a node of its own to ground
    let spare = n.node();
    let fuzz = n.pot(spare, 0, f, 1e3, Taper::Linear, 0.7);
    n.r(y, c3, 47e3); // R8, the loop
    n.r(c1, 0, 220e3); // R5
    n.r(c1, z, 220.0); // R6 + C4: the treble cut
    n.c(z, 0, 1e-9);
    n.r(e2, 0, 22e3); // R9
    n.r(e3, 0, 470.0); // R10 ∥ C5
    n.c(e3, 0, 220e-6);
    n.r(vcc, c3, 1.2e3); // R11
    n.c(c3, w, 33e-6);
    n.r(w, wl, DCR_600);
    n.l(wl, 0, L_M);
    n.bjt(c1, b1, e1, Q_2N5087);
    if q2_forward {
        n.bjt_forward(vcc, c1, e2, Q_2N4401);
    } else {
        n.bjt(vcc, c1, e2, Q_2N4401);
    }
    n.bjt(c3, e2, e3, Q_2N4401);
    caps(&mut n, c1, b1, e1, CAPS_2N5087);
    caps(&mut n, vcc, c1, e2, CAPS_2N4401);
    caps(&mut n, c3, e2, e3, CAPS_2N4401);
    // the secondary: a · v(L_m), through ideal buffers (the winding sees no load)
    let bw = n.node();
    n.buffer(wl, bw);
    let dv = n.node();
    n.r(bw, dv, (1.0 - OCT_A) * 10e3);
    n.r(dv, 0, OCT_A * 10e3);
    let s1 = n.named("s1");
    n.buffer(dv, s1);
    let out = if stage2 {
        n.cut(s1);
        // stage 2: the centre-tapped half-windings ±s1, the full-wave rectifier, the volume pot
        let mi = n.node();
        let s3 = n.named("s3");
        n.r(s1, mi, 10e3);
        n.r(mi, s3, 10e3);
        n.opamp_ideal(0, mi, s3);
        let out = n.named("out");
        n.clipper(s1, out, ClipperModel::diode(D_1N34A_OCT));
        n.clipper(s3, out, ClipperModel::diode(D_1N34A_OCT));
        n.r(out, 0, RECT_LOAD);
        out
    } else {
        s1
    };
    let nodes = vec![
        ("q1_emitter", e1),
        ("q1_base", b1),
        ("q1_collector", c1),
        ("q2_emitter", e2),
        ("q3_collector", c3),
        ("q3_emitter", e3),
        ("winding_lm", wl),
    ];
    Circuit {
        out: n.output(out),
        tap: Some(n.output(s1)),
        bias_meter: n.probe(c3),
        supply_meter: n.probe(vcc),
        net: n,
        fuzz,
        volume,
        tone: None,
        mids: None,
        bias: (bias, R2_OCT),
        sag,
        ge: Vec::new(),
        supply_sign: 1.0,
        bias_sign: 1.0,
        nodes,
    }
}
