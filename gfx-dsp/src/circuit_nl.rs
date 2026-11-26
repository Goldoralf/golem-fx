// SPDX-License-Identifier: GPL-3.0-or-later
//! Nonlinear circuits solved sample by sample: the nodal DK method (Holters & Zölzer, DAFx-11 and
//! EUSIPCO 2015). One module for every nonlinear circuit in golem-fx (gfx-dist, gfx-fuzz; later
//! gfx-wah's transistor stage and amp preamps). gfx-dsp's linear `circuit` stays as it is.
//!
//! - **Netlist** ([`Netlist`]): R, C, L, ideal op-amps and buffers, variable resistors and pots (two-
//!   and three-terminal, with their taper), ideal voltage sources and current sources driven by an
//!   input vector u (several signals, the supply), and devices: clippers (a pair of Shockley strings,
//!   with an optional MOSFET channel), BJTs (Ebers–Moll, NPN / PNP, SPICE temperature law, collector-
//!   junction leakage scale), op-amp macromodels (a tanh or square-law input stage charging a 30 pF
//!   compensation capacitor — slew and gain-bandwidth from one law — with a leak setting A₀, rails
//!   that follow the supply, an optional output resistance), JFETs (Shichman–Hodges, drain / source
//!   symmetric, optional soft pinch-off) and the JFET pair port (a differential pair on its tail as
//!   one transconductor port, the tail following the common mode one sample late: gfx-dist §2.11's
//!   discrete op-amps). Every device constant is a run-time
//!   parameter: no matrix depends on one.
//! - **Discretisation**: trapezoidal companion models at the given (oversampled) rate, or backward
//!   Euler for a capacitor that would ring at half the rate (a stiff mode). States are the companions'
//!   history currents, so a moving pot never jumps them.
//! - **DK reduction**: with states J, inputs u and port currents i, every observed quantity (port
//!   voltages v, reactive voltages, outputs) is one row of P·[J; u; i], P = W M⁻¹ Z. The nonlinear
//!   equation per sample is g(v) = p + K f(v) − v = 0, p = P_vJ J + P_vu u, K = P_vi.
//! - **Solve** (gfx-dist §2.2): seed = the previous solution, extrapolated only when the two previous
//!   samples were solved by plain Newton; Newton with backtracking (Armijo on ‖g‖²) and device
//!   limiting (SPICE `pnjlim` on junctions, a tanh input step ≤ 3 V_lin, the integrator stopped at its
//!   rail's knee when it arrives from inside); stop at |Δv| < 10⁻⁶ V (or a residual below it) or 20
//!   iterations. On failure: plain Newton from the previous solution, then the same without line search
//!   (SPICE's way, the limits as the damping), then continuation in p from the previous sample's (p, v)
//!   in 4, then 16 steps; then the previous solution and its currents are held and a counter
//!   incremented. Fixed caps, data-dependent path only: two renders are bit-identical. Jacobian solves:
//!   explicit for 1 – 2 ports, LU with partial pivoting above.
//! - **Pots**: Woodbury. The matrices are precomputed with every variable resistor at a nominal value
//!   (M₀); a change ΔG enters as P = P₀ − L (I + ΔG Q)⁻¹ ΔG R, a k × k solve plus an O(rows · cols · k)
//!   correction, cheap enough for every base-rate sample while a pot moves. Ends clamped at 10 Ω.
//! - **Stages**: [`Netlist::cut`] splits the circuit into a directed graph of stages. A cut is exact
//!   only (a) at an ideal voltage-source output (an op-amp macro's buffer) or (b) at a node that draws
//!   no current downstream (an op-amp input): the downstream stage sees the node as an ideal source.
//!   The netlist's author declares cuts; [`NonlinearCircuit::monolithic`] ignores them (the reference).
//! - **DC point**: caps open, inductors shorted, gmin on every node; Newton from a seed, continuation
//!   from the seed's inputs, source stepping from zero. [`NonlinearCircuit::init_dc`] stores the seed;
//!   [`NonlinearCircuit::solve_dc`] re-solves inline (bounded) and sets the states: a silent start.
//! - **Analysis**: [`NonlinearCircuit::ac_response`] (the circuit linearized at its operating point,
//!   Laplace domain at s = jω), [`Netlist::to_spice`] (the same netlist and device constants as an
//!   ngspice deck), an offline reference mode (monolithic, any rate, tolerance set by the caller).
//!
//! Real-time: [`NonlinearCircuit::new`] allocates; `process`, `set_pot`, `set_var`, `set_supply`,
//! `set_device`, `set_temperature`, `solve_dc` and `copy_state_from` do not.

use crate::pot::Taper;
use std::f64::consts::PI;

/// Node 0 is ground.
pub type Node = usize;

pub const KELVIN: f64 = 273.15;
/// k / q in V/K.
pub const K_Q: f64 = 1.380649e-23 / 1.602176634e-19;
/// Every op-amp macro's compensation capacitor: the models set I_t and V_lin against it, so a part
/// swap changes device constants only.
pub const CC: f64 = 30e-12;
/// Smallest resistance a pot or variable resistor reaches (its ends).
pub const R_MIN: f64 = 10.0;
/// The DC solve's conductance from every node to ground (SPICE's gmin).
pub const GMIN: f64 = 1e-12;
/// Newton: iteration cap per attempt.
pub const NEWTON_CAP: usize = 20;
/// The largest port count of a stage.
pub const MAX_PORTS: usize = 12;
/// The most variable resistors in one stage (a pot is two).
pub const MAX_VARS: usize = 12;
/// Exponentials continue linearly beyond this argument (SPICE).
const EXP_LIM: f64 = 40.0;

/// The thermal voltage kT / q at `celsius`.
pub fn thermal_voltage(celsius: f64) -> f64 {
    K_Q * (celsius + KELVIN)
}

/// The shared fast exp: e^x by 2^k · e^r, |r| ≤ ln 2 / 2, degree-8 Taylor (relative error < 3·10⁻¹⁰
/// [measured in the tests], well under the 10⁻⁷ the designs ask). x is clamped to ±700.
#[inline]
pub fn fast_exp(x: f64) -> f64 {
    const MAGIC: f64 = 6755399441055744.0; // 1.5 · 2^52: round to nearest
    let x = x.clamp(-700.0, 700.0);
    let y = x * std::f64::consts::LOG2_E;
    let k = (y + MAGIC) - MAGIC;
    let r = (y - k) * std::f64::consts::LN_2;
    // degree-8 Taylor by Estrin's scheme: four short independent chains instead of one of eight
    let r2 = r * r;
    let r4 = r2 * r2;
    let p01 = 1.0 + r;
    let p23 = 0.5 + r * (1.0 / 6.0);
    let p45 = 1.0 / 24.0 + r * (1.0 / 120.0);
    let p67 = 1.0 / 720.0 + r * (1.0 / 5040.0);
    let p = (p01 + r2 * p23) + r4 * ((p45 + r2 * p67) + r4 * (1.0 / 40320.0));
    p * f64::from_bits(((k as i64 + 1023) as u64) << 52)
}

/// e^x and its derivative, continued linearly beyond EXP_LIM.
#[inline]
fn exp_lin(x: f64) -> (f64, f64) {
    if x < EXP_LIM {
        let e = fast_exp(x);
        (e, e)
    } else {
        let e0 = fast_exp(EXP_LIM);
        (e0 * (1.0 + x - EXP_LIM), e0)
    }
}

/// vc · ln(1 + e^(x / vc)) and its slope: a linear conductance with a smooth knee (convex and
/// monotone, so Newton converges on it quadratically, unlike a piecewise-quadratic knee).
#[inline]
fn softplus(x: f64, vc: f64, ivc: f64) -> (f64, f64) {
    let ln1p = |e: f64| if e < 1e-8 { e } else { e.ln_1p() };
    if x > 0.0 {
        let e = fast_exp(-x * ivc);
        (x + vc * ln1p(e), 1.0 / (1.0 + e))
    } else {
        let e = fast_exp(x * ivc);
        (vc * ln1p(e), e / (1.0 + e))
    }
}

/// SPICE's junction voltage limiting: acts only above `vcrit` and compresses the step logarithmically.
#[inline]
pub fn pnjlim(vnew: f64, vold: f64, vt: f64, vcrit: f64) -> f64 {
    if vnew > vcrit && (vnew - vold).abs() > 2.0 * vt {
        if vold > 0.0 {
            let arg = 1.0 + (vnew - vold) / vt;
            if arg > 0.0 {
                vold + vt * arg.ln()
            } else {
                vcrit
            }
        } else {
            vt * (vnew / vt).ln()
        }
    } else {
        vnew
    }
}

fn vcrit(is: f64, nvt: f64) -> f64 {
    nvt * (nvt / (std::f64::consts::SQRT_2 * is)).ln()
}

// ---------------------------------------------------------------------------------------------
// Device models

/// A pn junction (Shockley), with SPICE's temperature law: Is(T) = Is · (T / T₀)^(XTI / N) ·
/// exp(Eg / (N k) · (1 / T₀ − 1 / T)).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Junction {
    pub is: f64,
    pub n: f64,
    /// Band gap (eV): 1.11 silicon, 0.67 germanium.
    pub eg: f64,
    pub xti: f64,
    /// The temperature the constants are given at (°C).
    pub tnom: f64,
}

impl Junction {
    pub const fn silicon(is: f64, n: f64) -> Self {
        Self { is, n, eg: 1.11, xti: 3.0, tnom: 25.0 }
    }
    pub const fn germanium(is: f64, n: f64) -> Self {
        Self { is, n, eg: 0.67, xti: 3.0, tnom: 25.0 }
    }
    /// (Is(T), N·V_T(T)).
    pub fn at(&self, celsius: f64) -> (f64, f64) {
        let t = celsius + KELVIN;
        let t0 = self.tnom + KELVIN;
        let is = self.is * (t / t0).powf(self.xti / self.n) * ((self.eg / (self.n * K_Q)) * (1.0 / t0 - 1.0 / t)).exp();
        (is, self.n * K_Q * t)
    }
}

/// A MOSFET's diode-connected channel: i = k (v − vth)² above vth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Channel {
    pub vth: f64,
    pub k: f64,
}

/// One conduction direction of a clipper: `count` identical junctions in series, an optional
/// different junction in series, an optional channel in parallel. A series string of ideal Shockley
/// junctions is exactly one junction with N = Σ nᵢ and ln Is = Σ nᵢ ln Isᵢ / N.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipSide {
    pub j: Junction,
    pub count: u32,
    pub extra: Option<Junction>,
    pub channel: Option<Channel>,
}

impl ClipSide {
    pub const fn new(j: Junction, count: u32) -> Self {
        Self { j, count, extra: None, channel: None }
    }
    /// (Is, N·V_T) of the equivalent single junction.
    fn equivalent(&self, celsius: f64) -> (f64, f64) {
        let (is1, nvt1) = self.j.at(celsius);
        let c = self.count.max(1) as f64;
        let (mut nvt, mut lnis) = (c * nvt1, c * nvt1 * is1.ln());
        if let Some(x) = self.extra {
            let (is2, nvt2) = x.at(celsius);
            nvt += nvt2;
            lnis += nvt2 * is2.ln();
        }
        ((lnis / nvt).exp(), nvt)
    }
}

/// A two-terminal clipper: `pos` conducts from a to k, `neg` from k to a; `None` = lifted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipperModel {
    pub pos: Option<ClipSide>,
    pub neg: Option<ClipSide>,
}

impl ClipperModel {
    /// A single diode a → k.
    pub const fn diode(j: Junction) -> Self {
        Self { pos: Some(ClipSide::new(j, 1)), neg: None }
    }
    /// Anti-parallel strings of `count` junctions each way.
    pub const fn pair(j: Junction, count: u32) -> Self {
        Self { pos: Some(ClipSide::new(j, count)), neg: Some(ClipSide::new(j, count)) }
    }
    /// Lifted: no current.
    pub const NONE: Self = Self { pos: None, neg: None };
}

/// Ebers–Moll BJT (transport form). `leak` scales the collector junction's saturation current
/// Is / βR (gfx-fuzz's `leakage`; 1 = the device as given). `nf`, `nr`: the emission coefficients of
/// the forward and reverse junctions (1 = ideal; DAFx-17's germanium sets measure 1.06 – 1.17). Is
/// follows the temperature with the forward one: Is(T) = Is · (T / T₀)^(XTI / NF) ·
/// exp(Eg / (NF k) · (1 / T₀ − 1 / T)) (gfx-fuzz §2.5; [`Netlist::to_spice`] writes XTI / NF and
/// Eg / NF, so ngspice's own BJT law, which has no NF, gives the same Is).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BjtModel {
    pub npn: bool,
    pub is: f64,
    pub bf: f64,
    pub br: f64,
    pub leak: f64,
    pub eg: f64,
    pub xti: f64,
    pub tnom: f64,
    pub nf: f64,
    pub nr: f64,
}

impl BjtModel {
    /// The open-base collector current I_CEO = Is (1 + leak (βF + 1) / βR) at `celsius` (the number
    /// builders measure), with the reverse junction fully reverse-biased.
    pub fn iceo(&self, celsius: f64) -> f64 {
        let (is, _) = Junction { is: self.is, n: self.nf, eg: self.eg, xti: self.xti, tnom: self.tnom }.at(celsius);
        is * (1.0 + self.leak * (self.bf + 1.0) / self.br)
    }
}

/// Op-amp macromodel constants. I_t = SR · CC, V_lin = SR / (2π GBW) (tanh) — the slew rate and the
/// gain-bandwidth from one law; a square-law stage (a JFET pair) saturates at ±I_t with the same
/// small-signal transconductance. Rails: the output stops `sat_hi` below the positive rail and
/// `sat_lo` above the negative one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OpAmpModel {
    /// Slew rate, V/s.
    pub sr: f64,
    /// Gain-bandwidth, Hz.
    pub gbw: f64,
    /// Open-loop DC gain.
    pub a0: f64,
    pub sat_hi: f64,
    pub sat_lo: f64,
    pub square_law: bool,
}

/// Shichman–Hodges JFET (SPICE level 1), drain / source symmetric (the higher of the two is the
/// drain). Ports (v_GS, v_GD) like the BJT's (v_BE, v_BC); currents (i_G, i_D). gfx-dist §2.11: the
/// transistor-level reference of the discrete op-amps, `to_spice`, and later gfx-fuzz / preamps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JfetModel {
    pub n_channel: bool,
    /// Pinch-off, V, in the device's own sign as SPICE writes it (negative for a depletion JFET, N or
    /// P: the P channel's voltages are mirrored, its V_TO is not).
    pub vto: f64,
    /// A / V².
    pub beta: f64,
    /// 1 / V.
    pub lambda: f64,
    /// The gate junctions' saturation current, A; 0 = no gate current (exact cuts at a gate).
    pub is: f64,
    /// Their emission coefficient.
    pub n: f64,
    /// V / °C (ngspice VTOTC; 0 = SPICE's default).
    pub vtotc: f64,
    /// % / °C (ngspice BETATCE; 0).
    pub betatce: f64,
    pub tnom: f64,
    /// Knee width at pinch-off, V: v_ov = soft · ln(1 + e^((v_GS − V_TO) / soft)); 0 = SPICE's hard knee.
    pub soft: f64,
}

/// A JFET differential pair on a tail, as one transconductor port (the op-amp macro's input port,
/// with the exact pair law and its own output node): i(drain → 0) = I_t / 2 + Δ / 2,
/// Δ = β v_d √(2 I_t / β − v_d²) for |v_d| < √(I_t / β), ±I_t beyond; v_d = v(+) − v(−); no gate
/// current. `tail_r` > 0: I_t follows the common mode, I_t = v_s / tail_r with v_s = v_cm − (V_TO +
/// √(I_t / 2β)), v_cm from the previous sample (one port); `i_tail` is then only the DC seed. I_t is a
/// constant during a sample's Newton solve; it is latched from the node voltages after each solved
/// sample (gfx-dist §2.11).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PairModel {
    pub beta: f64,
    pub i_tail: f64,
    /// 0 = fixed tail.
    pub tail_r: f64,
    pub vto: f64,
}

impl PairModel {
    /// The tail current a common mode `vcm` gives (the closed form: a quadratic in √I_t).
    pub fn tail_at(&self, vcm: f64) -> f64 {
        if self.tail_r <= 0.0 {
            return self.i_tail;
        }
        let a = 1.0 / (2.0 * self.beta).sqrt();
        let c = (vcm - self.vto).max(0.0);
        let s = (-a + (a * a + 4.0 * self.tail_r * c).sqrt()) / (2.0 * self.tail_r);
        (s * s).max(1e-15)
    }
}

/// What a device is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DeviceModel {
    Clipper(ClipperModel),
    Bjt(BjtModel),
    OpAmp(OpAmpModel),
    Jfet(JfetModel),
    Pair(PairModel),
}

impl DeviceModel {
    fn ports(&self) -> usize {
        match self {
            DeviceModel::Clipper(_) | DeviceModel::Pair(_) => 1,
            DeviceModel::Bjt(_) | DeviceModel::OpAmp(_) | DeviceModel::Jfet(_) => 2,
        }
    }
}

/// A device's evaluated constants (at the current temperature and supply).
#[derive(Clone, Copy, Debug)]
enum Eval {
    Clipper {
        // per side: Is (0 = lifted), N·V_T, vcrit, channel (vth, k; k = 0: none)
        isp: f64,
        nvp: f64,
        vcp: f64,
        chp: (f64, f64),
        isn: f64,
        nvn: f64,
        vcn: f64,
        chn: (f64, f64),
        ivp: f64,
        ivn: f64,
    },
    Bjt {
        s: f64,
        is: f64,
        vt: f64,
        bf: f64,
        br: f64,
        kl: f64,
        vcrit: f64,
        ivt: f64,
        // the reverse junction's N·V_T, critical voltage and 1 / N·V_T (= the forward's at NR = NF)
        vtr: f64,
        vcritr: f64,
        ivtr: f64,
    },
    OpAmp {
        it: f64,
        vlin: f64,
        square: bool,
        g: f64,
        hi: f64,
        lo: f64,
        vc: f64,
        gc: f64,
        ivlin: f64,
        ivc: f64,
    },
    Jfet {
        s: f64,
        beta: f64,
        vto: f64,
        lambda: f64,
        soft: f64,
        is: f64,
        nvt: f64,
        vcrit: f64,
        ivt: f64,
    },
    Pair {
        beta: f64,
        /// The latched tail current, and from it √(I_t / β) and 2 I_t / β.
        it: f64,
        lim: f64,
        k2: f64,
        model: PairModel,
    },
}

/// The pair port's constants for tail current `it`.
fn pair_eval(model: PairModel, it: f64) -> Eval {
    Eval::Pair { beta: model.beta, it, lim: (it / model.beta).sqrt(), k2: 2.0 * it / model.beta, model }
}

/// A JFET channel in its normal mode (v_GS = `a` ≥ v_GD = `b`): (i_DS, ∂i/∂a, ∂i/∂b).
#[inline]
fn jfet_channel(beta: f64, vto: f64, lambda: f64, soft: f64, a: f64, b: f64) -> (f64, f64, f64) {
    let x = a - vto;
    let (g, gp) = if soft > 0.0 {
        softplus(x, soft, 1.0 / soft)
    } else if x > 0.0 {
        (x, 1.0)
    } else {
        (0.0, 0.0)
    };
    if g <= 0.0 {
        return (0.0, 0.0, 0.0);
    }
    let d = a - b;
    let cl = 1.0 + lambda * d;
    let (i, di_dd, di_dg) = if d >= g {
        (beta * g * g * cl, beta * g * g * lambda, 2.0 * beta * g * cl)
    } else {
        let f = d * (2.0 * g - d);
        (beta * f * cl, beta * ((2.0 * g - 2.0 * d) * cl + f * lambda), 2.0 * beta * d * cl)
    };
    (i, di_dd + di_dg * gp, -di_dd)
}

/// The rail clamp: a knee of 20 mV, then a conductance that holds the full slew current within
/// 10 mV more: the integrator stops ≈ 20 mV past its rail, with a continuous slope.
const RAIL_KNEE: f64 = 0.05;
const RAIL_HOLD: f64 = 0.05;

fn evaluate(model: &DeviceModel, rails: (f64, f64), celsius: f64, supply: f64) -> Eval {
    match *model {
        DeviceModel::Clipper(c) => {
            let side = |s: Option<ClipSide>| -> (f64, f64, f64, (f64, f64)) {
                match s {
                    Some(s) => {
                        let (is, nvt) = s.equivalent(celsius);
                        let ch = s.channel.map_or((0.0, 0.0), |c| (c.vth, c.k));
                        (is, nvt, vcrit(is, nvt), ch)
                    }
                    None => (0.0, 1.0, 1e9, (0.0, 0.0)),
                }
            };
            let (isp, nvp, vcp, chp) = side(c.pos);
            let (isn, nvn, vcn, chn) = side(c.neg);
            Eval::Clipper { isp, nvp, vcp, chp, isn, nvn, vcn, chn, ivp: 1.0 / nvp, ivn: 1.0 / nvn }
        }
        DeviceModel::Bjt(b) => {
            let j = Junction { is: b.is, n: b.nf, eg: b.eg, xti: b.xti, tnom: b.tnom };
            let (is, vt) = j.at(celsius);
            let vtr = if b.nr == b.nf { vt } else { b.nr * K_Q * (celsius + KELVIN) };
            Eval::Bjt {
                s: if b.npn { 1.0 } else { -1.0 },
                is,
                vt,
                bf: b.bf,
                br: b.br,
                kl: b.leak,
                vcrit: vcrit(is, vt),
                ivt: 1.0 / vt,
                vtr,
                vcritr: vcrit(is, vtr),
                ivtr: 1.0 / vtr,
            }
        }
        DeviceModel::OpAmp(o) => {
            let it = o.sr * CC;
            let vlin = o.sr / (2.0 * PI * o.gbw);
            let gm = it / vlin;
            let hi = rails.1 * supply - o.sat_hi;
            let lo = rails.0 * supply + o.sat_lo;
            Eval::OpAmp {
                it,
                vlin,
                square: o.square_law,
                g: gm / o.a0,
                hi,
                lo,
                vc: RAIL_KNEE,
                gc: it / RAIL_HOLD,
                ivlin: 1.0 / vlin,
                ivc: 1.0 / RAIL_KNEE,
            }
        }
        DeviceModel::Jfet(f) => {
            // ngspice level 1: V_TO + VTOTC (T − T₀), β · 1.01^(BETATCE (T − T₀)); Is by the junction law
            let dt = celsius - f.tnom;
            let (is, nvt) = if f.is > 0.0 { Junction { is: f.is, n: f.n, eg: 1.11, xti: 3.0, tnom: f.tnom }.at(celsius) } else { (0.0, f.n * thermal_voltage(celsius)) };
            Eval::Jfet {
                s: if f.n_channel { 1.0 } else { -1.0 },
                beta: f.beta * 1.01f64.powf(f.betatce * dt),
                vto: f.vto + f.vtotc * dt,
                lambda: f.lambda,
                soft: f.soft,
                is,
                nvt,
                vcrit: if is > 0.0 { vcrit(is, nvt) } else { 1e9 },
                ivt: 1.0 / nvt,
            }
        }
        DeviceModel::Pair(p) => pair_eval(p, p.i_tail),
    }
}

impl Eval {
    /// The device's port count.
    fn ports(&self) -> usize {
        match self {
            Eval::Clipper { .. } | Eval::Pair { .. } => 1,
            _ => 2,
        }
    }
}

impl Eval {
    /// Port currents and their Jacobian (rows / cols `off..off + ports`, row stride `m`).
    #[inline]
    fn eval(&self, v: &[f64], i: &mut [f64], jac: &mut [f64], m: usize, off: usize) -> usize {
        match *self {
            Eval::Clipper { isp, nvp, isn, nvn, chp, chn, ivp, ivn, .. } => {
                let x = v[off];
                let (mut cur, mut d) = (0.0, 0.0);
                let mut exps = 0;
                if isp > 0.0 && isn > 0.0 && nvp == nvn {
                    // one exp serves both directions
                    let (e, de) = exp_lin(x.abs() * ivp);
                    exps = 1;
                    let r = 1.0 / e;
                    let (ep, dp, en, dn) = if x >= 0.0 { (e, de, r, r) } else { (r, r, e, de) };
                    cur += isp * (ep - 1.0) - isn * (en - 1.0);
                    d += (isp * dp + isn * dn) * ivp;
                } else {
                    if isp > 0.0 {
                        let (e, de) = exp_lin(x * ivp);
                        cur += isp * (e - 1.0);
                        d += isp * de * ivp;
                        exps += 1;
                    }
                    if isn > 0.0 {
                        let (e, de) = exp_lin(-x * ivn);
                        cur -= isn * (e - 1.0);
                        d += isn * de * ivn;
                        exps += 1;
                    }
                }
                if chp.1 > 0.0 && x > chp.0 {
                    cur += chp.1 * (x - chp.0) * (x - chp.0);
                    d += 2.0 * chp.1 * (x - chp.0);
                }
                if chn.1 > 0.0 && -x > chn.0 {
                    cur -= chn.1 * (-x - chn.0) * (-x - chn.0);
                    d += 2.0 * chn.1 * (-x - chn.0);
                }
                // a lifted clipper still conducts gmin (keeps the Jacobian honest)
                cur += GMIN * x;
                d += GMIN;
                i[off] = cur;
                jac[off * m + off] = d;
                exps
            }
            Eval::Bjt { s, is, bf, br, kl, ivt, ivtr, .. } => {
                let vbe = s * v[off];
                let vbc = s * v[off + 1];
                let (ef, def) = exp_lin(vbe * ivt);
                let (er, der) = exp_lin(vbc * ivtr);
                let i_f = is * (ef - 1.0);
                let i_r = is * (er - 1.0);
                let gf = is * def * ivt;
                let gr = is * der * ivtr;
                i[off] = s * (i_f / bf + kl * i_r / br);
                i[off + 1] = s * (i_f - i_r - kl * i_r / br);
                jac[off * m + off] = gf / bf;
                jac[off * m + off + 1] = kl * gr / br;
                jac[(off + 1) * m + off] = gf;
                jac[(off + 1) * m + off + 1] = -gr * (1.0 + kl / br);
                2
            }
            Eval::OpAmp { it, square, g, hi, lo, vc, gc, ivlin, ivc, .. } => {
                let x = v[off] * ivlin;
                let mut exps = 0;
                if square {
                    // square law of a JFET pair, saturating at ±I_t; V_ov = 2 V_lin keeps gm
                    let y = 0.5 * x;
                    if y.abs() < 1.0 {
                        i[off] = it * y * (2.0 - y.abs());
                        jac[off * m + off] = it * (1.0 - y.abs()) * ivlin;
                    } else {
                        i[off] = it * y.signum();
                        jac[off * m + off] = 0.0;
                    }
                } else {
                    let a = fast_exp(-2.0 * x.abs());
                    let q = 1.0 / (1.0 + a);
                    let th = (1.0 - a) * q;
                    jac[off * m + off] = it * ivlin * 4.0 * a * q * q;
                    i[off] = it * th * x.signum();
                    exps += 1;
                }
                let w = v[off + 1];
                let mid = 0.5 * (hi + lo);
                let mut cur = g * (w - mid);
                let mut d = g;
                // the rail clamp: a diode-like conductance gc with a soft-plus knee of vc
                let x = if w > mid { w - hi } else { lo - w };
                let s = if w > mid { 1.0 } else { -1.0 };
                let (sp, ds) = softplus(x, vc, ivc);
                cur += s * gc * sp;
                d += gc * ds;
                exps += 1;
                i[off + 1] = cur;
                jac[(off + 1) * m + off + 1] = d;
                exps
            }
            Eval::Jfet { s, beta, vto, lambda, soft, is, ivt, .. } => {
                let x = s * v[off];
                let y = s * v[off + 1];
                let (ic, dx, dy) = if x >= y {
                    jfet_channel(beta, vto, lambda, soft, x, y)
                } else {
                    let (i2, da, db) = jfet_channel(beta, vto, lambda, soft, y, x);
                    (-i2, -db, -da)
                };
                let mut exps = if soft > 0.0 { 1 } else { 0 };
                let (mut igs, mut ggs, mut igd, mut ggd) = (0.0, 0.0, 0.0, 0.0);
                if is > 0.0 {
                    let (es, des) = exp_lin(x * ivt);
                    let (ed, ded) = exp_lin(y * ivt);
                    igs = is * (es - 1.0);
                    ggs = is * des * ivt;
                    igd = is * (ed - 1.0);
                    ggd = is * ded * ivt;
                    exps += 2;
                }
                i[off] = s * (igs + igd);
                i[off + 1] = s * (ic - igd);
                jac[off * m + off] = ggs;
                jac[off * m + off + 1] = ggd;
                jac[(off + 1) * m + off] = dx;
                jac[(off + 1) * m + off + 1] = dy - ggd;
                exps
            }
            Eval::Pair { beta, it, lim, k2, .. } => {
                let x = v[off];
                if x.abs() < lim {
                    let r = (k2 - x * x).sqrt();
                    i[off] = 0.5 * (it + beta * x * r);
                    jac[off * m + off] = 0.5 * beta * (k2 - 2.0 * x * x) / r;
                } else {
                    i[off] = 0.5 * (it + it * x.signum());
                    jac[off * m + off] = 0.0;
                }
                0
            }
        }
    }

    /// The pair port's small-signal transconductance to its common mode (the tail's share), at port
    /// voltage `vd` and common mode `vcm`; 0 for a fixed tail or another device.
    fn pair_gcm(&self, vd: f64, vcm: f64) -> f64 {
        let Eval::Pair { beta, model, .. } = *self else { return 0.0 };
        if model.tail_r <= 0.0 || vcm - model.vto <= 0.0 {
            return 0.0;
        }
        let it = model.tail_at(vcm);
        // R_t I + a √I = v_cm − V_TO  →  dI / dv_cm = 1 / (R_t + a / (2 √I))
        let a = 1.0 / (2.0 * beta).sqrt();
        let di = 1.0 / (model.tail_r + a / (2.0 * it.sqrt()));
        let k2 = 2.0 * it / beta;
        let dd = if vd * vd < it / beta { vd / (k2 - vd * vd).sqrt() } else { vd.signum() };
        0.5 * di * (1.0 + dd)
    }

    /// Latches a pair's tail from the common mode `vcm` (no-op for a fixed tail or another device).
    fn latch(&mut self, vcm: f64) -> f64 {
        if let Eval::Pair { model, it, .. } = *self {
            if model.tail_r > 0.0 {
                let nt = model.tail_at(vcm);
                *self = pair_eval(model, nt);
                return (nt - it).abs();
            }
        }
        0.0
    }

    /// Device limiting of a Newton step: `vnew` is changed in place.
    #[inline]
    fn limit(&self, vnew: &mut [f64], vold: &[f64], off: usize, dc: bool) {
        match *self {
            Eval::Clipper { nvp, vcp, nvn, vcn, isp, isn, .. } => {
                let (a, b) = (vnew[off], vold[off]);
                vnew[off] = if a >= 0.0 {
                    if isp > 0.0 {
                        pnjlim(a, b, nvp, vcp)
                    } else {
                        a
                    }
                } else if isn > 0.0 {
                    -pnjlim(-a, -b, nvn, vcn)
                } else {
                    a
                };
            }
            Eval::Bjt { s, vt, vcrit, vtr, vcritr, .. } => {
                vnew[off] = s * pnjlim(s * vnew[off], s * vold[off], vt, vcrit);
                vnew[off + 1] = s * pnjlim(s * vnew[off + 1], s * vold[off + 1], vtr, vcritr);
            }
            Eval::OpAmp { vlin, hi, lo, vc, .. } => {
                // the input stage moves by at most 3 V_lin, unless it stays saturated on one side
                let (a, b) = (vnew[off], vold[off]);
                let lim = 3.0 * vlin;
                let saturated = dc || a.abs() > lim && b.abs() > lim && a.signum() == b.signum();
                if !saturated {
                    vnew[off] = b + (a - b).clamp(-lim, lim);
                }
                // the integrator stops at a rail's knee when it reaches it from inside: the loop
                // breaks there, and the linear prediction past the rail is wrong
                let (a, b) = (vnew[off + 1], vold[off + 1]);
                let k = 3.0 * vc;
                if a > hi - k && b < hi - k {
                    vnew[off + 1] = hi - k;
                } else if a < lo + k && b > lo + k {
                    vnew[off + 1] = lo + k;
                }
            }
            Eval::Jfet { s, is, nvt, vcrit, .. } => {
                // the gate junctions only; the channel is a square law (gfx-dist §2.11)
                if is > 0.0 {
                    vnew[off] = s * pnjlim(s * vnew[off], s * vold[off], nvt, vcrit);
                    vnew[off + 1] = s * pnjlim(s * vnew[off + 1], s * vold[off + 1], nvt, vcrit);
                }
            }
            Eval::Pair { .. } => {}
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The netlist

/// One element, between global nodes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Element {
    R(Node, Node, f64),
    /// Capacitor, trapezoidal.
    C(Node, Node, f64),
    /// Capacitor, backward Euler (damps a stiff mode the trapezoidal rule would ring at fs / 2).
    CBe(Node, Node, f64),
    L(Node, Node, f64),
    /// Variable resistor (index into the netlist's variables).
    Var(Node, Node, usize),
    /// Current source from input `k`, flowing out of the first node, through the source, into the second.
    ISrc(Node, Node, usize),
    /// Ideal op-amp: V(+) = V(−), any output current. (+, −, out).
    OpAmp(Node, Node, Node),
    /// Ideal unity buffer: V(out) = V(ctl). (ctl, out).
    Buffer(Node, Node),
    /// A device (index into the netlist's devices) on its terminals: clipper (a, k, 0), BJT (c, b, e),
    /// op-amp input stage (+, −, integrator node).
    Device(usize, [Node; 3]),
}

impl Element {
    /// The element's nodes.
    pub fn nodes(&self) -> Vec<Node> {
        match *self {
            Element::R(a, b, _)
            | Element::C(a, b, _)
            | Element::CBe(a, b, _)
            | Element::L(a, b, _)
            | Element::Var(a, b, _)
            | Element::ISrc(a, b, _)
            | Element::Buffer(a, b) => vec![a, b],
            Element::OpAmp(a, b, c) => vec![a, b, c],
            Element::Device(_, n) => n.to_vec(),
        }
    }

    /// The same element with its nodes mapped.
    pub fn map(&self, f: impl Fn(Node) -> Node) -> Element {
        match *self {
            Element::R(a, b, v) => Element::R(f(a), f(b), v),
            Element::C(a, b, v) => Element::C(f(a), f(b), v),
            Element::CBe(a, b, v) => Element::CBe(f(a), f(b), v),
            Element::L(a, b, v) => Element::L(f(a), f(b), v),
            Element::Var(a, b, k) => Element::Var(f(a), f(b), k),
            Element::ISrc(a, b, k) => Element::ISrc(f(a), f(b), k),
            Element::Buffer(a, b) => Element::Buffer(f(a), f(b)),
            Element::OpAmp(a, b, c) => Element::OpAmp(f(a), f(b), f(c)),
            Element::Device(d, n) => Element::Device(d, [f(n[0]), f(n[1]), f(n[2])]),
        }
    }
}

/// A pot: one or two variable resistors (start → wiper, wiper → end) and its law.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pot {
    pub start: Option<usize>,
    pub end: Option<usize>,
    pub total: f64,
    pub taper: Taper,
}

impl Pot {
    /// (start → wiper, wiper → end) resistances at rotation `t` (0 – 1), ends clamped at R_MIN.
    pub fn halves(&self, t: f64) -> (f64, f64) {
        let f = self.taper.fraction(t);
        ((self.total * f).max(R_MIN), (self.total * (1.0 - f)).max(R_MIN))
    }
}

/// A circuit description, written once and compiled by [`NonlinearCircuit`].
#[derive(Clone, Debug)]
pub struct Netlist {
    nodes: usize,
    elements: Vec<Element>,
    stage_of: Vec<usize>,
    stage: usize,
    source_of: Vec<Option<usize>>,
    cut: Vec<bool>,
    devices: Vec<DeviceModel>,
    rails: Vec<(f64, f64)>,
    vars: Vec<(f64, f64, f64)>, // (initial, min, max) ohms
    pots: Vec<Pot>,
    outputs: Vec<Node>,
    /// Outputs read on demand ([`NonlinearCircuit::probe`]), not computed every sample.
    lazy: Vec<bool>,
    inputs: usize,
    names: Vec<(Node, &'static str)>,
}

impl Netlist {
    /// An empty netlist with `inputs` inputs (the vector u).
    pub fn new(inputs: usize) -> Self {
        Self {
            nodes: 1,
            elements: Vec::new(),
            stage_of: Vec::new(),
            stage: 0,
            source_of: vec![None],
            cut: vec![false],
            devices: Vec::new(),
            rails: Vec::new(),
            vars: Vec::new(),
            pots: Vec::new(),
            outputs: Vec::new(),
            lazy: Vec::new(),
            inputs,
            names: Vec::new(),
        }
    }

    pub fn inputs(&self) -> usize {
        self.inputs
    }
    pub fn elements(&self) -> &[Element] {
        &self.elements
    }
    pub fn devices(&self) -> &[DeviceModel] {
        &self.devices
    }
    pub fn pots(&self) -> &[Pot] {
        &self.pots
    }
    pub fn stages(&self) -> usize {
        self.stage + 1
    }

    /// A new node.
    pub fn node(&mut self) -> Node {
        self.nodes += 1;
        self.source_of.push(None);
        self.cut.push(false);
        self.nodes - 1
    }

    /// A new node with a name (for `to_spice` and error messages).
    pub fn named(&mut self, name: &'static str) -> Node {
        let n = self.node();
        self.names.push((n, name));
        n
    }

    /// The node driven by input `k` (an ideal source to ground; one node per input).
    pub fn input(&mut self, k: usize) -> Node {
        assert!(k < self.inputs, "input {k} of {}", self.inputs);
        if let Some(n) = self.source_of.iter().position(|s| *s == Some(k)) {
            return n;
        }
        let n = self.node();
        self.source_of[n] = Some(k);
        n
    }

    fn push(&mut self, e: Element) {
        self.elements.push(e);
        self.stage_of.push(self.stage);
    }

    pub fn r(&mut self, a: Node, b: Node, ohms: f64) {
        self.push(Element::R(a, b, ohms));
    }
    pub fn c(&mut self, a: Node, b: Node, farads: f64) {
        self.push(Element::C(a, b, farads));
    }
    /// A capacitor discretised by backward Euler.
    pub fn c_be(&mut self, a: Node, b: Node, farads: f64) {
        self.push(Element::CBe(a, b, farads));
    }
    pub fn l(&mut self, a: Node, b: Node, henries: f64) {
        self.push(Element::L(a, b, henries));
    }
    pub fn isrc(&mut self, a: Node, b: Node, input: usize) {
        self.push(Element::ISrc(a, b, input));
    }
    pub fn opamp_ideal(&mut self, plus: Node, minus: Node, out: Node) {
        self.push(Element::OpAmp(plus, minus, out));
    }
    pub fn buffer(&mut self, ctl: Node, out: Node) {
        self.push(Element::Buffer(ctl, out));
    }

    /// A variable resistor between `a` and `b`, `ohms` now, moving within [min, max] (the Woodbury
    /// nominal is their geometric mean). Returns its index.
    pub fn var(&mut self, a: Node, b: Node, ohms: f64, min: f64, max: f64) -> usize {
        self.vars.push((ohms.clamp(min, max), min.max(R_MIN), max.max(R_MIN)));
        let k = self.vars.len() - 1;
        self.push(Element::Var(a, b, k));
        k
    }

    /// A three-terminal pot: track `start` → `end`, wiper `wiper`. Returns the pot's index.
    pub fn pot(&mut self, start: Node, wiper: Node, end: Node, total: f64, taper: Taper, rotation: f64) -> usize {
        let p = Pot { start: None, end: None, total, taper };
        let (ra, rb) = p.halves(rotation);
        let s = self.var(start, wiper, ra, R_MIN, total);
        let e = self.var(wiper, end, rb, R_MIN, total);
        self.pots.push(Pot { start: Some(s), end: Some(e), total, taper });
        self.pots.len() - 1
    }

    /// A pot wired as a rheostat: the resistance start → wiper only (the end tied to the wiper).
    pub fn rheostat(&mut self, a: Node, b: Node, total: f64, taper: Taper, rotation: f64) -> usize {
        let p = Pot { start: None, end: None, total, taper };
        let (ra, _) = p.halves(rotation);
        let s = self.var(a, b, ra, R_MIN, total);
        self.pots.push(Pot { start: Some(s), end: None, total, taper });
        self.pots.len() - 1
    }

    fn device(&mut self, model: DeviceModel, rails: (f64, f64), nodes: [Node; 3]) -> usize {
        self.devices.push(model);
        self.rails.push(rails);
        let k = self.devices.len() - 1;
        self.push(Element::Device(k, nodes));
        k
    }

    /// A clipper from `a` to `k` (diode, pair, strings, MOSFET). Returns the device index.
    pub fn clipper(&mut self, a: Node, k: Node, model: ClipperModel) -> usize {
        self.device(DeviceModel::Clipper(model), (0.0, 0.0), [a, k, 0])
    }

    /// A BJT (collector, base, emitter). Returns the device index.
    pub fn bjt(&mut self, c: Node, b: Node, e: Node, model: BjtModel) -> usize {
        self.device(DeviceModel::Bjt(model), (0.0, 0.0), [c, b, e])
    }

    /// A JFET (drain, gate, source): 2 ports. Returns the device index.
    pub fn jfet(&mut self, d: Node, g: Node, s: Node, model: JfetModel) -> usize {
        self.device(DeviceModel::Jfet(model), (0.0, 0.0), [d, g, s])
    }

    /// A JFET differential pair on its tail as one port: the + device's drain current into `drain`
    /// (from `drain` to ground), controlled by v(`plus`) − v(`minus`); the − device's drain and the
    /// tail are not nodes of the netlist. Returns the device index.
    pub fn jfet_pair(&mut self, plus: Node, minus: Node, drain: Node, model: PairModel) -> usize {
        self.device(DeviceModel::Pair(model), (0.0, 0.0), [plus, minus, drain])
    }

    /// An op-amp macromodel (+, −, out). `rails` = (negative, positive) rail as multiples of the
    /// supply ([`NonlinearCircuit::set_supply`]): (0, 1) on a single supply. `rout` > 0 puts an
    /// output resistance after the ideal buffer (then the output is no exact cut). Returns the
    /// device index.
    pub fn opamp(&mut self, plus: Node, minus: Node, out: Node, model: OpAmpModel, rails: (f64, f64), rout: f64) -> usize {
        let int = self.node();
        let d = self.device(DeviceModel::OpAmp(model), rails, [plus, minus, int]);
        self.c(int, 0, CC);
        if rout > 0.0 {
            let b = self.node();
            self.buffer(int, b);
            self.r(b, out, rout);
        } else {
            self.buffer(int, out);
        }
        d
    }

    /// Ends the current stage at `node` (exact cuts only: an ideal source's output, or a node that
    /// draws no current downstream). Later elements belong to the next stage, which sees `node` as an
    /// ideal source.
    pub fn cut(&mut self, node: Node) {
        self.cut[node] = true;
        self.stage += 1;
    }

    /// Declares a node voltage readable after each sample (an output or a meter). Returns its index.
    pub fn output(&mut self, node: Node) -> usize {
        self.outputs.push(node);
        self.lazy.push(false);
        self.outputs.len() - 1
    }

    /// Declares a node voltage read on demand (a meter read once per block or per host sample):
    /// [`NonlinearCircuit::probe`] computes it from the last sample, the solve does not. Returns its
    /// index (the same index space as [`Netlist::output`]).
    pub fn probe(&mut self, node: Node) -> usize {
        self.outputs.push(node);
        self.lazy.push(true);
        self.outputs.len() - 1
    }

    /// The element count (a mark for [`Netlist::linear_twin`]).
    pub fn mark(&self) -> usize {
        self.elements.len()
    }

    /// The linear twin of the elements from `from` on: clippers and BJTs removed, op-amp macros made
    /// ideal, monolithic, `entry` driven by a new last input, the same outputs. Other inputs keep their
    /// indices; any other node the twin shares with the elements before `from` (another stage's
    /// output feeding the same network) is grounded: the twin carries the entry's signal only.
    pub fn linear_twin(&self, from: usize, entry: Node) -> Netlist {
        let mut t = Netlist::new(self.inputs + 1);
        t.nodes = self.nodes;
        t.source_of = self.source_of.clone();
        t.source_of[entry] = Some(self.inputs);
        t.cut = vec![false; self.nodes];
        t.names = self.names.clone();
        let mut before = vec![false; self.nodes];
        for e in &self.elements[..from] {
            for n in e.nodes() {
                before[n] = true;
            }
        }
        let g = |n: Node| if n != entry && before[n] && self.source_of[n].is_none() { 0 } else { n };
        for e in &self.elements[from..] {
            match e.map(g) {
                Element::Device(d, n) => {
                    if let DeviceModel::OpAmp(_) = self.devices[d] {
                        t.push(Element::OpAmp(n[0], n[1], n[2]));
                    }
                }
                Element::Var(a, b, k) => {
                    let (v, lo, hi) = self.vars[k];
                    t.var(a, b, v, lo, hi);
                }
                e => t.push(e),
            }
        }
        // the pots keep their indices only if every variable was copied: map them by order
        let first_var = self.elements[..from].iter().filter(|e| matches!(e, Element::Var(..))).count();
        for p in &self.pots {
            let shift = |v: Option<usize>| v.and_then(|k| k.checked_sub(first_var));
            if p.start.is_none_or(|k| k >= first_var) && p.end.is_none_or(|k| k >= first_var) {
                t.pots.push(Pot { start: shift(p.start), end: shift(p.end), total: p.total, taper: p.taper });
            } else {
                t.pots.push(Pot { start: None, end: None, total: p.total, taper: p.taper });
            }
        }
        t.outputs = self.outputs.clone();
        t.lazy = self.lazy.clone();
        t
    }

    /// The same netlist as an ngspice deck: element cards, `.model` cards (diodes, BJTs), op-amp
    /// macros as B-sources, the inputs as DC sources at `inputs`, `.temp`, and an operating-point
    /// analysis. Device constants at their nominal temperature; pots at `pot_rotations`.
    pub fn to_spice(&self, title: &str, inputs: &[f64], pot_rotations: &[f64], celsius: f64, supply: f64) -> String {
        let mut s = format!("* {title}\n* generated by gfx-dsp circuit_nl::Netlist::to_spice\n.temp {celsius}\n");
        let name = |n: Node| -> String {
            if n == 0 {
                "0".into()
            } else {
                format!("n{n}")
            }
        };
        let mut vals: Vec<f64> = self.vars.iter().map(|v| v.0).collect();
        for (k, p) in self.pots.iter().enumerate() {
            if let Some(&t) = pot_rotations.get(k) {
                let (a, b) = p.halves(t);
                if let Some(i) = p.start {
                    vals[i] = a;
                }
                if let Some(i) = p.end {
                    vals[i] = b;
                }
            }
        }
        for (n, src) in self.source_of.iter().enumerate() {
            if let Some(k) = src {
                s += &format!("VIN{k} {} 0 DC {}\n", name(n), inputs.get(*k).copied().unwrap_or(0.0));
            }
        }
        let mut extra = self.nodes;
        let mut models = String::new();
        for (k, e) in self.elements.iter().enumerate() {
            match *e {
                Element::R(a, b, r) => s += &format!("R{k} {} {} {r:e}\n", name(a), name(b)),
                Element::C(a, b, c) | Element::CBe(a, b, c) => s += &format!("C{k} {} {} {c:e}\n", name(a), name(b)),
                Element::L(a, b, l) => s += &format!("L{k} {} {} {l:e}\n", name(a), name(b)),
                Element::Var(a, b, v) => s += &format!("R{k} {} {} {:e}\n", name(a), name(b), vals[v]),
                Element::ISrc(a, b, i) => {
                    s += &format!("I{k} {} {} DC {}\n", name(a), name(b), inputs.get(i).copied().unwrap_or(0.0))
                }
                Element::OpAmp(p, m, o) => s += &format!("E{k} {} 0 {} {} 1e9\n", name(o), name(p), name(m)),
                Element::Buffer(c, o) => s += &format!("E{k} {} 0 {} 0 1\n", name(o), name(c)),
                Element::Device(d, n) => match self.devices[d] {
                    DeviceModel::Clipper(c) => {
                        for (dir, side) in [(0, c.pos), (1, c.neg)] {
                            let Some(side) = side else { continue };
                            let (from, to) = if dir == 0 { (n[0], n[1]) } else { (n[1], n[0]) };
                            let mut chain: Vec<(Junction, &str)> = (0..side.count).map(|_| (side.j, "J")).collect();
                            if let Some(x) = side.extra {
                                chain.push((x, "X"));
                            }
                            let mut at = from;
                            for (q, (j, tag)) in chain.iter().enumerate() {
                                let next = if q + 1 == chain.len() {
                                    to
                                } else {
                                    extra += 1;
                                    extra - 1
                                };
                                let m = format!("D{k}{dir}{tag}");
                                s += &format!("D{k}_{dir}_{q} {} {} {m}\n", name(at), name(next));
                                models += &format!(".model {m} D(IS={:e} N={} EG={} XTI={} TNOM={})\n", j.is, j.n, j.eg, j.xti, j.tnom);
                                at = next;
                            }
                            if let Some(ch) = side.channel {
                                s += &format!(
                                    "B{k}_{dir}ch {} {} I = {:e} * (V({},{}) > {} ? (V({},{}) - {})^2 : 0)\n",
                                    name(from),
                                    name(to),
                                    ch.k,
                                    name(from),
                                    name(to),
                                    ch.vth,
                                    name(from),
                                    name(to),
                                    ch.vth
                                );
                            }
                        }
                    }
                    DeviceModel::Bjt(b) => {
                        let m = format!("Q{k}M");
                        s += &format!("Q{k} {} {} {} {m}\n", name(n[0]), name(n[1]), name(n[2]));
                        // ngspice's BJT temperature law has no NF: Eg / NF and XTI / NF give ours; the
                        // leakage scale is βR / leak (the collector junction's Is / βR × leak)
                        let nfr = if b.nf != 1.0 || b.nr != 1.0 { format!(" NF={} NR={}", b.nf, b.nr) } else { String::new() };
                        models += &format!(
                            ".model {m} {}(IS={:e} BF={} BR={} EG={} XTI={} TNOM={}{nfr})\n",
                            if b.npn { "NPN" } else { "PNP" },
                            b.is,
                            b.bf,
                            b.br / b.leak.max(1e-12),
                            b.eg / b.nf,
                            b.xti / b.nf,
                            b.tnom
                        );
                    }
                    DeviceModel::OpAmp(o) => {
                        let ev = evaluate(&self.devices[d], self.rails[d], celsius, supply);
                        if let Eval::OpAmp { it, vlin, g, hi, lo, vc, gc, .. } = ev {
                            let vd = format!("V({},{})", name(n[0]), name(n[1]));
                            let stage = if o.square_law {
                                format!(
                                    "{it:e} * (abs({vd}/{}) < 1 ? ({vd}/{}) * (2 - abs({vd}/{})) : sgn({vd}))",
                                    2.0 * vlin,
                                    2.0 * vlin,
                                    2.0 * vlin
                                )
                            } else {
                                format!("{it:e} * tanh({vd}/{vlin:e})")
                            };
                            s += &format!("B{k}in 0 {} I = {stage}\n", name(n[2]));
                            let w = format!("V({})", name(n[2]));
                            let clamp = |x: String| {
                                format!("{:e} * ln(1 + exp(({x}) / {vc}))", gc * vc)
                            };
                            s += &format!(
                                "B{k}rail {} 0 I = {g:e} * ({w} - {}) + {} - {}\n",
                                name(n[2]),
                                0.5 * (hi + lo),
                                clamp(format!("{w} - {hi}")),
                                clamp(format!("{lo} - {w}"))
                            );
                        }
                    }
                    DeviceModel::Jfet(f) => {
                        let m = format!("J{k}M");
                        if f.soft > 0.0 {
                            // ngspice's level 1 has no soft knee: the channel as a B-source (flagged), the
                            // gate junctions as diodes
                            s += &format!("* J{k}: soft pinch-off knee ({} V), written as a B-source\n", f.soft);
                            let sg = if f.n_channel { 1.0 } else { -1.0 };
                            let (d, g, sn) = (name(n[0]), name(n[1]), name(n[2]));
                            let vgs = format!("({sg}*V({g},{sn}))");
                            let vgd = format!("({sg}*V({g},{d}))");
                            let ov = |v: &str| format!("({} * ln(1 + exp(({v} - {}) / {})))", f.soft, f.vto, f.soft);
                            let ch = |a: &str, b: &str| {
                                let (g2, dd) = (ov(a), format!("({a} - {b})"));
                                format!("({dd} >= {g2} ? {:e} * {g2}^2 * (1 + {} * {dd}) : {:e} * {dd} * (2 * {g2} - {dd}) * (1 + {} * {dd}))", f.beta, f.lambda, f.beta, f.lambda)
                            };
                            s += &format!("B{k}ch {d} {sn} I = {sg} * ({vgs} >= {vgd} ? {} : -{})\n", ch(&vgs, &vgd), ch(&vgd, &vgs));
                            if f.is > 0.0 {
                                let (a, b) = if f.n_channel { (g.clone(), sn.clone()) } else { (sn.clone(), g.clone()) };
                                s += &format!("D{k}gs {a} {b} {m}D\nD{k}gd {} {} {m}D\n", if f.n_channel { &g } else { &d }, if f.n_channel { &d } else { &g });
                                models += &format!(".model {m}D D(IS={:e} N={} TNOM={})\n", f.is, f.n, f.tnom);
                            }
                        } else {
                            s += &format!("J{k} {} {} {} {m}\n", name(n[0]), name(n[1]), name(n[2]));
                            models += &format!(
                                ".model {m} {}(VTO={} BETA={:e} LAMBDA={} IS={:e} N={} VTOTC={} BETATCE={} TNOM={})\n",
                                if f.n_channel { "NJF" } else { "PJF" },
                                f.vto,
                                f.beta,
                                f.lambda,
                                f.is,
                                f.n,
                                f.vtotc,
                                f.betatce,
                                f.tnom
                            );
                        }
                    }
                    DeviceModel::Pair(p) => {
                        // the pair port with its tail solved implicitly (the DC point the real-time solve
                        // latches: gfx-dist §2.11)
                        let vd = format!("V({},{})", name(n[0]), name(n[1]));
                        let it = if p.tail_r > 0.0 {
                            let a = 1.0 / (2.0 * p.beta).sqrt();
                            let c = format!("max(0.5 * (V({}) + V({})) - ({}), 0)", name(n[0]), name(n[1]), p.vto);
                            format!("((-{a:e} + sqrt({:e} + {:e} * {c})) / {:e})^2", a * a, 4.0 * p.tail_r, 2.0 * p.tail_r)
                        } else {
                            format!("{:e}", p.i_tail)
                        };
                        s += &format!("* B{k}: JFET pair port (beta {:e}, V_TO {}, tail {} ohm)\n", p.beta, p.vto, p.tail_r);
                        s += &format!(
                            "B{k} {} 0 I = 0.5 * {it} + 0.5 * (abs({vd}) < sqrt({it} / {b:e}) ? {b:e} * {vd} * sqrt(2 * {it} / {b:e} - {vd}^2) : {it} * sgn({vd}))\n",
                            name(n[2]),
                            b = p.beta
                        );
                    }
                },
            }
        }
        s += &models;
        s += ".op\n";
        for o in &self.outputs {
            s += &format!(".print op V({})\n", name(*o));
        }
        s += ".end\n";
        s
    }
}

// ---------------------------------------------------------------------------------------------
// Small dense linear algebra (no allocation)

/// out[r] = init[r] + Σ_c a[r · stride + c] · x[c] for r in 0..out.len(), c in 0..x.len(). Each row
/// is summed in index order, exactly as one accumulator would (bit-identical to the plain loop), but
/// four rows run side by side: a dot product is latency-bound, four are not.
#[inline]
fn matvec(a: &[f64], stride: usize, x: &[f64], init: Option<&[f64]>, out: &mut [f64]) {
    let n = x.len();
    let rows = out.len();
    let mut r = 0;
    while r + 8 <= rows {
        let row = |k: usize| &a[(r + k) * stride..(r + k) * stride + n];
        let (r0, r1, r2, r3, r4, r5, r6, r7) = (row(0), row(1), row(2), row(3), row(4), row(5), row(6), row(7));
        let mut s = match init {
            Some(i) => [i[r], i[r + 1], i[r + 2], i[r + 3], i[r + 4], i[r + 5], i[r + 6], i[r + 7]],
            None => [0.0; 8],
        };
        for c in 0..n {
            let xc = x[c];
            s[0] += r0[c] * xc;
            s[1] += r1[c] * xc;
            s[2] += r2[c] * xc;
            s[3] += r3[c] * xc;
            s[4] += r4[c] * xc;
            s[5] += r5[c] * xc;
            s[6] += r6[c] * xc;
            s[7] += r7[c] * xc;
        }
        out[r..r + 8].copy_from_slice(&s);
        r += 8;
    }
    while r + 4 <= rows {
        let (r0, r1, r2, r3) = (&a[r * stride..r * stride + n], &a[(r + 1) * stride..(r + 1) * stride + n], &a[(r + 2) * stride..(r + 2) * stride + n], &a[(r + 3) * stride..(r + 3) * stride + n]);
        let (mut s0, mut s1, mut s2, mut s3) = match init {
            Some(i) => (i[r], i[r + 1], i[r + 2], i[r + 3]),
            None => (0.0, 0.0, 0.0, 0.0),
        };
        for c in 0..n {
            let xc = x[c];
            s0 += r0[c] * xc;
            s1 += r1[c] * xc;
            s2 += r2[c] * xc;
            s3 += r3[c] * xc;
        }
        out[r] = s0;
        out[r + 1] = s1;
        out[r + 2] = s2;
        out[r + 3] = s3;
        r += 4;
    }
    while r < rows {
        let row = &a[r * stride..r * stride + n];
        let mut acc = init.map_or(0.0, |i| i[r]);
        for c in 0..n {
            acc += row[c] * x[c];
        }
        out[r] = acc;
        r += 1;
    }
}

/// Solves a x = b in place (b ← x), partial pivoting. Returns false if singular.
fn lu_solve(a: &mut [f64], b: &mut [f64], n: usize) -> bool {
    match n {
        0 => true,
        1 => {
            if a[0] == 0.0 || !a[0].is_finite() {
                return false;
            }
            b[0] /= a[0];
            true
        }
        2 => {
            let det = a[0] * a[3] - a[1] * a[2];
            if det == 0.0 || !det.is_finite() {
                return false;
            }
            let x0 = (a[3] * b[0] - a[1] * b[1]) / det;
            let x1 = (a[0] * b[1] - a[2] * b[0]) / det;
            b[0] = x0;
            b[1] = x1;
            true
        }
        _ => {
            for col in 0..n {
                let mut piv = col;
                for r in col + 1..n {
                    if a[r * n + col].abs() > a[piv * n + col].abs() {
                        piv = r;
                    }
                }
                if a[piv * n + col] == 0.0 || !a[piv * n + col].is_finite() {
                    return false;
                }
                if piv != col {
                    for c in 0..n {
                        a.swap(col * n + c, piv * n + c);
                    }
                    b.swap(col, piv);
                }
                let d = a[col * n + col];
                let bc = b[col];
                // the pivot row and the rows below as slices: the same operations, without bounds
                // checks in the inner loop
                let (top, below) = a[..n * n].split_at_mut((col + 1) * n);
                let prow = &top[col * n + col..col * n + n];
                for (q, row) in below.chunks_exact_mut(n).enumerate() {
                    let row = &mut row[col..];
                    let f = row[0] / d;
                    if f != 0.0 {
                        for (x, p) in row.iter_mut().zip(prow) {
                            *x -= f * p;
                        }
                        b[col + 1 + q] -= f * bc;
                    }
                }
            }
            for r in (0..n).rev() {
                let row = &a[r * n..r * n + n];
                let mut acc = b[r];
                for c in r + 1..n {
                    acc -= row[c] * b[c];
                }
                b[r] = acc / row[r];
            }
            true
        }
    }
}

/// LU factorisation with partial pivoting, size a constant (the loops unroll), by reciprocal
/// pivots: L below the diagonal (unit), U on and above it, the row swaps in `perm`, 1 / U's diagonal
/// in `inv`. The opt-in fast path ([`NonlinearCircuit::set_fast_solve`]): its rounding differs from
/// [`lu_solve`]'s divisions.
#[inline]
fn lu_factor<const N: usize>(a: &mut [f64], perm: &mut [usize; MAX_PORTS], inv: &mut [f64; MAX_PORTS]) -> bool {
    for col in 0..N {
        let mut piv = col;
        for r in col + 1..N {
            if a[r * N + col].abs() > a[piv * N + col].abs() {
                piv = r;
            }
        }
        if a[piv * N + col] == 0.0 || !a[piv * N + col].is_finite() {
            return false;
        }
        perm[col] = piv;
        if piv != col {
            for c in 0..N {
                a.swap(col * N + c, piv * N + c);
            }
        }
        let id = 1.0 / a[col * N + col];
        inv[col] = id;
        let (top, below) = a[..N * N].split_at_mut((col + 1) * N);
        let prow = &top[col * N + col + 1..col * N + N];
        for row in below.as_chunks_mut::<N>().0 {
            let row = &mut row[col..];
            let f = row[0] * id;
            row[0] = f;
            if f != 0.0 {
                for (x, p) in row[1..].iter_mut().zip(prow) {
                    *x -= f * p;
                }
            }
        }
    }
    true
}

/// Solves with [`lu_factor`]'s factors (b ← x).
#[inline]
fn lu_apply<const N: usize>(a: &[f64], perm: &[usize; MAX_PORTS], inv: &[f64; MAX_PORTS], b: &mut [f64]) {
    // the row swaps moved whole rows (L included): all of them first, then L
    for col in 0..N {
        b.swap(col, perm[col]);
    }
    for col in 0..N {
        let bc = b[col];
        for r in col + 1..N {
            b[r] -= a[r * N + col] * bc;
        }
    }
    for r in (0..N).rev() {
        let row = &a[r * N..r * N + N];
        let mut acc = b[r];
        for c in r + 1..N {
            acc -= row[c] * b[c];
        }
        b[r] = acc * inv[r];
    }
}

/// Gauss-Jordan inverse with partial pivoting into `inv`, `work` = n × 2n.
fn invert(m: &[f64], inv: &mut [f64], work: &mut [f64], n: usize) {
    let w = 2 * n;
    for r in 0..n {
        for c in 0..n {
            work[r * w + c] = m[r * n + c];
            work[r * w + n + c] = if r == c { 1.0 } else { 0.0 };
        }
    }
    for col in 0..n {
        let mut piv = col;
        for r in col + 1..n {
            if work[r * w + col].abs() > work[piv * w + col].abs() {
                piv = r;
            }
        }
        if piv != col {
            for c in 0..w {
                work.swap(col * w + c, piv * w + c);
            }
        }
        let d = work[col * w + col];
        let d = if d.abs() < 1e-300 { 1e-300 } else { d };
        for c in 0..w {
            work[col * w + c] /= d;
        }
        for r in 0..n {
            if r != col {
                let f = work[r * w + col];
                if f != 0.0 {
                    for c in 0..w {
                        work[r * w + c] -= f * work[col * w + c];
                    }
                }
            }
        }
    }
    for r in 0..n {
        for c in 0..n {
            inv[r * n + c] = work[r * w + n + c];
        }
    }
}

// ---------------------------------------------------------------------------------------------
// A stage

/// Where a stage's input slot comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
enum InSrc {
    Ext(usize),
    Stage(usize, usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum RKind {
    C,
    Be,
    L,
}

/// Solver statistics (the bench's Newton check).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    /// Samples solved (per stage, summed).
    pub samples: u64,
    /// Newton iterations (Jacobian solves), fallbacks included.
    pub iterations: u64,
    /// Device exponentials evaluated.
    pub exps: u64,
    /// Samples that needed the continuation fallback.
    pub fallbacks: u64,
    /// Samples left unsolved (the previous solution held).
    pub unsolved: u64,
    /// The worst sample's iteration count.
    pub worst: u32,
}

impl Stats {
    fn add(&mut self, o: &Stats) {
        self.samples += o.samples;
        self.iterations += o.iterations;
        self.exps += o.exps;
        self.fallbacks += o.fallbacks;
        self.unsolved += o.unsolved;
        self.worst = self.worst.max(o.worst);
    }
    /// Mean iterations per stage-sample.
    pub fn mean(&self) -> f64 {
        self.iterations as f64 / self.samples.max(1) as f64
    }
}

#[derive(Clone, Debug)]
struct Stage {
    // structure (local nodes; 0 = ground)
    nn: usize,
    elems: Vec<Element>,
    srcs: Vec<(Node, usize)>, // (local node, input slot)
    in_map: Vec<InSrc>,
    reactive: Vec<(Node, Node, f64, RKind)>, // (p, q, value, kind)
    ports: Vec<[Node; 4]>,                   // (vp, vn, ip, in)
    devs: Vec<(usize, usize, Eval)>,          // (global device, port offset, evaluated)
    vars: Vec<(Node, Node, usize)>,           // (a, b, global var)
    var_g0: Vec<f64>,
    var_g: Vec<f64>,
    obs: Vec<Node>,
    /// The pair ports whose tail follows the common mode: (local device, obs row of +, obs row of −).
    tails: Vec<(usize, usize, usize)>,
    m: usize,
    nr: usize,
    nu: usize,
    no: usize,
    t: f64,
    // matrices
    p0: Vec<f64>,
    p: Vec<f64>,
    lw: Vec<f64>,
    rw: Vec<f64>,
    q: Vec<f64>,
    dirty: bool,
    kbuf: [f64; MAX_PORTS * MAX_PORTS],
    /// The companion conductance of each reactive element.
    rg: Vec<f64>,
    // DC workspace
    n_dc: usize,
    m_dc: Vec<f64>,
    inv_dc: Vec<f64>,
    work_dc: Vec<f64>,
    p_dc: Vec<f64>, // rows: m + nr + no; cols: nu + m
    seed_v: Vec<f64>,
    seed_u: Vec<f64>,
    seeded: bool,
    // state
    j: Vec<f64>,
    x: Vec<f64>,
    jn: Vec<f64>,
    v: [f64; MAX_PORTS],
    v1: [f64; MAX_PORTS],
    v2: [f64; MAX_PORTS],
    i: [f64; MAX_PORTS],
    pp: [f64; MAX_PORTS],
    plain: [bool; 2],
    plain2: bool,
    /// Seed by quadratic extrapolation when the three previous samples were plain Newton.
    quad: bool,
    y: Vec<f64>,
    u: Vec<f64>,
    tol: f64,
    /// Trailing observation rows computed on demand only (probes); whether `x` holds the last step.
    lazy: usize,
    stepped: bool,
    recip: bool,
    stats: Stats,
}

/// The Newton machinery over one system g(v) = p + K f(v) − v.
struct Newton<'a> {
    k: &'a [f64],
    m: usize,
    devs: &'a [(usize, usize, Eval)],
    tol: f64,
    /// The DC solve: no input-stage step limit (the loop gain makes it crawl at DC).
    dc: bool,
    /// No line search: every (limited) step is taken, as SPICE does — the last resort before the
    /// continuation, for stiff exponentials where Armijo crawls.
    plain_steps: bool,
    /// Each port's device block [start, end): Jf's nonzero rows in that column.
    blk: [(usize, usize); MAX_PORTS],
    /// The Jacobian solve multiplies by each pivot's reciprocal instead of dividing (opt-in:
    /// [`NonlinearCircuit::set_fast_solve`]).
    recip: bool,
}

/// The device block of every port.
fn blocks(devs: &[(usize, usize, Eval)]) -> [(usize, usize); MAX_PORTS] {
    let mut b = [(0, 0); MAX_PORTS];
    for &(_, off, ev) in devs {
        let n = ev.ports();
        for k in off..off + n {
            b[k] = (off, off + n);
        }
    }
    b
}

impl Newton<'_> {
    #[inline]
    fn eval(&self, v: &[f64], i: &mut [f64], jf: &mut [f64]) -> u32 {
        let m = self.m;
        for x in jf[..m * m].iter_mut() {
            *x = 0.0;
        }
        let mut e = 0;
        for d in self.devs {
            e += d.2.eval(v, i, jf, m, d.1);
        }
        e as u32
    }

    #[inline]
    fn residual(&self, p: &[f64], v: &[f64], i: &[f64], g: &mut [f64]) -> f64 {
        let m = self.m;
        let mut pv = [0.0; MAX_PORTS];
        for r in 0..m {
            pv[r] = p[r] - v[r];
        }
        matvec(self.k, m, &i[..m], Some(&pv[..m]), &mut g[..m]);
        let mut n2 = 0.0;
        for r in 0..m {
            n2 += g[r] * g[r];
        }
        n2
    }

    /// The residual (V) under which a point is converged: a tenth of the step tolerance.
    #[inline]
    fn rtol(&self) -> f64 {
        self.tol
    }

    /// Damped Newton from `v` (in place). Returns (converged, iterations, exps); `i` holds f(v).
    fn solve(&self, p: &[f64], v: &mut [f64; MAX_PORTS], i: &mut [f64; MAX_PORTS], cap: usize) -> (bool, u32, u32) {
        // workspaces sized to the stage (no 1 kB memset per sample on a 2-port stage)
        // and the port count a constant: every small loop unrolls
        match self.m {
            0 => self.solve_g::<0, 4>(p, v, i, cap),
            1 => self.solve_g::<1, 4>(p, v, i, cap),
            2 => self.solve_g::<2, 4>(p, v, i, cap),
            3 => self.solve_g::<3, 9>(p, v, i, cap),
            4 => self.solve_g::<4, 16>(p, v, i, cap),
            5 => self.solve_g::<5, 25>(p, v, i, cap),
            6 => self.solve_g::<6, 36>(p, v, i, cap),
            7 => self.solve_g::<7, 49>(p, v, i, cap),
            8 => self.solve_g::<8, 64>(p, v, i, cap),
            9 => self.solve_g::<9, 81>(p, v, i, cap),
            10 => self.solve_g::<10, 100>(p, v, i, cap),
            11 => self.solve_g::<11, 121>(p, v, i, cap),
            _ => self.solve_g::<12, 144>(p, v, i, cap),
        }
    }

    fn solve_g<const M: usize, const S: usize>(&self, p: &[f64], v: &mut [f64; MAX_PORTS], i: &mut [f64; MAX_PORTS], cap: usize) -> (bool, u32, u32) {
        debug_assert_eq!(M, self.m);
        let m = M;
        let mut jf = [0.0; S];
        let mut a = [0.0; S];
        let mut g = [0.0; MAX_PORTS];
        let mut d = [0.0; MAX_PORTS];
        let mut vt = [0.0; MAX_PORTS];
        let mut it_ = [0.0; MAX_PORTS];
        let mut jt = [0.0; S];
        let mut gt = [0.0; MAX_PORTS];
        // the fast path (reciprocal pivots; chord steps reusing the factors were tried: on the
        // faces they cost more iterations than the factorisations they save)
        let fast = self.recip && M >= 3;
        let mut perm = [0usize; MAX_PORTS];
        let mut inv = [0.0; MAX_PORTS];
        let mut exps = self.eval(v, i, &mut jf);
        let mut norm = self.residual(p, v, i, &mut g);
        // a residual below the tolerance is converged (|A| ≥ 1 on these ports: the error in v is smaller)
        if g[..m].iter().all(|x| x.abs() < self.rtol()) {
            return (true, 0, exps);
        }
        for iter in 1..=cap as u32 {
            {
                // A = K Jf − I. Jf is block-diagonal (one block per device): only the products of
                // its block are summed, in the same order (the others are exact zeros)
                for r in 0..m {
                    let kr = &self.k[r * m..r * m + m];
                    for c in 0..m {
                        let (b0, b1) = self.blk[c];
                        let mut acc = 0.0;
                        for k in b0..b1 {
                            acc += kr[k] * jf[k * m + c];
                        }
                        a[r * m + c] = acc - if r == c { 1.0 } else { 0.0 };
                    }
                    d[r] = -g[r];
                }
                if fast {
                    if !lu_factor::<M>(&mut a[..m * m], &mut perm, &mut inv) {
                        return (false, iter, exps);
                    }
                    lu_apply::<M>(&a[..m * m], &perm, &inv, &mut d[..m]);
                } else if !lu_solve(&mut a[..m * m], &mut d[..m], m) {
                    return (false, iter, exps);
                }
            }
            let dn = d;
            for r in 0..m {
                vt[r] = v[r] + d[r];
            }
            for dv in self.devs {
                dv.2.limit(&mut vt, v, dv.1, self.dc);
            }
            // the limited step first (SPICE); if ‖g‖² does not fall there, backtrack along the
            // Newton direction itself (a descent direction), from the largest fraction the limits allow
            let mut lambda0: f64 = 1.0;
            for r in 0..m {
                let dl = vt[r] - v[r];
                if dl != dn[r] && dn[r] != 0.0 {
                    lambda0 = lambda0.min((dl / dn[r]).abs());
                }
                d[r] = dl;
            }
            let mut lambda = 1.0;
            let mut accepted = false;
            exps += self.eval(&vt, &mut it_, &mut jt);
            let nt = self.residual(p, &vt, &it_, &mut gt);
            let limited = lambda0 < 1.0 - 1e-12 || self.plain_steps;
            if nt.is_finite() && (nt <= norm * (1.0 - 1e-4) || nt < 1e-24 || limited) {
                accepted = true;
                norm = nt;
            } else {
                d = dn;
                lambda = lambda0.clamp(1.0 / 4096.0, 1.0);
            }
            for _ in 0..12 {
                if accepted {
                    break;
                }
                for r in 0..m {
                    vt[r] = v[r] + lambda * d[r];
                }
                exps += self.eval(&vt, &mut it_, &mut jt);
                let nt = self.residual(p, &vt, &it_, &mut gt);
                if nt.is_finite() && (nt <= norm * (1.0 - 1e-4 * lambda) || nt < 1e-24) {
                    accepted = true;
                    norm = nt;
                    break;
                }
                lambda *= 0.5;
            }
            if !accepted {
                // no descent along the limited step: take the last (smallest) trial if finite
                let nt = self.residual(p, &vt, &it_, &mut gt);
                if !nt.is_finite() {
                    return (false, iter, exps);
                }
                norm = nt;
            }
            let (mut step, mut full): (f64, f64) = (0.0, 0.0);
            for r in 0..m {
                step = step.max((lambda * d[r]).abs());
                full = full.max(d[r].abs());
                v[r] = vt[r];
                i[r] = it_[r];
                g[r] = gt[r];
            }
            jf[..m * m].copy_from_slice(&jt[..m * m]);
            // converged: the step is below the tolerance (a full Newton step that small is converged
            // even when roundoff keeps ‖g‖² from falling)
            if (step < self.tol && accepted) || full < self.tol || (accepted && gt[..m].iter().all(|x| x.abs() < self.rtol())) {
                return (true, iter, exps);
            }
        }
        (false, cap as u32, exps)
    }
}

impl Stage {
    /// SPICE-like starting point of every port: op-amps at mid-rail with no input difference,
    /// junctions at their critical voltage, clippers off.
    fn initial_guess(&self, v: &mut [f64; MAX_PORTS]) {
        for &(_, off, ev) in &self.devs {
            match ev {
                Eval::Clipper { .. } => v[off] = 0.0,
                Eval::Bjt { s, vcrit, .. } => {
                    v[off] = s * vcrit;
                    v[off + 1] = 0.0;
                }
                Eval::OpAmp { hi, lo, .. } => {
                    v[off] = 0.0;
                    v[off + 1] = 0.5 * (hi + lo);
                }
                Eval::Jfet { s, .. } => {
                    // SPICE's MODEINITJCT point: the channel on, saturated
                    v[off] = 0.0;
                    v[off + 1] = -s;
                }
                Eval::Pair { .. } => v[off] = 0.0,
            }
        }
    }

    fn ncol(&self) -> usize {
        self.nr + self.nu + self.m
    }

    fn nrow(&self) -> usize {
        self.m + self.nr + self.no
    }

    /// Unknown count (transient): nodes + sources + op-amp / buffer branches.
    fn branches(&self) -> usize {
        self.srcs.len() + self.elems.iter().filter(|e| matches!(e, Element::OpAmp(..) | Element::Buffer(..))).count()
    }

    /// Stamps the linear MNA matrix. `dc`: caps open, inductors as 0 V branches, gmin; vars at
    /// `var_g` (dc) or `var_g0` (transient nominal).
    fn assemble(&self, dc: bool, m: &mut [f64], n: usize) {
        for x in m[..n * n].iter_mut() {
            *x = 0.0;
        }
        let stamp = |a: Node, b: Node, g: f64, m: &mut [f64]| {
            if a > 0 {
                m[(a - 1) * n + a - 1] += g;
            }
            if b > 0 {
                m[(b - 1) * n + b - 1] += g;
            }
            if a > 0 && b > 0 {
                m[(a - 1) * n + b - 1] -= g;
                m[(b - 1) * n + a - 1] -= g;
            }
        };
        for e in &self.elems {
            if let Element::R(a, b, r) = *e {
                stamp(a, b, 1.0 / r, m);
            }
        }
        for (k, &(a, b, _)) in self.vars.iter().enumerate() {
            stamp(a, b, if dc { self.var_g[k] } else { self.var_g0[k] }, m);
        }
        // gmin on every node, in the DC solve and the transient alike (the same circuit at rest)
        for nd in 1..=self.nn {
            m[(nd - 1) * n + nd - 1] += GMIN;
        }
        if !dc {
            for &(a, b, val, kind) in &self.reactive {
                stamp(a, b, Self::companion_g(val, kind, self.t), m);
            }
        }
        // branches
        let mut br = self.nn;
        let put = |r: usize, c: usize, v: f64, m: &mut [f64]| m[r * n + c] += v;
        for &(nd, _) in &self.srcs {
            // V(nd) = u; the source's current enters KCL at nd
            put(nd - 1, br, 1.0, m);
            put(br, nd - 1, 1.0, m);
            br += 1;
        }
        for e in &self.elems {
            match *e {
                Element::OpAmp(p, mi, o) => {
                    put(o - 1, br, 1.0, m);
                    if p > 0 {
                        put(br, p - 1, 1.0, m);
                    }
                    if mi > 0 {
                        put(br, mi - 1, -1.0, m);
                    }
                    br += 1;
                }
                Element::Buffer(c, o) => {
                    put(o - 1, br, 1.0, m);
                    put(br, o - 1, 1.0, m);
                    if c > 0 {
                        put(br, c - 1, -1.0, m);
                    }
                    br += 1;
                }
                _ => {}
            }
        }
        if dc {
            for &(a, b, _, kind) in &self.reactive {
                if kind == RKind::L {
                    // a 0 V source a → b: its current leaves a, enters b
                    if a > 0 {
                        put(a - 1, br, 1.0, m);
                        put(br, a - 1, 1.0, m);
                    }
                    if b > 0 {
                        put(b - 1, br, -1.0, m);
                        put(br, b - 1, -1.0, m);
                    }
                    br += 1;
                }
            }
        }
    }

    fn companion_g(val: f64, kind: RKind, t: f64) -> f64 {
        match kind {
            RKind::C => 2.0 * val / t,
            RKind::Be => val / t,
            RKind::L => t / (2.0 * val),
        }
    }

    /// Sparse observation rows: ports, reactive voltages (dc: cap voltages, inductor currents),
    /// outputs.
    fn obs_rows(&self, dc: bool) -> Vec<Vec<(usize, f64)>> {
        let mut rows = Vec::new();
        let node = |n: Node, s: f64, v: &mut Vec<(usize, f64)>| {
            if n > 0 {
                v.push((n - 1, s));
            }
        };
        for pt in &self.ports {
            let mut r = Vec::new();
            node(pt[0], 1.0, &mut r);
            node(pt[1], -1.0, &mut r);
            rows.push(r);
        }
        let mut lbr = self.nn + self.branches();
        for &(a, b, _, kind) in &self.reactive {
            let mut r = Vec::new();
            if dc && kind == RKind::L {
                r.push((lbr, 1.0));
                lbr += 1;
            } else {
                node(a, 1.0, &mut r);
                node(b, -1.0, &mut r);
            }
            rows.push(r);
        }
        for &o in &self.obs {
            let mut r = Vec::new();
            node(o, 1.0, &mut r);
            rows.push(r);
        }
        rows
    }

    /// Sparse injection columns: reactive histories (transient only), input slots, ports.
    fn inj_cols(&self, dc: bool) -> Vec<Vec<(usize, f64)>> {
        let mut cols = Vec::new();
        let node = |n: Node, s: f64, v: &mut Vec<(usize, f64)>| {
            if n > 0 {
                v.push((n - 1, s));
            }
        };
        if !dc {
            for &(a, b, _, kind) in &self.reactive {
                let mut c = Vec::new();
                let s = if kind == RKind::L { -1.0 } else { 1.0 };
                node(a, s, &mut c);
                node(b, -s, &mut c);
                cols.push(c);
            }
        }
        for slot in 0..self.nu {
            let mut c = Vec::new();
            for (k, &(_, sl)) in self.srcs.iter().enumerate() {
                if sl == slot {
                    c.push((self.nn + k, 1.0));
                }
            }
            for e in &self.elems {
                if let Element::ISrc(a, b, sl) = *e {
                    if sl == slot {
                        node(a, -1.0, &mut c);
                        node(b, 1.0, &mut c);
                    }
                }
            }
            cols.push(c);
        }
        for pt in &self.ports {
            let mut c = Vec::new();
            node(pt[2], -1.0, &mut c);
            node(pt[3], 1.0, &mut c);
            cols.push(c);
        }
        cols
    }

    fn sandwich(inv: &[f64], n: usize, rows: &[Vec<(usize, f64)>], cols: &[Vec<(usize, f64)>], out: &mut [f64]) {
        let nc = cols.len();
        for (r, row) in rows.iter().enumerate() {
            for (c, col) in cols.iter().enumerate() {
                let mut acc = 0.0;
                for &(a, wa) in row {
                    for &(b, zb) in col {
                        acc += wa * zb * inv[a * n + b];
                    }
                }
                out[r * nc + c] = acc;
            }
        }
    }

    /// Builds the transient matrices (P₀, Woodbury factors) at the sample period `t`.
    fn build(&mut self) {
        let n = self.nn + self.branches();
        let mut m = vec![0.0; n * n];
        let mut inv = vec![0.0; n * n];
        let mut work = vec![0.0; n * 2 * n];
        self.assemble(false, &mut m, n);
        invert(&m, &mut inv, &mut work, n);
        let rows = self.obs_rows(false);
        let cols = self.inj_cols(false);
        let (nrow, ncol) = (rows.len(), cols.len());
        self.p0 = vec![0.0; nrow * ncol];
        Self::sandwich(&inv, n, &rows, &cols, &mut self.p0);
        let e: Vec<Vec<(usize, f64)>> = self
            .vars
            .iter()
            .map(|&(a, b, _)| {
                let mut c = Vec::new();
                if a > 0 {
                    c.push((a - 1, 1.0));
                }
                if b > 0 {
                    c.push((b - 1, -1.0));
                }
                c
            })
            .collect();
        let k = e.len();
        self.lw = vec![0.0; nrow * k];
        Self::sandwich(&inv, n, &rows, &e, &mut self.lw);
        self.rw = vec![0.0; k * ncol];
        Self::sandwich(&inv, n, &e, &cols, &mut self.rw);
        self.q = vec![0.0; k * k];
        Self::sandwich(&inv, n, &e, &e, &mut self.q);
        self.p = self.p0.clone();
        self.rg = self.reactive.iter().map(|&(_, _, val, kind)| Self::companion_g(val, kind, self.t)).collect();
        self.dirty = true;
    }

    /// P = P₀ − L (I + ΔG Q)⁻¹ ΔG R, then K (P's port block) copied out for Newton.
    fn woodbury(&mut self) {
        self.woodbury_p();
        let mut k = [0.0; MAX_PORTS * MAX_PORTS];
        self.kmat(&mut k);
        self.kbuf = k;
    }

    fn woodbury_p(&mut self) {
        self.dirty = false;
        let k = self.vars.len();
        let (nrow, ncol) = (self.nrow(), self.ncol());
        if k == 0 {
            return;
        }
        let mut a = [0.0; MAX_VARS * MAX_VARS];
        let mut s = [0.0; MAX_VARS * MAX_VARS];
        let mut col = [0.0; MAX_VARS];
        assert!(k <= MAX_VARS, "at most {MAX_VARS} variable resistors per stage");
        for r in 0..k {
            let dg = self.var_g[r] - self.var_g0[r];
            for c in 0..k {
                a[r * k + c] = if r == c { 1.0 } else { 0.0 } + dg * self.q[r * k + c];
            }
        }
        // S = A⁻¹ ΔG, column by column
        for c in 0..k {
            let mut aa = a;
            for r in 0..k {
                col[r] = if r == c { self.var_g[c] - self.var_g0[c] } else { 0.0 };
            }
            lu_solve(&mut aa[..k * k], &mut col[..k], k);
            for r in 0..k {
                s[r * k + c] = col[r];
            }
        }
        // T = S R (k × ncol) into jn-sized scratch: use a stack buffer per column
        for c in 0..ncol {
            let mut t = [0.0; MAX_VARS];
            for r in 0..k {
                let mut acc = 0.0;
                for q in 0..k {
                    acc += s[r * k + q] * self.rw[q * ncol + c];
                }
                t[r] = acc;
            }
            for row in 0..nrow {
                let mut acc = self.p0[row * ncol + c];
                for q in 0..k {
                    acc -= self.lw[row * k + q] * t[q];
                }
                self.p[row * ncol + c] = acc;
            }
        }
    }

    fn kmat(&self, out: &mut [f64; MAX_PORTS * MAX_PORTS]) {
        let (m, ncol, off) = (self.m, self.ncol(), self.nr + self.nu);
        for r in 0..m {
            for c in 0..m {
                out[r * m + c] = self.p[r * ncol + off + c];
            }
        }
    }

    /// One sample with input slots `u`.
    fn step(&mut self) {
        if self.dirty {
            self.woodbury();
        }
        let (m, nr, nu, ncol) = (self.m, self.nr, self.nu, self.ncol());
        self.x[..nr].copy_from_slice(&self.j);
        self.x[nr..nr + nu].copy_from_slice(&self.u);
        let mut p = [0.0; MAX_PORTS];
        matvec(&self.p, ncol, &self.x[..nr + nu], None, &mut p[..m]);
        let mut held = false;
        if m > 0 {
            let nw = Newton { k: &self.kbuf[..m * m], m, devs: &self.devs, tol: self.tol, dc: false, plain_steps: false, blk: blocks(&self.devs), recip: self.recip };
            let mut v = self.v;
            if self.quad && self.plain[0] && self.plain[1] && self.plain2 {
                for r in 0..m {
                    v[r] = 3.0 * (self.v[r] - self.v1[r]) + self.v2[r];
                }
            } else if self.plain[0] && self.plain[1] {
                for r in 0..m {
                    v[r] = 2.0 * self.v[r] - self.v1[r];
                }
            }
            let mut i = [0.0; MAX_PORTS];
            let (ok, it, ex) = nw.solve(&p, &mut v, &mut i, NEWTON_CAP);
            let (mut its, mut exps) = (it, ex);
            let mut plain = ok;
            let mut solved = ok;
            if !ok && self.plain[0] && self.plain[1] {
                // the extrapolated seed failed: plain Newton from the previous solution first
                v = self.v;
                let (ok2, it2, ex2) = nw.solve(&p, &mut v, &mut i, NEWTON_CAP);
                its += it2;
                exps += ex2;
                solved = ok2;
            }
            if !solved {
                // then without the line search (SPICE's way: the limits are the damping)
                let sp = Newton { plain_steps: true, ..nw };
                v = self.v;
                let (ok2, it2, ex2) = sp.solve(&p, &mut v, &mut i, NEWTON_CAP);
                its += it2;
                exps += ex2;
                solved = ok2;
                if solved {
                    self.stats.fallbacks += 1;
                }
            }
            if !solved {
                self.stats.fallbacks += 1;
                // continuation in p from the previous sample's (p, v)
                for steps in [4usize, 16] {
                    let mut vc = self.v;
                    let mut good = true;
                    for s in 1..=steps {
                        let a = s as f64 / steps as f64;
                        let mut pa = [0.0; MAX_PORTS];
                        for r in 0..m {
                            pa[r] = self.pp[r] + a * (p[r] - self.pp[r]);
                        }
                        let start = vc;
                        let (mut ok2, it2, ex2) = nw.solve(&pa, &mut vc, &mut i, NEWTON_CAP);
                        its += it2;
                        exps += ex2;
                        if !ok2 {
                            // the same continuation step without the line search
                            vc = start;
                            let sp = Newton { plain_steps: true, ..nw };
                            let (ok3, it3, ex3) = sp.solve(&pa, &mut vc, &mut i, NEWTON_CAP);
                            its += it3;
                            exps += ex3;
                            ok2 = ok3;
                        }
                        if !ok2 {
                            good = false;
                            break;
                        }
                    }
                    if good {
                        v = vc;
                        solved = true;
                        break;
                    }
                }
                plain = false;
                if !solved {
                    held = true;
                    self.stats.unsolved += 1;
                    v = self.v;
                    // and its currents: f at the old point may be another device by now (a swap)
                    i = self.i;
                }
            }
            self.stats.iterations += its as u64;
            self.stats.exps += exps as u64;
            self.stats.worst = self.stats.worst.max(its);
            self.v2 = self.v1;
            self.v1 = self.v;
            self.v = v;
            self.i = i;
            self.pp = p;
            self.plain2 = self.plain[1];
            self.plain = [plain, self.plain[0]];
            self.x[nr + nu..nr + nu + m].copy_from_slice(&i[..m]);
        }
        self.stats.samples += 1;
        // reactive voltages → new histories; outputs
        matvec(&self.p[m * ncol..], ncol, &self.x, None, &mut self.jn);
        for r in 0..nr {
            let acc = self.jn[r];
            let kind = self.reactive[r].3;
            let g = self.rg[r];
            let j = self.j[r];
            let jn = match kind {
                RKind::C => 2.0 * g * acc - j,
                RKind::Be => g * acc,
                RKind::L => 2.0 * g * acc + j,
            };
            self.jn[r] = if jn.abs() < 1e-200 { 0.0 } else { jn };
        }
        let now = self.no - self.lazy;
        matvec(&self.p[(m + nr) * ncol..], ncol, &self.x, None, &mut self.y[..now]);
        // the pairs' tails follow the common mode one sample late; a held sample keeps the old latch
        if !held {
            for q in 0..self.tails.len() {
                let (d, a, b) = self.tails[q];
                let vcm = 0.5 * (self.y[a] + self.y[b]);
                self.devs[d].2.latch(vcm);
            }
        }
        self.stepped = true;
        std::mem::swap(&mut self.j, &mut self.jn);
    }

    /// The DC operating point for input slots `u`; sets the states. Returns whether it converged.
    fn solve_dc(&mut self, init: bool) -> bool {
        let n = self.n_dc;
        let mut mm = std::mem::take(&mut self.m_dc);
        let mut inv = std::mem::take(&mut self.inv_dc);
        let mut work = std::mem::take(&mut self.work_dc);
        self.assemble(true, &mut mm, n);
        invert(&mm, &mut inv, &mut work, n);
        self.m_dc = mm;
        self.work_dc = work;
        // P_dc = W M⁻¹ Z (rows: ports, reactive, outputs; cols: slots, ports), no allocation: the
        // sparse rows / columns are walked directly
        let (m, nr, no, nu) = (self.m, self.nr, self.no, self.nu);
        let ncd = nu + m;
        let at = |a: Node| if a > 0 { Some(a - 1) } else { None };
        let mut lbr = self.nn + self.branches();
        for row in 0..m + nr + no {
            // a row is (index, sign) pairs
            let mut rr = [(usize::MAX, 0.0); 2];
            if row < m {
                let pt = self.ports[row];
                rr = [(at(pt[0]).unwrap_or(usize::MAX), 1.0), (at(pt[1]).unwrap_or(usize::MAX), -1.0)];
            } else if row < m + nr {
                let (a, b, _, kind) = self.reactive[row - m];
                if kind == RKind::L {
                    rr = [(lbr, 1.0), (usize::MAX, 0.0)];
                    lbr += 1;
                } else {
                    rr = [(at(a).unwrap_or(usize::MAX), 1.0), (at(b).unwrap_or(usize::MAX), -1.0)];
                }
            } else {
                rr[0] = (at(self.obs[row - m - nr]).unwrap_or(usize::MAX), 1.0);
            }
            for c in 0..ncd {
                let mut acc = 0.0;
                let mut add = |b: usize, z: f64| {
                    for &(a, w) in &rr {
                        if a != usize::MAX {
                            acc += w * z * inv[a * n + b];
                        }
                    }
                };
                if c < nu {
                    for (k, &(_, sl)) in self.srcs.iter().enumerate() {
                        if sl == c {
                            add(self.nn + k, 1.0);
                        }
                    }
                    for e in &self.elems {
                        if let Element::ISrc(a, b, sl) = *e {
                            if sl == c {
                                if a > 0 {
                                    add(a - 1, -1.0);
                                }
                                if b > 0 {
                                    add(b - 1, 1.0);
                                }
                            }
                        }
                    }
                } else {
                    let pt = self.ports[c - nu];
                    if pt[2] > 0 {
                        add(pt[2] - 1, -1.0);
                    }
                    if pt[3] > 0 {
                        add(pt[3] - 1, 1.0);
                    }
                }
                self.p_dc[row * ncd + c] = acc;
            }
        }
        self.inv_dc = inv;
        let mut k = [0.0; MAX_PORTS * MAX_PORTS];
        for r in 0..m {
            for c in 0..m {
                k[r * m + c] = self.p_dc[r * ncd + nu + c];
            }
        }
        let nw = Newton { k: &k[..m * m], m, devs: &self.devs, tol: self.tol.min(1e-9), dc: true, plain_steps: false, blk: blocks(&self.devs), recip: self.recip };
        let pdc = |u: &[f64], p: &mut [f64; MAX_PORTS], pdc: &[f64]| {
            for r in 0..m {
                let mut acc = 0.0;
                for c in 0..nu {
                    acc += pdc[r * ncd + c] * u[c];
                }
                p[r] = acc;
            }
        };
        let mut v = [0.0; MAX_PORTS];
        let mut i = [0.0; MAX_PORTS];
        let mut p = [0.0; MAX_PORTS];
        let mut ok = m == 0;
        let u: [f64; 16] = {
            let mut a = [0.0; 16];
            a[..nu].copy_from_slice(&self.u);
            a
        };
        if !ok && self.seeded {
            v[..m].copy_from_slice(&self.seed_v[..m]);
            pdc(&u[..nu], &mut p, &self.p_dc);
            ok = nw.solve(&p, &mut v, &mut i, 50).0;
            if !ok {
                // continuation from the seed's inputs
                for steps in [4usize, 16, 64] {
                    v[..m].copy_from_slice(&self.seed_v[..m]);
                    ok = true;
                    for s in 1..=steps {
                        let a = s as f64 / steps as f64;
                        let mut ua = [0.0; 16];
                        for c in 0..nu {
                            ua[c] = self.seed_u[c] + a * (u[c] - self.seed_u[c]);
                        }
                        pdc(&ua[..nu], &mut p, &self.p_dc);
                        if !nw.solve(&p, &mut v, &mut i, 50).0 {
                            ok = false;
                            break;
                        }
                    }
                    if ok {
                        break;
                    }
                }
            }
        }
        if !ok {
            // full sources from the devices' own starting points (op-amps at mid-rail, junctions at
            // their critical voltage)
            self.initial_guess(&mut v);
            pdc(&u[..nu], &mut p, &self.p_dc);
            ok = nw.solve(&p, &mut v, &mut i, 100).0;
        }
        if !ok {
            // source stepping from zero
            for steps in [20usize, 100] {
                v = [0.0; MAX_PORTS];
                self.initial_guess(&mut v);
                ok = true;
                for s in 1..=steps {
                    let a = s as f64 / steps as f64;
                    let mut ua = [0.0; 16];
                    for c in 0..nu {
                        ua[c] = a * u[c];
                    }
                    pdc(&ua[..nu], &mut p, &self.p_dc);
                    if !nw.solve(&p, &mut v, &mut i, 50).0 {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    break;
                }
            }
        }
        // the pairs' tails solved implicitly: a fixed point on the common mode (gfx-dist §2.11)
        if ok && !self.tails.is_empty() {
            pdc(&u[..nu], &mut p, &self.p_dc);
            let mut last = [f64::NAN; MAX_PORTS];
            for _ in 0..200 {
                let mut moved: f64 = 0.0;
                for q in 0..self.tails.len() {
                    let (d, a, b) = self.tails[q];
                    let obs = |row: usize| {
                        let row = m + nr + row;
                        let mut acc = 0.0;
                        for c in 0..nu {
                            acc += self.p_dc[row * ncd + c] * u[c];
                        }
                        for c in 0..m {
                            acc += self.p_dc[row * ncd + nu + c] * i[c];
                        }
                        acc
                    };
                    let vcm = 0.5 * (obs(a) + obs(b));
                    moved = moved.max((vcm - last[q]).abs());
                    last[q] = vcm;
                    self.devs[d].2.latch(vcm);
                }
                if moved < 1e-9 {
                    break;
                }
                let nw = Newton { k: &k[..m * m], m, devs: &self.devs, tol: self.tol.min(1e-9), dc: true, plain_steps: false, blk: blocks(&self.devs), recip: self.recip };
                if !nw.solve(&p, &mut v, &mut i, 50).0 {
                    ok = false;
                    break;
                }
            }
        }
        if m > 0 {
            let nw = Newton { k: &k[..m * m], m, devs: &self.devs, tol: self.tol.min(1e-9), dc: true, plain_steps: false, blk: blocks(&self.devs), recip: self.recip };
            let mut jf = [0.0; MAX_PORTS * MAX_PORTS];
            nw.eval(&v, &mut i, &mut jf);
        }
        if init && ok {
            self.seed_v[..m].copy_from_slice(&v[..m]);
            self.seed_u.copy_from_slice(&self.u);
            self.seeded = true;
        }
        // states and outputs from the DC solution
        for r in 0..nr + no {
            let row = m + r;
            let mut acc = 0.0;
            for c in 0..nu {
                acc += self.p_dc[row * ncd + c] * u[c];
            }
            for c in 0..m {
                acc += self.p_dc[row * ncd + nu + c] * i[c];
            }
            if r < nr {
                let (_, _, val, kind) = self.reactive[r];
                let g = Self::companion_g(val, kind, self.t);
                // trapezoidal / BE companion current zero at rest: J = G v; inductor: J = i
                self.j[r] = if kind == RKind::L { acc } else { g * acc };
            } else {
                self.y[r - nr] = acc;
            }
        }
        self.stepped = false;
        self.v = v;
        self.v1 = v;
        self.v2 = v;
        self.plain2 = true;
        self.i = i;
        self.plain = [true, true];
        // p of the transient system at rest (continuation's start)
        if self.dirty {
            self.woodbury();
        }
        let ncol = self.ncol();
        for r in 0..m {
            let mut acc = 0.0;
            for c in 0..nr {
                acc += self.p[r * ncol + c] * self.j[c];
            }
            for c in 0..nu {
                acc += self.p[r * ncol + nr + c] * u[c];
            }
            self.pp[r] = acc;
        }
        ok
    }

    /// Complex small-signal solve at `freq` with slot amplitudes `u`; returns the observed outputs.
    fn ac(&self, freq: f64, u: &[(f64, f64)]) -> Vec<(f64, f64)> {
        let n = self.nn + self.branches();
        let w = 2.0 * PI * freq;
        let mut a = vec![(0.0, 0.0); n * n];
        let mut b = vec![(0.0, 0.0); n];
        let add = |a: &mut Vec<(f64, f64)>, r: usize, c: usize, v: (f64, f64)| {
            a[r * n + c].0 += v.0;
            a[r * n + c].1 += v.1;
        };
        let stamp = |a: &mut Vec<(f64, f64)>, p: Node, q: Node, y: (f64, f64)| {
            if p > 0 {
                add(a, p - 1, p - 1, y);
            }
            if q > 0 {
                add(a, q - 1, q - 1, y);
            }
            if p > 0 && q > 0 {
                add(a, p - 1, q - 1, (-y.0, -y.1));
                add(a, q - 1, p - 1, (-y.0, -y.1));
            }
        };
        let mut real = vec![0.0; n * n];
        self.assemble(false, &mut real, n);
        // assemble() put the transient companions and the nominal vars in: take R and branches
        // from it by re-stamping without them
        for x in real.iter_mut() {
            *x = 0.0;
        }
        {
            // R + branches only
            let mut tmp = self.clone();
            tmp.reactive.clear();
            tmp.vars.clear();
            tmp.assemble(false, &mut real, n);
        }
        for r in 0..n {
            for c in 0..n {
                a[r * n + c].0 += real[r * n + c];
            }
        }
        for (k, &(p, q, _)) in self.vars.iter().enumerate() {
            stamp(&mut a, p, q, (self.var_g[k], 0.0));
        }
        for &(p, q, val, kind) in &self.reactive {
            let y = match kind {
                RKind::C | RKind::Be => (0.0, w * val),
                RKind::L => (0.0, -1.0 / (w * val)),
            };
            stamp(&mut a, p, q, y);
        }
        // devices linearized at the operating point
        let m = self.m;
        if m > 0 {
            let mut jf = [0.0; MAX_PORTS * MAX_PORTS];
            let mut i = [0.0; MAX_PORTS];
            for d in &self.devs {
                d.2.eval(&self.v, &mut i, &mut jf, m, d.1);
            }
            for k in 0..m {
                for l in 0..m {
                    let g = jf[k * m + l];
                    if g == 0.0 {
                        continue;
                    }
                    let (ip, inn) = (self.ports[k][2], self.ports[k][3]);
                    let (vp, vn) = (self.ports[l][0], self.ports[l][1]);
                    for (row, s) in [(ip, 1.0), (inn, -1.0)] {
                        if row == 0 {
                            continue;
                        }
                        if vp > 0 {
                            add(&mut a, row - 1, vp - 1, (s * g, 0.0));
                        }
                        if vn > 0 {
                            add(&mut a, row - 1, vn - 1, (-s * g, 0.0));
                        }
                    }
                }
            }
            // a pair's tail follows the common mode: its share of the drain current, linearized
            for &(d, ya, yb) in &self.tails {
                let off = self.devs[d].1;
                let gcm = self.devs[d].2.pair_gcm(self.v[off], 0.5 * (self.y[ya] + self.y[yb]));
                let pt = self.ports[off];
                if gcm != 0.0 && pt[2] > 0 {
                    for nd in [pt[0], pt[1]] {
                        if nd > 0 {
                            add(&mut a, pt[2] - 1, nd - 1, (0.5 * gcm, 0.0));
                        }
                    }
                }
            }
        }
        for (k, &(_, sl)) in self.srcs.iter().enumerate() {
            b[self.nn + k] = u[sl];
        }
        for e in &self.elems {
            if let Element::ISrc(p, q, sl) = *e {
                if p > 0 {
                    b[p - 1].0 -= u[sl].0;
                    b[p - 1].1 -= u[sl].1;
                }
                if q > 0 {
                    b[q - 1].0 += u[sl].0;
                    b[q - 1].1 += u[sl].1;
                }
            }
        }
        let x = solve_complex(&mut a, &mut b, n);
        self.obs.iter().map(|&o| if o == 0 { (0.0, 0.0) } else { x[o - 1] }).collect()
    }
}

fn solve_complex(a: &mut [(f64, f64)], b: &mut [(f64, f64)], n: usize) -> Vec<(f64, f64)> {
    let mul = |x: (f64, f64), y: (f64, f64)| (x.0 * y.0 - x.1 * y.1, x.0 * y.1 + x.1 * y.0);
    let div = |x: (f64, f64), y: (f64, f64)| {
        let d = y.0 * y.0 + y.1 * y.1;
        ((x.0 * y.0 + x.1 * y.1) / d, (x.1 * y.0 - x.0 * y.1) / d)
    };
    let mag = |x: (f64, f64)| x.0.hypot(x.1);
    for col in 0..n {
        let mut piv = col;
        for r in col + 1..n {
            if mag(a[r * n + col]) > mag(a[piv * n + col]) {
                piv = r;
            }
        }
        if piv != col {
            for c in 0..n {
                a.swap(col * n + c, piv * n + c);
            }
            b.swap(col, piv);
        }
        let d = a[col * n + col];
        for r in col + 1..n {
            let f = div(a[r * n + col], d);
            for c in col..n {
                let t = mul(f, a[col * n + c]);
                a[r * n + c].0 -= t.0;
                a[r * n + c].1 -= t.1;
            }
            let t = mul(f, b[col]);
            b[r].0 -= t.0;
            b[r].1 -= t.1;
        }
    }
    let mut x = vec![(0.0, 0.0); n];
    for r in (0..n).rev() {
        let mut acc = b[r];
        for c in r + 1..n {
            let t = mul(a[r * n + c], x[c]);
            acc.0 -= t.0;
            acc.1 -= t.1;
        }
        x[r] = div(acc, a[r * n + r]);
    }
    x
}

// ---------------------------------------------------------------------------------------------
// The circuit: a graph of stages

/// A re-evaluated pair keeps its latched tail (it is state, not a constant).
fn keep_latch(old: Eval, new: Eval) -> Eval {
    match (old, new) {
        (Eval::Pair { it, .. }, Eval::Pair { model, .. }) if model.tail_r > 0.0 => pair_eval(model, it),
        _ => new,
    }
}

/// A netlist compiled at one sample rate: a directed graph of stages, solved in order.
#[derive(Clone, Debug)]
pub struct NonlinearCircuit {
    stages: Vec<Stage>,
    out_map: Vec<(usize, usize)>,
    var_map: Vec<(usize, usize)>,
    dev_map: Vec<(usize, usize)>,
    dev_models: Vec<DeviceModel>,
    dev_rails: Vec<(f64, f64)>,
    pots: Vec<Pot>,
    inputs: Vec<f64>,
    celsius: f64,
    supply: f64,
    sample_rate: f64,
}

impl NonlinearCircuit {
    /// Compiles `net` with its stage cuts at `sample_rate`.
    pub fn new(net: &Netlist, sample_rate: f64) -> Self {
        Self::compile(net, sample_rate, false)
    }

    /// Compiles `net` as one stage (cuts ignored): the reference the cut graph is checked against.
    pub fn monolithic(net: &Netlist, sample_rate: f64) -> Self {
        Self::compile(net, sample_rate, true)
    }

    fn compile(net: &Netlist, sample_rate: f64, mono: bool) -> Self {
        let ns = if mono { 1 } else { net.stage + 1 };
        let stage_of = |e: usize| if mono { 0 } else { net.stage_of[e] };
        let nodes_of = |e: &Element| -> Vec<Node> {
            match *e {
                Element::R(a, b, _)
                | Element::C(a, b, _)
                | Element::CBe(a, b, _)
                | Element::L(a, b, _)
                | Element::Var(a, b, _)
                | Element::ISrc(a, b, _)
                | Element::Buffer(a, b) => vec![a, b],
                Element::OpAmp(a, b, c) => vec![a, b, c],
                Element::Device(_, n) => n.to_vec(),
            }
        };
        // owners
        let mut owner = vec![usize::MAX; net.nodes];
        for (k, e) in net.elements.iter().enumerate() {
            for nd in nodes_of(e) {
                if nd != 0 && net.source_of[nd].is_none() && owner[nd] == usize::MAX {
                    owner[nd] = stage_of(k);
                }
            }
        }
        // imports (stage, node) and exports per stage
        let mut exports: Vec<Vec<Node>> = vec![Vec::new(); ns];
        for (k, e) in net.elements.iter().enumerate() {
            let s = stage_of(k);
            for nd in nodes_of(e) {
                if nd != 0 && net.source_of[nd].is_none() && owner[nd] != s {
                    assert!(
                        net.cut[nd] && owner[nd] < s,
                        "node {nd} ({}) is used by stage {s} but owned by stage {} and not a cut",
                        net.names.iter().find(|x| x.0 == nd).map_or("unnamed", |x| x.1),
                        owner[nd]
                    );
                    if !exports[owner[nd]].contains(&nd) {
                        exports[owner[nd]].push(nd);
                    }
                }
            }
        }
        let mut out_map = Vec::new();
        let mut stage_obs: Vec<Vec<Node>> = exports.clone();
        // the pair ports' input nodes, read after every sample (their tails' common mode)
        let mut tail_obs: Vec<Vec<(usize, usize, usize)>> = vec![Vec::new(); ns];
        for (k, e) in net.elements.iter().enumerate() {
            if let Element::Device(d, n) = *e {
                if let DeviceModel::Pair(_) = net.devices[d] {
                    let s = stage_of(k);
                    stage_obs[s].push(n[0]);
                    stage_obs[s].push(n[1]);
                    let a = stage_obs[s].len() - 2;
                    tail_obs[s].push((d, a, a + 1));
                }
            }
        }
        out_map.resize(net.outputs.len(), (0, 0));
        let mut lazy = vec![0usize; ns];
        // the outputs computed every sample first, the probes last (rows the step leaves out)
        for pass in [false, true] {
            for (k, &o) in net.outputs.iter().enumerate() {
                if net.lazy[k] != pass {
                    continue;
                }
                let s = if net.source_of[o].is_some() { 0 } else { owner[o] };
                assert!(s != usize::MAX, "output node {o} is not connected");
                stage_obs[s].push(o);
                out_map[k] = (s, stage_obs[s].len() - 1);
                lazy[s] += pass as usize;
            }
        }
        let mut var_map = vec![(0, 0); net.vars.len()];
        let mut dev_map = vec![(0, 0); net.devices.len()];
        let mut stages = Vec::new();
        let t = 1.0 / sample_rate;
        for s in 0..ns {
            let mut local = vec![0usize; net.nodes];
            let mut nn = 0;
            let mut srcs: Vec<(Node, usize)> = Vec::new();
            let mut in_map: Vec<InSrc> = Vec::new();
            let slot_of = |src: InSrc, in_map: &mut Vec<InSrc>| -> usize {
                if let Some(k) = in_map.iter().position(|x| *x == src) {
                    k
                } else {
                    in_map.push(src);
                    in_map.len() - 1
                }
            };
            let map = |nd: Node, local: &mut Vec<usize>, nn: &mut usize, srcs: &mut Vec<(Node, usize)>, in_map: &mut Vec<InSrc>| -> Node {
                if nd == 0 {
                    return 0;
                }
                if local[nd] == 0 {
                    *nn += 1;
                    local[nd] = *nn;
                    if let Some(k) = net.source_of[nd] {
                        let sl = slot_of(InSrc::Ext(k), in_map);
                        srcs.push((*nn, sl));
                    } else if owner[nd] != s {
                        let j = exports[owner[nd]].iter().position(|x| *x == nd).unwrap();
                        let sl = slot_of(InSrc::Stage(owner[nd], j), in_map);
                        srcs.push((*nn, sl));
                    }
                }
                local[nd]
            };
            let mut elems = Vec::new();
            let mut reactive = Vec::new();
            let mut ports = Vec::new();
            let mut devs = Vec::new();
            let mut vars = Vec::new();
            for (k, e) in net.elements.iter().enumerate() {
                if stage_of(k) != s {
                    continue;
                }
                let mut l = |nd: Node| map(nd, &mut local, &mut nn, &mut srcs, &mut in_map);
                let le = match *e {
                    Element::R(a, b, r) => Element::R(l(a), l(b), r),
                    Element::C(a, b, c) => {
                        let (a, b) = (l(a), l(b));
                        reactive.push((a, b, c, RKind::C));
                        Element::C(a, b, c)
                    }
                    Element::CBe(a, b, c) => {
                        let (a, b) = (l(a), l(b));
                        reactive.push((a, b, c, RKind::Be));
                        Element::CBe(a, b, c)
                    }
                    Element::L(a, b, h) => {
                        let (a, b) = (l(a), l(b));
                        reactive.push((a, b, h, RKind::L));
                        Element::L(a, b, h)
                    }
                    Element::Var(a, b, v) => {
                        let (a, b) = (l(a), l(b));
                        var_map[v] = (s, vars.len());
                        vars.push((a, b, v));
                        Element::Var(a, b, v)
                    }
                    Element::ISrc(a, b, k) => {
                        let (a, b) = (l(a), l(b));
                        let sl = slot_of(InSrc::Ext(k), &mut in_map);
                        Element::ISrc(a, b, sl)
                    }
                    Element::OpAmp(a, b, c) => Element::OpAmp(l(a), l(b), l(c)),
                    Element::Buffer(a, b) => Element::Buffer(l(a), l(b)),
                    Element::Device(d, n) => {
                        let ln = [l(n[0]), l(n[1]), l(n[2])];
                        let off = ports.len();
                        match net.devices[d] {
                            DeviceModel::Clipper(_) => ports.push([ln[0], ln[1], ln[0], ln[1]]),
                            DeviceModel::Bjt(_) => {
                                ports.push([ln[1], ln[2], ln[1], ln[2]]);
                                ports.push([ln[1], ln[0], ln[0], ln[2]]);
                            }
                            DeviceModel::OpAmp(_) => {
                                ports.push([ln[0], ln[1], 0, ln[2]]);
                                ports.push([ln[2], 0, ln[2], 0]);
                            }
                            // (d, g, s) as the BJT's (c, b, e): (v_GS, i_G g → s), (v_GD, i_D d → s)
                            DeviceModel::Jfet(_) => {
                                ports.push([ln[1], ln[2], ln[1], ln[2]]);
                                ports.push([ln[1], ln[0], ln[0], ln[2]]);
                            }
                            // (+, −, drain): v(+) − v(−), the current from the drain to ground
                            DeviceModel::Pair(_) => ports.push([ln[0], ln[1], ln[2], 0]),
                        }
                        dev_map[d] = (s, devs.len());
                        devs.push((d, off, evaluate(&net.devices[d], net.rails[d], 25.0, 9.0)));
                        Element::Device(d, ln)
                    }
                };
                elems.push(le);
            }
            let obs: Vec<Node> = stage_obs[s].iter().map(|&nd| map(nd, &mut local, &mut nn, &mut srcs, &mut in_map)).collect();
            let tails: Vec<(usize, usize, usize)> = tail_obs[s].iter().map(|&(d, a, b)| (dev_map[d].1, a, b)).collect();
            let m = ports.len();
            assert!(m <= MAX_PORTS, "stage {s}: {m} ports (max {MAX_PORTS})");
            let nu = in_map.len();
            assert!(nu <= 16, "stage {s}: {nu} input slots (max 16)");
            let nr = reactive.len();
            let no = obs.len();
            let var_g: Vec<f64> = vars.iter().map(|&(_, _, v)| 1.0 / net.vars[v].0).collect();
            let var_g0: Vec<f64> = vars.iter().map(|&(_, _, v)| 1.0 / (net.vars[v].1 * net.vars[v].2).sqrt()).collect();
            let nl = reactive.iter().filter(|r| r.3 == RKind::L).count();
            let nb = srcs.len() + elems.iter().filter(|e| matches!(e, Element::OpAmp(..) | Element::Buffer(..))).count();
            let n_dc = nn + nb + nl;
            let mut st = Stage {
                nn,
                elems,
                srcs,
                in_map,
                reactive,
                ports,
                devs,
                vars,
                var_g0,
                var_g,
                obs,
                tails,
                m,
                nr,
                nu,
                no,
                t,
                p0: Vec::new(),
                p: Vec::new(),
                lw: Vec::new(),
                rw: Vec::new(),
                q: Vec::new(),
                dirty: true,
                kbuf: [0.0; MAX_PORTS * MAX_PORTS],
                rg: Vec::new(),
                n_dc,
                m_dc: vec![0.0; n_dc * n_dc],
                inv_dc: vec![0.0; n_dc * n_dc],
                work_dc: vec![0.0; n_dc * 2 * n_dc],
                p_dc: vec![0.0; (m + nr + no) * (nu + m)],
                seed_v: vec![0.0; m],
                seed_u: vec![0.0; nu],
                seeded: false,
                j: vec![0.0; nr],
                x: vec![0.0; nr + nu + m],
                jn: vec![0.0; nr],
                v: [0.0; MAX_PORTS],
                v1: [0.0; MAX_PORTS],
                v2: [0.0; MAX_PORTS],
                i: [0.0; MAX_PORTS],
                pp: [0.0; MAX_PORTS],
                plain: [false, false],
                plain2: false,
                quad: false,
                y: vec![0.0; no],
                u: vec![0.0; nu],
                tol: 1e-6,
                lazy: lazy[s],
                stepped: false,
                recip: false,
                stats: Stats::default(),
            };
            st.build();
            stages.push(st);
        }
        let mut c = Self {
            stages,
            out_map,
            var_map,
            dev_map,
            dev_models: net.devices.clone(),
            dev_rails: net.rails.clone(),
            pots: net.pots.clone(),
            inputs: vec![0.0; net.inputs],
            celsius: 25.0,
            supply: 9.0,
            sample_rate,
        };
        c.reevaluate();
        c
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }

    /// Port count of each stage.
    pub fn ports(&self) -> Vec<usize> {
        self.stages.iter().map(|s| s.m).collect()
    }

    /// Newton's stop tolerance |Δv| (V); 10⁻⁶ by default, 10⁻¹² for an offline reference.
    pub fn set_tolerance(&mut self, tol: f64) {
        for s in &mut self.stages {
            s.tol = tol;
        }
    }

    /// Newton's Jacobian solve by reciprocal pivots (one division per pivot instead of one per
    /// row): faster, rounding-level different. Off by default (gfx-dist's renders are unchanged);
    /// gfx-fuzz turns it on.
    pub fn set_fast_solve(&mut self, on: bool) {
        for s in &mut self.stages {
            s.recip = on;
        }
    }

    /// Newton's seed by quadratic extrapolation of the last three solutions (when all three were
    /// solved by plain Newton; linear otherwise): fewer iterations on an oversampled signal. Off by
    /// default (gfx-dist's renders are unchanged); gfx-fuzz turns it on.
    pub fn set_quadratic_seed(&mut self, on: bool) {
        for s in &mut self.stages {
            s.quad = on;
        }
    }

    fn reevaluate(&mut self) {
        for (d, &(s, k)) in self.dev_map.iter().enumerate() {
            let old = self.stages[s].devs[k].2;
            self.stages[s].devs[k].2 = keep_latch(old, evaluate(&self.dev_models[d], self.dev_rails[d], self.celsius, self.supply));
        }
    }

    /// The supply voltage the op-amp rails follow (the supply's own input in u is the caller's).
    pub fn set_supply(&mut self, volts: f64) {
        if volts != self.supply {
            self.supply = volts;
            for (d, &(s, k)) in self.dev_map.iter().enumerate() {
                if let DeviceModel::OpAmp(_) = self.dev_models[d] {
                    self.stages[s].devs[k].2 = evaluate(&self.dev_models[d], self.dev_rails[d], self.celsius, volts);
                }
            }
        }
    }

    /// Every device's temperature (°C): V_T and Is by SPICE's law.
    pub fn set_temperature(&mut self, celsius: f64) {
        if celsius != self.celsius {
            self.celsius = celsius;
            self.reevaluate();
        }
    }

    /// Replaces a device's constants (a part swap). Must keep the device's kind.
    pub fn set_device(&mut self, device: usize, model: DeviceModel) {
        assert_eq!(model.ports(), self.dev_models[device].ports(), "a part swap keeps the device's kind");
        if self.dev_models[device] != model {
            self.dev_models[device] = model;
            let (s, k) = self.dev_map[device];
            let old = self.stages[s].devs[k].2;
            self.stages[s].devs[k].2 = keep_latch(old, evaluate(&model, self.dev_rails[device], self.celsius, self.supply));
        }
    }

    pub fn device(&self, device: usize) -> DeviceModel {
        self.dev_models[device]
    }

    /// Sets a variable resistor (ohms, clamped at R_MIN). Applied by Woodbury at the next sample.
    pub fn set_var(&mut self, var: usize, ohms: f64) {
        let (s, k) = self.var_map[var];
        let g = 1.0 / ohms.max(R_MIN);
        if self.stages[s].var_g[k] != g {
            self.stages[s].var_g[k] = g;
            self.stages[s].dirty = true;
        }
    }

    /// Turns pot `pot` to rotation `t` (0 – 1) with its law.
    pub fn set_pot(&mut self, pot: usize, t: f64) {
        let p = self.pots[pot];
        let (a, b) = p.halves(t);
        if let Some(v) = p.start {
            self.set_var(v, a);
        }
        if let Some(v) = p.end {
            self.set_var(v, b);
        }
    }

    /// Sets input `k` of the vector u.
    #[inline]
    pub fn set_input(&mut self, k: usize, value: f64) {
        self.inputs[k] = value;
    }

    fn gather(&mut self, s: usize) {
        let (before, rest) = self.stages.split_at_mut(s);
        let st = &mut rest[0];
        for (sl, src) in st.in_map.iter().enumerate() {
            st.u[sl] = match *src {
                InSrc::Ext(k) => self.inputs[k],
                InSrc::Stage(o, j) => before[o].y[j],
            };
        }
    }

    /// One sample with the inputs set by [`NonlinearCircuit::set_input`].
    #[inline]
    pub fn process(&mut self) {
        for s in 0..self.stages.len() {
            self.gather(s);
            self.stages[s].step();
        }
    }

    /// Output `k` (a node voltage declared by [`Netlist::output`]) after the last sample.
    #[inline]
    pub fn output(&self, k: usize) -> f64 {
        let (s, o) = self.out_map[k];
        self.stages[s].y[o]
    }

    /// Probe `k` (declared by [`Netlist::probe`], or any output) after the last sample, computed now.
    pub fn probe(&self, k: usize) -> f64 {
        let (s, o) = self.out_map[k];
        let st = &self.stages[s];
        if !st.stepped || o < st.no - st.lazy {
            return st.y[o];
        }
        let ncol = st.ncol();
        let row = &st.p[(st.m + st.nr + o) * ncol..(st.m + st.nr + o + 1) * ncol];
        let mut acc = 0.0;
        for c in 0..ncol {
            acc += row[c] * st.x[c];
        }
        acc
    }

    /// Solves the DC operating point for the current inputs, pots and devices and puts every state
    /// at rest there. Inline and bounded (≤ 50 Newton iterations per attempt). Returns whether every
    /// stage converged.
    pub fn solve_dc(&mut self) -> bool {
        let mut ok = true;
        for s in 0..self.stages.len() {
            self.gather(s);
            ok &= self.stages[s].solve_dc(false);
        }
        ok
    }

    /// [`NonlinearCircuit::solve_dc`] with source stepping from zero, kept as every later re-solve's
    /// seed (at `initialize`).
    pub fn init_dc(&mut self) -> bool {
        let mut ok = true;
        for s in 0..self.stages.len() {
            self.gather(s);
            self.stages[s].seeded = false;
            ok &= self.stages[s].solve_dc(true);
        }
        ok
    }

    /// Copies every state (and the pots, devices, supply) from a circuit compiled from the same
    /// netlist at the same rate. No allocation.
    pub fn copy_state_from(&mut self, o: &NonlinearCircuit) {
        for (a, b) in self.stages.iter_mut().zip(&o.stages) {
            a.j.copy_from_slice(&b.j);
            a.y.copy_from_slice(&b.y);
            a.u.copy_from_slice(&b.u);
            a.v = b.v;
            a.v1 = b.v1;
            a.v2 = b.v2;
            a.plain2 = b.plain2;
            a.i = b.i;
            a.pp = b.pp;
            a.plain = b.plain;
            a.var_g.copy_from_slice(&b.var_g);
            a.dirty = true;
            a.devs.copy_from_slice(&b.devs);
            a.seed_v.copy_from_slice(&b.seed_v);
            a.seed_u.copy_from_slice(&b.seed_u);
            a.seeded = b.seeded;
        }
        self.dev_models.copy_from_slice(&o.dev_models);
        self.inputs.copy_from_slice(&o.inputs);
        self.celsius = o.celsius;
        self.supply = o.supply;
    }

    /// Solver statistics, all stages summed.
    pub fn stats(&self) -> Stats {
        let mut s = Stats::default();
        for st in &self.stages {
            s.add(&st.stats);
        }
        s
    }

    /// Solver statistics of stage `s`.
    pub fn stage_stats(&self, s: usize) -> Stats {
        self.stages[s].stats
    }

    pub fn reset_stats(&mut self) {
        for st in &mut self.stages {
            st.stats = Stats::default();
        }
    }

    /// The port voltages of stage `s` (diagnostics, the bench's uniqueness check).
    pub fn port_voltages(&self, s: usize) -> &[f64] {
        &self.stages[s].v[..self.stages[s].m]
    }

    /// det(K Jf − I) of stage `s` at port voltages `v` (the uniqueness check: its sign must not
    /// change over the port range).
    pub fn jacobian_det(&self, s: usize, v: &[f64]) -> f64 {
        let st = &self.stages[s];
        let m = st.m;
        let mut k = [0.0; MAX_PORTS * MAX_PORTS];
        let mut st2 = st.clone();
        if st2.dirty {
            st2.woodbury();
        }
        st2.kmat(&mut k);
        let nw = Newton { k: &k[..m * m], m, devs: &st.devs, tol: st.tol, dc: false, plain_steps: false, blk: blocks(&st.devs), recip: st.recip };
        let mut vv = [0.0; MAX_PORTS];
        vv[..m].copy_from_slice(v);
        let mut i = [0.0; MAX_PORTS];
        let mut jf = [0.0; MAX_PORTS * MAX_PORTS];
        nw.eval(&vv, &mut i, &mut jf);
        let mut a = vec![0.0; m * m];
        for r in 0..m {
            for c in 0..m {
                let mut acc = 0.0;
                for q in 0..m {
                    acc += k[r * m + q] * jf[q * m + c];
                }
                a[r * m + c] = acc - if r == c { 1.0 } else { 0.0 };
            }
        }
        // determinant by elimination with pivoting
        let mut det = 1.0;
        for col in 0..m {
            let mut piv = col;
            for r in col + 1..m {
                if a[r * m + col].abs() > a[piv * m + col].abs() {
                    piv = r;
                }
            }
            if a[piv * m + col] == 0.0 {
                return 0.0;
            }
            if piv != col {
                for c in 0..m {
                    a.swap(col * m + c, piv * m + c);
                }
                det = -det;
            }
            det *= a[col * m + col];
            for r in col + 1..m {
                let f = a[r * m + col] / a[col * m + col];
                for c in col..m {
                    a[r * m + c] -= f * a[col * m + c];
                }
            }
        }
        det
    }

    /// The small-signal transfer from input `input` to output `output` at `freq` Hz, the circuit
    /// linearized at its present operating point (Laplace domain, s = jω). Allocates (analysis).
    pub fn ac_response(&self, freq: f64, input: usize, output: usize) -> (f64, f64) {
        let mut outs: Vec<Vec<(f64, f64)>> = Vec::new();
        for st in &self.stages {
            let mut st = st.clone();
            if st.dirty {
                st.woodbury();
            }
            let u: Vec<(f64, f64)> = st
                .in_map
                .iter()
                .map(|src| match *src {
                    InSrc::Ext(k) => {
                        if k == input {
                            (1.0, 0.0)
                        } else {
                            (0.0, 0.0)
                        }
                    }
                    InSrc::Stage(o, j) => outs[o][j],
                })
                .collect();
            outs.push(st.ac(freq, &u));
        }
        let (s, o) = self.out_map[output];
        outs[s][o]
    }
}

// ---------------------------------------------------------------------------------------------
// Parts (constants from SPICE models and datasheets; [U] where not verified against a source)

/// 1N914 / 1N4148 (the common SPICE model: Is 2.52 nA, N 1.752).
pub const D_1N914: Junction = Junction::silicon(2.52e-9, 1.752);
/// 1N34A germanium point-contact: Vf ≈ 0.29 V at 1 mA [U: generic germanium constants].
pub const D_1N34A: Junction = Junction::germanium(2e-7, 1.3);
/// Red LED (Is 93.2 pA, N 3.73: ≈ 1.6 V at 1 mA).
pub const D_LED_RED: Junction = Junction::silicon(93.2e-12, 3.73);
/// 2N7000 body diode: ≈ 0.6 V at 1 mA [U].
pub const D_2N7000_BODY: Junction = Junction::silicon(1e-12, 1.1);
/// 2N7000 channel, diode-connected: Vth ≈ 2.1 V [U: datasheet typical].
pub const CH_2N7000: Channel = Channel { vth: 2.1, k: 0.1 };

/// LM308 with 30 pF: 0.3 V/µs and 1 MHz read from the datasheet curves; swing ≈ 1 V from each rail.
pub const LM308: OpAmpModel = OpAmpModel { sr: 0.3e6, gbw: 1.0e6, a0: 3e5, sat_hi: 1.0, sat_lo: 1.0, square_law: false };
/// LM358 class (single-supply, output to ground, ≈ 1.5 V under the supply) [U].
pub const LM358: OpAmpModel = OpAmpModel { sr: 0.4e6, gbw: 0.8e6, a0: 1e5, sat_hi: 1.5, sat_lo: 0.05, square_law: false };
/// 4558: 1.7 V/µs, 3 MHz, ≈ 1 V swing.
pub const RC4558: OpAmpModel = OpAmpModel { sr: 1.7e6, gbw: 3.0e6, a0: 1e5, sat_hi: 1.0, sat_lo: 1.0, square_law: false };
/// TL072 / TL082: 13 V/µs, 3 MHz, ≈ 1.5 V swing.
pub const TL072: OpAmpModel = OpAmpModel { sr: 13e6, gbw: 3.0e6, a0: 2e5, sat_hi: 1.5, sat_lo: 1.5, square_law: false };
/// "Ideal": 50 V/µs, 20 MHz, rail to rail — fast and wide enough to vanish from the audio band at
/// these gains (a truly infinite slew and bandwidth is no macromodel: stiffer than any rate solves).
pub const IDEAL: OpAmpModel = OpAmpModel { sr: 50e6, gbw: 20e6, a0: 1e6, sat_hi: 0.0, sat_lo: 0.0, square_law: false };

#[cfg(test)]
mod tests;
