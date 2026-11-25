// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-dist — golem's circuit-simulated distortion pedals (design: README.md).
//!
//! Each circuit is a netlist (circuits.rs) solved sample by sample by gfx-dsp's `circuit_nl` (nodal
//! DK method, damped Newton, Woodbury pot updates, exact stage cuts), oversampled per circuit through
//! gfx-dsp's minimum-phase half-band cascade. The knobs turn the netlist's pots by rotation with
//! their real tapers; `supply` is an input of the netlist; `diodes` / `opamp` / `hp` swap parts.
//!
//! - Per channel, two slots, each holding every circuit compiled at its rate: the running one, and
//!   the one a `circuit` / part / `hp` change prepares (from its DC point, or a copy of the running
//!   state for a part swap) and crossfades in (30 ms equal-power for a circuit, 20 ms linear for a
//!   part on the same circuit).
//! - `bass_keep`: a Linkwitz-Riley 4 split before the circuit; the low band at unity through the
//!   circuit's post-clipper network made linear (its twin, at the host rate) and the oversampler's
//!   identity path of the same rate.
//! - `level` 0 dB = unity loudness at the defaults on every circuit: a fixed K per circuit.

#![allow(clippy::needless_range_loop)]

use gfx_dsp::circuit_nl::*;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::oversample::{OversamplerN, MAX_FACTOR};
use gfx_dsp::smooth::OnePole;
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::{Effect, ParamInfo};

pub mod circuits;
use circuits::*;

#[cfg(feature = "plugin")]
pub mod plugin;

/// Parameter indices (PARAMS order).
pub mod p {
    pub const CIRCUIT: usize = 0;
    pub const GAIN: usize = 1;
    pub const TONE: usize = 2;
    pub const LEVEL: usize = 3;
    pub const DIODES: usize = 4;
    pub const OPAMP: usize = 5;
    pub const SUPPLY: usize = 6;
    pub const HP: usize = 7;
    pub const TIGHT: usize = 8;
    pub const INPUT: usize = 9;
    pub const BASS_KEEP: usize = 10;
}

/// The `circuit` options shipped, in the README's order.
pub const CIRCUITS: [&str; N_CIRCUITS] = ["Op-Amp Dist", "Booster Dist", "Blend OD", "MOSFET OD"];
pub const N_CIRCUITS: usize = 4;
pub const DIODES: [&str; 7] = ["Circuit", "Silicon", "Germanium", "LED", "MOSFET", "MOSFET + Ge", "None"];
pub const OPAMPS: [&str; 6] = ["Circuit", "LM308", "LM358", "4558", "TL072", "Ideal"];

pub static PARAMS: [ParamInfo; 11] = [
    ParamInfo::enumeration("circuit", "Circuit", N_CIRCUITS as u32, 0),
    ParamInfo::float("gain", "Gain", 0.0, 100.0, 50.0).with_exp(),
    ParamInfo::float("tone_stack", "Tone", 0.0, 100.0, 50.0).with_exp(),
    ParamInfo::float("level", "Level", -24.0, 24.0, 0.0).with_exp(),
    ParamInfo::enumeration("diodes", "Clippers", 7, 0),
    ParamInfo::enumeration("opamp", "Op-Amp", 6, 0),
    ParamInfo::float("supply", "Supply", 50.0, 200.0, 100.0),
    ParamInfo::boolean("hp", "HP Mode", false),
    ParamInfo::float("tight", "Tight", 20.0, 1000.0, 20.0),
    ParamInfo::float("input", "Input", -12.0, 12.0, 0.0),
    ParamInfo::float("bass_keep", "Bass Keep", 0.0, 500.0, 0.0),
];

/// K(circuit), dB: `level` 0 dB = the bypass loudness at `gain` 50 %, `tone_stack` 50 % (README §2.4;
/// measured by `tests::level_constants`). "Hardware level pot fully up" = `level` −K.
pub const K_DB: [f64; N_CIRCUITS] = [-14.0, -3.6, -27.0, -16.3];

const MAX_CH: usize = 2;
const FADE_CIRCUIT_MS: f64 = 30.0;
const FADE_PART_MS: f64 = 20.0;
/// Linked mono: L == R for this long before channel 1 fades onto channel 0's output (over
/// FADE_CIRCUIT_MS).
const LINK_HOLD_MS: f64 = 60.0;

/// A circuit build: the netlist's description and its compiled solvers.
pub fn build(k: usize) -> Circuit {
    match k {
        0 => opamp_dist(),
        1 => booster_dist(),
        2 => blend_od(),
        _ => mosfet_od(),
    }
}

/// The clipper model `diodes` option `opt` gives a clipper whose circuit model is `own` with
/// `count` junctions per side.
pub fn clipper_option(opt: usize, own: ClipperModel, count: u32) -> ClipperModel {
    let mosfet = ClipSide { j: D_2N7000_BODY, count, extra: None, channel: Some(CH_2N7000) };
    match opt {
        1 => ClipperModel::pair(D_1N914, count),
        2 => ClipperModel::pair(D_1N34A, count),
        3 => ClipperModel::pair(D_LED_RED, count),
        4 => ClipperModel { pos: Some(mosfet), neg: Some(mosfet) },
        5 => ClipperModel { pos: Some(mosfet), neg: Some(ClipSide { extra: Some(D_1N34A), channel: None, ..mosfet }) },
        6 => ClipperModel::NONE,
        _ => own,
    }
}

/// The op-amp model `opamp` option `opt` gives an op-amp whose circuit part is `own`.
pub fn opamp_option(opt: usize, own: OpAmpModel) -> OpAmpModel {
    match opt {
        1 => LM308,
        2 => LM358,
        3 => RC4558,
        4 => TL072,
        5 => IDEAL,
        _ => own,
    }
}

/// What a slot runs: everything a fade switches.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Config {
    circuit: usize,
    diodes: usize,
    opamp: usize,
    hp: bool,
}

/// One circuit compiled for a slot: the nonlinear solver and the bass-keep twin.
#[derive(Clone, Debug)]
struct Compiled {
    def: Circuit,
    nc: NonlinearCircuit,
    twin: NonlinearCircuit,
    stages: usize,
}

impl Compiled {
    /// Circuit `k` at its own rate, or at the rate its `opamp` Ideal swap needs (`fast`).
    fn new(k: usize, sr: f64, force: Option<usize>, fast: bool) -> Self {
        let def = build(k);
        let base = if fast { def.ideal_stages } else { def.stages };
        let stages = force.unwrap_or_else(|| stages_at(base, sr));
        let fs = sr * (1usize << stages) as f64;
        let nc = NonlinearCircuit::new(&def.net, fs);
        let tw = def.net.linear_twin(def.twin.0, def.twin.1);
        let twin = NonlinearCircuit::new(&tw, sr);
        Self { def, nc, twin, stages }
    }

    /// Applies a configuration's parts (no matrix change except `hp`'s variable resistor).
    fn apply(&mut self, cfg: Config) {
        for &(d, own, count) in &self.def.clippers {
            self.nc.set_device(d, DeviceModel::Clipper(clipper_option(cfg.diodes, own, count)));
        }
        for &(d, own) in &self.def.opamps {
            self.nc.set_device(d, DeviceModel::OpAmp(opamp_option(cfg.opamp, own)));
        }
        if let Some((v, r)) = self.def.hp {
            self.nc.set_var(v, if cfg.hp { r } else { 1e9 });
            self.twin.set_var(v - first_twin_var(&self.def), if cfg.hp { r } else { 1e9 });
        }
    }

    fn set_pots(&mut self, gain: f64, tone: f64) {
        for &(pt, rev) in &self.def.gain {
            let t = if rev { 1.0 - gain } else { gain };
            self.nc.set_pot(pt, t);
            self.twin.set_pot(pt, t);
        }
        for &(pt, rev) in &self.def.tone {
            let t = if rev { 1.0 - tone } else { tone };
            self.nc.set_pot(pt, t);
            self.twin.set_pot(pt, t);
        }
    }

    fn set_supply(&mut self, vs: f64) {
        self.nc.set_input(IN_VS, vs);
        self.nc.set_input(IN_VB, 0.5 * vs);
        self.nc.set_supply(vs);
    }
}

/// The twin's variable resistors are the netlist's from the twin mark on: the index shift.
fn first_twin_var(def: &Circuit) -> usize {
    def.net.elements()[..def.twin.0].iter().filter(|e| matches!(e, Element::Var(..))).count()
}

/// Half-band stages at host rate `sr` for a circuit with `base` stages at 44.1 / 48 kHz.
pub fn stages_at(base: usize, sr: f64) -> usize {
    if sr > 150_000.0 {
        base.saturating_sub(2)
    } else if sr > 75_000.0 {
        base.saturating_sub(1)
    } else {
        base
    }
}

#[derive(Clone, Debug)]
struct Slot {
    cfg: Config,
    /// The compiled variant running: `circuit`, or N_CIRCUITS + `circuit` for an Ideal swap that
    /// needs a higher rate.
    var: usize,
    circuits: Vec<Compiled>,
    os: OversamplerN,
    twin_os: OversamplerN,
}

impl Slot {
    fn cur(&mut self) -> &mut Compiled {
        &mut self.circuits[self.var]
    }

    /// The variant a configuration runs on.
    fn variant(&self, cfg: Config) -> usize {
        let fast = N_CIRCUITS + cfg.circuit;
        if cfg.opamp == 5 && self.circuits[fast].stages != self.circuits[cfg.circuit].stages {
            fast
        } else {
            cfg.circuit
        }
    }

    /// One host sample of the high band through the circuit.
    #[inline]
    fn run(&mut self, x: f64) -> f64 {
        let c = &mut self.circuits[self.var];
        let mut buf = [0.0; MAX_FACTOR];
        self.os.up(x, &mut buf);
        let n = 1usize << c.stages;
        for v in buf.iter_mut().take(n) {
            c.nc.set_input(IN_SIG, *v);
            c.nc.process();
            *v = c.nc.output(c.def.out);
        }
        self.os.down(&mut buf)
    }

    /// One host sample of the low band through the twin.
    #[inline]
    fn run_twin(&mut self, x: f64) -> f64 {
        let c = &mut self.circuits[self.var];
        let entry = c.def.net.inputs();
        c.twin.set_input(entry, c.def.polarity * x);
        c.twin.process();
        let y = c.twin.output(c.def.out);
        self.twin_os.identity(y)
    }
}

/// First-order high-pass (prewarped bilinear).
#[derive(Clone, Copy, Debug, Default)]
struct HighPass {
    b0: f64,
    a1: f64,
    x1: f64,
    y1: f64,
}

impl HighPass {
    fn set(&mut self, f: f64, sr: f64) {
        let k = (std::f64::consts::PI * f.min(0.45 * sr) / sr).tan();
        self.b0 = 1.0 / (1.0 + k);
        self.a1 = (k - 1.0) / (1.0 + k);
    }
    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * (x - self.x1) - self.a1 * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
    fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

#[derive(Clone, Debug)]
struct Chan {
    slots: [Slot; 2],
    tight: HighPass,
    dc: HighPass,
    split_lo: [Svf; 2],
    split_hi: [Svf; 2],
}

pub struct Dist {
    sr: f64,
    v: [f32; 11],
    ch: Vec<Chan>,
    now: u64,
    gain: ExpSmoother,
    tone: ExpSmoother,
    level: ExpSmoother,
    supply: OnePole,
    tight: OnePole,
    input: OnePole,
    bass_keep: OnePole,
    tight_mix: OnePole,
    /// The running slot, and the fade towards the other one (0 = none).
    active: usize,
    fade: f64,
    fade_step: f64,
    fade_power: bool,
    tight_set: f64,
    split_set: f64,
    /// (input dB, level dB, input gain, level gain), recomputed when they move.
    gains: (f32, f32, f64, f64),
    k_lin: [f64; N_CIRCUITS],
    /// Linked mono (README, As coded): while channel 1's input is bit-identical to channel 0's, only
    /// channel 0's circuit runs and channel 1 outputs its output. On the first sample that differs,
    /// channel 1 takes a copy of channel 0's whole state and runs from there.
    linked: bool,
    /// Unlinked: consecutive samples with L == R, and the linear fade of channel 1 onto channel 0's
    /// output (0 – 1) that relinks them.
    same_run: u32,
    relink: f64,
    /// Tests: true stereo always (the reference linked mono is checked against).
    #[doc(hidden)]
    pub no_link: bool,
    /// Tests: forces the half-band stage count of every circuit (the alias measurements).
    #[doc(hidden)]
    pub stages_override: Option<usize>,
}

impl Dist {
    pub fn new() -> Self {
        let v: [f32; 11] = std::array::from_fn(|i| PARAMS[i].default);
        let exp = |i: usize| ExpSmoother::new(PARAMS[i].min, PARAMS[i].max, PARAMS[i].default);
        Self {
            sr: 48000.0,
            v,
            ch: Vec::new(),
            now: 0,
            gain: exp(p::GAIN),
            tone: exp(p::TONE),
            level: exp(p::LEVEL),
            supply: OnePole::new(100.0, 100.0, 1e-4),
            tight: OnePole::new(30.0, 20.0, 1e-2),
            input: OnePole::new(20.0, 0.0, 1e-4),
            bass_keep: OnePole::new(30.0, 0.0, 1e-2),
            tight_mix: OnePole::new(5.0, 0.0, 1e-6),
            active: 0,
            fade: 0.0,
            fade_step: 0.0,
            fade_power: false,
            tight_set: f64::NAN,
            split_set: f64::NAN,
            gains: (f32::NAN, f32::NAN, 1.0, 1.0),
            k_lin: std::array::from_fn(|k| 10f64.powf(K_DB[k] / 20.0)),
            linked: false,
            same_run: 0,
            relink: 0.0,
            no_link: false,
            stages_override: None,
        }
    }

    fn target(&self) -> Config {
        Config {
            circuit: (self.v[p::CIRCUIT].round() as usize).min(N_CIRCUITS - 1),
            diodes: (self.v[p::DIODES].round() as usize).min(6),
            opamp: (self.v[p::OPAMP].round() as usize).min(5),
            hp: self.v[p::HP] > 0.5,
        }
    }

    fn supply_volts(&self, k: usize) -> f64 {
        build_supply(k) * self.supply.value() as f64 / 100.0
    }

    /// Puts slot `s` of every channel on `cfg` at rest (DC point), with the current controls.
    ///
    /// A part swap starts from the DC point too, not from a copy of the running state: a diode node
    /// sitting at 3 V when silicon replaces "None" is a 60-iteration walk down an exponential, and
    /// the DC point of the new parts is where the real pedal would be after the switch's thump.
    fn start_slot(&mut self, s: usize, cfg: Config) {
        let (g, t) = (self.gain.value() as f64 / 100.0, self.tone.value() as f64 / 100.0);
        let vs = self.supply_volts(cfg.circuit);
        let live = self.live();
        for c in self.ch.iter_mut().take(live) {
            let dst = &mut c.slots[s];
            dst.cfg = cfg;
            let k = dst.variant(cfg);
            dst.var = k;
            let cc = &mut dst.circuits[k];
            cc.apply(cfg);
            cc.set_pots(g, t);
            cc.set_supply(vs);
            cc.nc.set_input(IN_SIG, 0.0);
            cc.nc.solve_dc();
            cc.twin.solve_dc();
            let st = cc.stages;
            dst.os.set_stages(st);
            dst.twin_os.set_stages(st);
        }
    }

    /// The channels whose circuits run: channel 0 alone while linked.
    fn live(&self) -> usize {
        if self.linked {
            1
        } else {
            self.ch.len()
        }
    }

    /// Unlinks channel 1: a copy of channel 0's whole state (every compiled circuit and twin, both
    /// slots, the filters), so it continues exactly as if it had run on the same input all along. No
    /// allocation.
    fn unlink(&mut self) {
        self.linked = false;
        self.same_run = 0;
        self.relink = 0.0;
        let (a, b) = self.ch.split_at_mut(1);
        let (src, dst) = (&a[0], &mut b[0]);
        for (d, s) in dst.slots.iter_mut().zip(&src.slots) {
            d.cfg = s.cfg;
            d.var = s.var;
            for (dc, sc) in d.circuits.iter_mut().zip(&s.circuits) {
                dc.nc.copy_state_from(&sc.nc);
                dc.twin.copy_state_from(&sc.twin);
            }
            d.os.copy_from(&s.os);
            d.twin_os.copy_from(&s.twin_os);
        }
        dst.tight = src.tight;
        dst.dc = src.dc;
        dst.split_lo.clone_from(&src.split_lo);
        dst.split_hi.clone_from(&src.split_hi);
    }

    fn update_controls(&mut self) {
        let sr = self.sr;
        let t = self.tight.value() as f64;
        if (t - self.tight_set).abs() > 1e-3 || self.tight_set.is_nan() {
            self.tight_set = t;
            for c in &mut self.ch {
                c.tight.set(t, sr);
            }
        }
        let bk = self.bass_keep.value() as f64;
        if (bk - self.split_set).abs() > 0.05 || self.split_set.is_nan() {
            self.split_set = bk;
            if bk > 0.5 {
                for c in &mut self.ch {
                    for k in 0..2 {
                        c.split_lo[k].set(SvfMode::LowPass, bk as f32, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr as f32);
                        c.split_hi[k].set(SvfMode::HighPass, bk as f32, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr as f32);
                    }
                }
            }
        }
    }

    /// Per-stage statistics of the running circuit of channel 0.
    pub fn stage_stats(&self) -> Vec<Stats> {
        let s = &self.ch[0].slots[self.active];
        let nc = &s.circuits[s.var].nc;
        (0..nc.stage_count()).map(|k| nc.stage_stats(k)).collect()
    }

    /// Statistics of the running circuit of channel 0 (tests, bench).
    pub fn stats(&self) -> Stats {
        let s = &self.ch[0].slots[self.active];
        s.circuits[s.var].nc.stats()
    }
}

/// A circuit's own supply.
pub fn build_supply(k: usize) -> f64 {
    SUPPLY[k]
}

/// Each circuit's own supply (V): 9 V on all four shipped circuits (the Blend OD's pumped
/// +18 / −9 V rails follow it as multiples; the Discrete OD's 8 V will be the first to differ).
pub const SUPPLY: [f64; N_CIRCUITS] = [9.0; N_CIRCUITS];

impl Default for Dist {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Dist {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate as f64;
        let n = channels.clamp(1, MAX_CH);
        // every circuit compiled once, its DC seed solved at the default controls
        let proto: Vec<Compiled> = (0..2 * N_CIRCUITS)
            .map(|v| {
                let k = v % N_CIRCUITS;
                let mut c = Compiled::new(k, self.sr, self.stages_override, v >= N_CIRCUITS);
                let cfg = Config { circuit: k, diodes: 0, opamp: 0, hp: false };
                c.apply(cfg);
                c.set_pots(0.5, 0.5);
                c.set_supply(build_supply(k));
                c.nc.init_dc();
                c.twin.init_dc();
                c
            })
            .collect();
        let slot = Slot {
            cfg: Config { circuit: 0, diodes: 0, opamp: 0, hp: false },
            var: 0,
            circuits: proto,
            os: OversamplerN::new(0),
            twin_os: OversamplerN::new(0),
        };
        self.ch = (0..n)
            .map(|_| Chan {
                slots: [slot.clone(), slot.clone()],
                tight: HighPass::default(),
                dc: HighPass::default(),
                split_lo: [Svf::new(), Svf::new()],
                split_hi: [Svf::new(), Svf::new()],
            })
            .collect();
        for c in &mut self.ch {
            c.dc.set(5.0, self.sr);
        }
        for e in [&mut self.gain, &mut self.tone, &mut self.level] {
            e.prepare(sample_rate);
        }
        for s in [&mut self.supply, &mut self.tight, &mut self.input, &mut self.bass_keep, &mut self.tight_mix] {
            s.prepare(sample_rate);
        }
        self.reset();
    }

    fn reset(&mut self) {
        let v = self.v;
        self.gain.settle(v[p::GAIN]);
        self.gain.reset();
        self.tone.settle(v[p::TONE]);
        self.tone.reset();
        self.level.settle(v[p::LEVEL]);
        self.level.reset();
        let on = if v[p::TIGHT] > 20.5 { 1.0 } else { 0.0 };
        self.tight_mix.set_target(on);
        for s in [&mut self.supply, &mut self.tight, &mut self.input, &mut self.bass_keep, &mut self.tight_mix] {
            s.settle();
        }
        self.now = 0;
        self.tight_set = f64::NAN;
        self.split_set = f64::NAN;
        self.gains = (f32::NAN, f32::NAN, 1.0, 1.0);
        self.update_controls();
        self.active = 0;
        self.fade = 0.0;
        // every channel is put on the same state below, then linked (at the end)
        self.linked = false;
        self.same_run = 0;
        self.relink = 0.0;
        let cfg = self.target();
        self.start_slot(0, cfg);
        for c in &mut self.ch {
            c.tight.reset();
            c.dc.reset();
            for k in 0..2 {
                c.split_lo[k].reset();
                c.split_hi[k].reset();
            }
            for s in &mut c.slots {
                s.os.reset();
                s.twin_os.reset();
                for cc in &mut s.circuits {
                    cc.nc.reset_stats();
                }
            }
        }
        self.linked = self.ch.len() == 2 && !self.no_link;
    }

    fn set_param(&mut self, id: &str, value: f32) {
        let Some(i) = PARAMS.iter().position(|q| q.id == id) else { return };
        let q = &PARAMS[i];
        let v = value.clamp(q.min, q.max);
        self.v[i] = v;
        let now = self.now;
        match i {
            p::GAIN => self.gain.set_target(v, now),
            p::TONE => self.tone.set_target(v, now),
            p::LEVEL => self.level.set_target(v, now),
            p::SUPPLY => self.supply.set_target(v),
            p::TIGHT => self.tight.set_target(v),
            p::INPUT => self.input.set_target(v),
            p::BASS_KEEP => self.bass_keep.set_target(v),
            _ => {}
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(self.ch.len());
        if nch == 0 {
            return;
        }
        let n = buffers[0].len();
        let hold = (LINK_HOLD_MS * 0.001 * self.sr) as u32;
        let relink_step = 1.0 / (FADE_CIRCUIT_MS * 0.001 * self.sr);
        for i in 0..n {
            // ---- linked mono, decided per sample (block-size independent)
            if nch == 2 && !self.no_link {
                let same = buffers[0][i].to_bits() == buffers[1][i].to_bits();
                if self.linked {
                    if !same {
                        self.unlink();
                    }
                } else {
                    self.same_run = if same { self.same_run.saturating_add(1) } else { 0 };
                    if self.same_run >= hold {
                        self.relink += relink_step;
                    } else if self.relink > 0.0 {
                        self.relink = (self.relink - relink_step).max(0.0);
                    }
                    if self.relink >= 1.0 {
                        self.linked = true;
                        self.relink = 0.0;
                    }
                }
            }
            let live = self.live();
            // ---- discrete changes: a fade to the other slot
            if self.fade == 0.0 {
                let tgt = self.target();
                let cur = self.ch[0].slots[self.active].cfg;
                if tgt != cur {
                    let other = 1 - self.active;
                    let same = tgt.circuit == cur.circuit;
                    self.start_slot(other, tgt);
                    let ms = if same { FADE_PART_MS } else { FADE_CIRCUIT_MS };
                    self.fade_step = 1.0 / (ms * 0.001 * self.sr);
                    // the same circuit with other parts is correlated: linear; another circuit, equal-power
                    self.fade_power = !same;
                    self.fade = 1e-12;
                }
            }
            // ---- continuous controls
            let pots = !self.gain.is_settled() || !self.tone.is_settled();
            self.gain.next();
            self.tone.next();
            self.level.next();
            let sup = !self.supply.is_settled();
            self.supply.next();
            for s in [&mut self.tight, &mut self.input, &mut self.bass_keep] {
                s.next();
            }
            self.update_controls();
            let (g, t) = (self.gain.value() as f64 / 100.0, self.tone.value() as f64 / 100.0);
            let running = [self.active, 1 - self.active];
            let n_run = if self.fade > 0.0 { 2 } else { 1 };
            if pots || sup {
                let pct = self.supply.value() as f64 / 100.0;
                for c in self.ch.iter_mut().take(live) {
                    for &s in &running[..n_run] {
                        let slot = &mut c.slots[s];
                        let k = slot.cfg.circuit;
                        let cc = slot.cur();
                        if pots {
                            cc.set_pots(g, t);
                        }
                        if sup {
                            cc.set_supply(build_supply(k) * pct);
                        }
                    }
                }
            }
            // ---- gains
            let (inp, level) = (self.input.value(), self.level.value());
            if inp != self.gains.0 || level != self.gains.1 {
                self.gains = (inp, level, 10f64.powf(inp as f64 / 20.0), 10f64.powf(level as f64 / 20.0));
            }
            let in_g = self.gains.2;
            let k = self.ch[0].slots[self.active].cfg.circuit;
            let lg = self.gains.3;
            let kl = self.k_lin;
            let out_g = |k: usize, _: f32| lg * kl[k];
            let (wa, wb) = if self.fade > 0.0 {
                let x = self.fade.min(1.0);
                if self.fade_power {
                    let h = x * std::f64::consts::FRAC_PI_2;
                    (h.cos(), h.sin())
                } else {
                    (1.0 - x, x)
                }
            } else {
                (1.0, 0.0)
            };
            let k_other = self.ch[0].slots[1 - self.active].cfg.circuit;
            let (ga, gb) = (out_g(k, level), out_g(k_other, level));
            // `tight` at its bottom (20 Hz) is bypassed; in and out of the bypass in 5 ms (the filter
            // keeps running, so the switch has no step)
            self.tight_mix.set_target(if self.tight.value() > 20.5 { 1.0 } else { 0.0 });
            let tmix = self.tight_mix.next() as f64;
            let split = self.split_set > 0.5;
            for (ci, c) in self.ch.iter_mut().enumerate().take(nch.min(live)) {
                let mut x = buffers[ci][i] as f64 * in_g;
                let h = c.tight.process(x);
                if tmix > 0.0 {
                    x += tmix * (h - x);
                }
                let (hi, lo) = if split {
                    let xf = x as f32;
                    let l0 = c.split_lo[0].process(xf);
                    let lo = c.split_lo[1].process(l0);
                    let h0 = c.split_hi[0].process(xf);
                    let hi = c.split_hi[1].process(h0);
                    (hi as f64, Some(lo as f64))
                } else {
                    (x, None)
                };
                let mut y = 0.0;
                for (w, s, gk) in [(wa, running[0], ga), (wb, running[1], gb)] {
                    if w == 0.0 && s != self.active {
                        continue;
                    }
                    let slot = &mut c.slots[s];
                    let mut ys = slot.run(hi);
                    if let Some(l) = lo {
                        ys += slot.run_twin(l);
                    }
                    y += w * gk * ys;
                }
                let y = c.dc.process(y);
                buffers[ci][i] = y as f32;
            }
            if nch == 2 {
                if self.linked {
                    buffers[1][i] = buffers[0][i];
                } else if self.relink > 0.0 {
                    let (o0, o1) = (buffers[0][i] as f64, buffers[1][i] as f64);
                    buffers[1][i] = (o1 + self.relink * (o0 - o1)) as f32;
                }
            }
            if self.fade > 0.0 {
                self.fade += self.fade_step;
                if self.fade >= 1.0 {
                    self.fade = 0.0;
                    self.active = 1 - self.active;
                }
            }
            self.now += 1;
        }
    }
}

#[cfg(test)]
mod tests;
