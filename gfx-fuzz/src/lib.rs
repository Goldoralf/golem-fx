// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-fuzz — golem's fuzz: classic fuzz circuits solved as transistor networks, the guitar in front
//! of them included (design: README.md).
//!
//! - Each circuit is a netlist (circuits.rs) solved sample by sample by gfx-dsp's `circuit_nl`
//!   (nodal DK, damped Newton, Woodbury pot updates), oversampled per model through gfx-dsp's
//!   minimum-phase half-band cascade. The guitar (coil, pots, cable: gfx-dsp `pickup`) is in the
//!   netlist, driven by the coil's EMF rebuilt from the DI by the pickup's inverse filter; with
//!   `buffer` on, a buffer between the cable and the pedal, cut exactly (the guitar is then its own
//!   linear stage).
//! - `fuzz`, `guitar_vol`, `tone` (Si Four), `mids`, `bias`, `sag` are parts of the netlist;
//!   `voltage` is an input; `temp` and `leakage` are device constants.
//! - Per channel, one compiled circuit per (model, pickup, buffer, stack); two slots name the
//!   running one and the one a structural change crossfades in (30 ms, equal power, after 60 ms
//!   run unheard so its coupling capacitors settle on the signal), each with its own oversamplers,
//!   so models of different factors fade into each other.
//! - `level` 0 dB = unity loudness at the defaults on every model: a fixed K per model.

#![allow(clippy::needless_range_loop)]

use gfx_dsp::circuit_nl::*;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::oversample::{OversamplerN, MAX_FACTOR};
use gfx_dsp::pickup::{self, EmfFilter, Guitar, EMF_POLE_HZ};
use gfx_dsp::smooth::OnePole;
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::{Effect, ParamInfo};

pub mod circuits;
use circuits::*;

#[cfg(feature = "plugin")]
mod plugin;

/// Parameter indices (PARAMS order).
pub mod p {
    pub const FUZZ: usize = 0;
    pub const TONE: usize = 1;
    pub const LEVEL: usize = 2;
    pub const GUITAR_VOL: usize = 3;
    pub const MODEL: usize = 4;
    pub const BUFFER: usize = 5;
    pub const PICKUP: usize = 6;
    pub const BIAS: usize = 7;
    pub const TEMP: usize = 8;
    pub const LEAKAGE: usize = 9;
    pub const VOLTAGE: usize = 10;
    pub const SAG: usize = 11;
    pub const STACK: usize = 12;
    pub const MIDS: usize = 13;
    pub const INPUT: usize = 14;
    pub const MIX: usize = 15;
    pub const BASS_KEEP: usize = 16;
    pub const BYPASS: usize = 17;
}

pub const NP: usize = 18;

/// The circuits coded (README §2.3): the face in germanium and silicon, the muff. A `Config`'s
/// `model` is one of these.
pub const GE_PAIR: usize = 0;
pub const SI_PAIR: usize = 1;
pub const MUFF: usize = 2;
pub const KINDS: usize = 3;
pub const KIND_NAMES: [&str; KINDS] = ["Ge Pair", "Si Pair", "Si Four"];

/// The `model` options shipped, in the README's order, and the circuit each one runs. Si Pair is
/// coded but not shipped: at the 16× its aliases need it costs ≈ 9 µs per sample (README, As coded);
/// Ge Three and Octave Up wait for their netlists from a named document.
pub const MODELS: [&str; N_MODELS] = ["Ge Pair", "Si Four"];
pub const SHIPPED: [usize; N_MODELS] = [GE_PAIR, MUFF];
pub const N_MODELS: usize = 2;
pub const PICKUPS: [&str; 4] = ["Single Coil", "Humbucker", "P-90", "Active"];
pub const STACK_NAMES: [&str; 5] = ["Classic", "Early", "Triangle", "Russian", "Bypass"];

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::float("fuzz", "Fuzz", 0.0, 100.0, 70.0).with_exp(),
    ParamInfo::float("tone", "Tone", -100.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("level", "Level", -24.0, 24.0, 0.0).with_exp(),
    ParamInfo::float("guitar_vol", "Guitar Vol", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::enumeration("model", "Model", N_MODELS as u32, 0),
    ParamInfo::boolean("buffer", "Buffer", false),
    ParamInfo::enumeration("pickup", "Pickup", 4, 0),
    ParamInfo::float("bias", "Bias", -100.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("temp", "Temperature", 0.0, 50.0, 25.0),
    ParamInfo::float("leakage", "Leakage", 0.0, 400.0, 100.0),
    ParamInfo::float("voltage", "Voltage", 3.0, 18.0, 9.0).with_exp(),
    ParamInfo::float("sag", "Sag", 0.0, 100.0, 0.0),
    ParamInfo::enumeration("stack", "Tone Stack", 5, 0),
    ParamInfo::float("mids", "Mids", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("input", "Input", -12.0, 12.0, 0.0),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::float("bass_keep", "Bass Keep", 0.0, 500.0, 0.0),
    ParamInfo::boolean("bypass", "Bypass", false),
];

/// Half-band stages per circuit at 44.1 / 48 kHz (README §2.4; measured by
/// `tests::aliasing_meets_the_policy`): Ge Pair 8×, Si Pair 16×, Si Four 4×.
pub const STAGES: [usize; KINDS] = [3, 4, 2];

/// The sign that makes each model's small-signal mid-band gain positive (measured by
/// `tests::polarity_is_normalised`).
pub const POLARITY: [f64; KINDS] = [1.0, 1.0, 1.0];

/// K(model), dB: `level` 0 dB = the bypass loudness at the defaults (README §2.1; measured by
/// `tests::level_is_unity_at_the_defaults` on the DI proxy). "The hardware volume pot fully up" =
/// `level` −K. Si Pair's is not measured (not shipped).
pub const K_DB: [f64; KINDS] = [-2.5, 0.0, -11.3];

/// The guitar of each `pickup` option.
pub const GUITARS: [Guitar; 4] = [
    pickup::SINGLE_COIL,
    pickup::HUMBUCKER,
    pickup::P90,
    pickup::ACTIVE,
];

const MAX_CH: usize = 2;
const FADE_MS: f64 = 30.0;
const SETTLE_MS: f64 = 60.0;
const TILT_HZ: f64 = 800.0;

/// Everything a crossfade switches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub model: usize,
    pub pickup: usize,
    pub buffer: bool,
    /// The muff's stack (0 elsewhere).
    pub stack: usize,
}

impl Config {
    /// Every configuration of the shipped models (the pool's order).
    pub fn all() -> Vec<Config> {
        let mut v = Vec::new();
        for model in SHIPPED {
            let stacks = if model == MUFF { 5 } else { 1 };
            for stack in 0..stacks {
                for pickup in 0..4 {
                    for buffer in [false, true] {
                        v.push(Config {
                            model,
                            pickup,
                            buffer,
                            stack,
                        });
                    }
                }
            }
        }
        v
    }
}

/// The netlist of a configuration.
pub fn build(cfg: Config) -> Circuit {
    let g = &GUITARS[cfg.pickup];
    match cfg.model {
        0 => ge_pair(g, cfg.buffer),
        1 => si_pair(g, cfg.buffer),
        _ => si_four(cfg.stack, g, cfg.buffer),
    }
}

/// Half-band stages at host rate `sr` for a model with `base` stages at 44.1 / 48 kHz.
pub fn stages_at(base: usize, sr: f64) -> usize {
    if sr > 150_000.0 {
        base.saturating_sub(2)
    } else if sr > 75_000.0 {
        base.saturating_sub(1)
    } else {
        base
    }
}

/// The controls every circuit follows (plain units).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Controls {
    fuzz: f64,
    vol: f64,
    tone: f64,
    mids: f64,
    bias: f64,
    temp: f64,
    leak: f64,
    volts: f64,
    sag: f64,
}

/// One configuration compiled at its rate: the circuit, its EMF filter.
#[derive(Clone, Debug)]
struct Compiled {
    def: Circuit,
    nc: NonlinearCircuit,
    emf: EmfFilter,
    stages: usize,
}

impl Compiled {
    fn new(cfg: Config, sr: f64, force: Option<usize>) -> Self {
        let def = build(cfg);
        let stages = force.unwrap_or_else(|| stages_at(STAGES[cfg.model], sr));
        let fs = sr * (1usize << stages) as f64;
        let mut nc = NonlinearCircuit::new(&def.net, fs);
        nc.set_fast_solve(true);
        nc.set_quadratic_seed(std::env::var("NOQUAD").is_err());
        let g = &GUITARS[cfg.pickup];
        let emf = g.emf_filter(fs, EMF_POLE_HZ);
        Self {
            def,
            nc,
            emf,
            stages,
        }
    }

    /// Applies the controls; `structural` false = only what changed since `last`.
    fn apply(&mut self, c: &Controls, last: Option<&Controls>) {
        let ch = |f: fn(&Controls) -> f64| last.is_none_or(|l| f(l) != f(c));
        let d = &self.def;
        if ch(|c| c.fuzz) {
            self.nc.set_pot(d.fuzz, c.fuzz);
        }
        if ch(|c| c.vol) {
            self.nc.set_pot(d.volume, c.vol);
        }
        if ch(|c| c.tone) {
            if let Some(t) = d.tone {
                self.nc.set_pot(t, 0.5 * (c.tone + 1.0));
            }
        }
        if ch(|c| c.mids) {
            if let Some(m) = d.mids {
                self.nc.set_var(m, (22e3 * c.mids).max(R_MIN));
            }
        }
        if ch(|c| c.bias) {
            self.nc.set_var(d.bias.0, d.bias.1 * (-c.bias).exp2());
        }
        if ch(|c| c.sag) {
            self.nc.set_var(d.sag, (SAG_MAX * c.sag).max(R_MIN));
        }
        if ch(|c| c.volts) {
            self.nc.set_input(IN_VS, d.supply_sign * c.volts);
        }
        if ch(|c| c.temp) {
            self.nc.set_temperature(c.temp);
        }
        if ch(|c| c.leak) {
            for &(dev, m) in &d.ge {
                self.nc
                    .set_device(dev, DeviceModel::Bjt(BjtModel { leak: c.leak, ..m }));
            }
        }
    }
}

/// First-order tilt around 800 Hz: low band × g⁻¹, high band × g (TPT one-pole split; g = 1 is
/// the identity).
#[derive(Clone, Copy, Debug, Default)]
struct Tilt {
    k: f64,
    s: f64,
}

impl Tilt {
    fn set(&mut self, sr: f64) {
        let g = (std::f64::consts::PI * TILT_HZ / sr).tan();
        self.k = g / (1.0 + g);
    }
    #[inline]
    fn process(&mut self, x: f64, gl: f64, gh: f64) -> f64 {
        let v = self.k * (x - self.s);
        let lp = v + self.s;
        self.s = lp + v;
        lp * gl + (x - lp) * gh
    }
}

/// First-order high-pass (prewarped bilinear): the 5 Hz DC guard.
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
}

/// A running configuration of one channel: its oversamplers and post filters.
#[derive(Clone, Debug)]
struct Slot {
    cfg: Config,
    /// The configuration's place in the channel's pool.
    idx: usize,
    os: OversamplerN,
    dry_os: OversamplerN,
    tilt: Tilt,
    split_lo: [Svf; 2],
    split_hi: [Svf; 2],
}

impl Slot {
    fn new() -> Self {
        Self {
            cfg: Config {
                model: GE_PAIR,
                pickup: 0,
                buffer: false,
                stack: 0,
            },
            idx: 0,
            os: OversamplerN::new(0),
            dry_os: OversamplerN::new(0),
            tilt: Tilt::default(),
            split_lo: [Svf::new(), Svf::new()],
            split_hi: [Svf::new(), Svf::new()],
        }
    }
}

#[derive(Clone, Debug)]
struct Chan {
    pool: Vec<Compiled>,
    slots: [Slot; 2],
    dc: HighPass,
    bias_v: f64,
}

pub struct Fuzz {
    sr: f64,
    v: [f32; NP],
    ch: Vec<Chan>,
    now: u64,
    fuzz: ExpSmoother,
    tone: ExpSmoother,
    level: ExpSmoother,
    vol: ExpSmoother,
    mids: ExpSmoother,
    mix: ExpSmoother,
    bias: OnePole,
    temp: OnePole,
    leak: OnePole,
    volts: OnePole,
    sag: OnePole,
    input: OnePole,
    bass_keep: OnePole,
    bypass: OnePole,
    active: usize,
    fade: f64,
    fade_step: f64,
    /// Host samples the incoming slot runs unheard before its fade (its coupling capacitors settle
    /// on the signal: the blocking transient of a circuit started from rest is not heard).
    settle: u32,
    split_set: f64,
    last: Option<Controls>,
    /// (input dB, level dB, input gain, level gain).
    gains: (f32, f32, f64, f64),
    meter_coef: f64,
    supply_v: f64,
    k_lin: [f64; KINDS],
    /// The pool's configurations (its order).
    configs: Vec<Config>,
    dry_on: bool,
    /// Tests: forces the half-band stage count of every model (the alias measurements).
    #[doc(hidden)]
    pub stages_override: Option<usize>,
}

impl Fuzz {
    pub fn new() -> Self {
        let v: [f32; NP] = std::array::from_fn(|i| PARAMS[i].default);
        let exp = |i: usize| ExpSmoother::new(PARAMS[i].min, PARAMS[i].max, PARAMS[i].default);
        let pole = |i: usize, ms: f32| OnePole::new(ms, PARAMS[i].default, 1e-4);
        Self {
            sr: 48000.0,
            v,
            ch: Vec::new(),
            now: 0,
            fuzz: exp(p::FUZZ),
            tone: exp(p::TONE),
            level: exp(p::LEVEL),
            vol: exp(p::GUITAR_VOL),
            mids: exp(p::MIDS),
            mix: exp(p::MIX),
            bias: pole(p::BIAS, 300.0),
            temp: pole(p::TEMP, 300.0),
            leak: pole(p::LEAKAGE, 300.0),
            volts: pole(p::VOLTAGE, 100.0),
            sag: pole(p::SAG, 100.0),
            input: pole(p::INPUT, 100.0),
            bass_keep: OnePole::new(30.0, 0.0, 1e-2),
            bypass: OnePole::new(10.0, 0.0, 1e-6),
            active: 0,
            fade: 0.0,
            fade_step: 0.0,
            settle: 0,
            split_set: f64::NAN,
            last: None,
            gains: (f32::NAN, f32::NAN, 1.0, 1.0),
            meter_coef: 0.0,
            supply_v: 0.0,
            k_lin: std::array::from_fn(|k| 10f64.powf(K_DB[k] / 20.0)),
            configs: Config::all(),
            dry_on: false,
            stages_override: None,
        }
    }

    fn target(&self) -> Config {
        let model = SHIPPED[(self.v[p::MODEL].round() as usize).min(N_MODELS - 1)];
        Config {
            model,
            pickup: (self.v[p::PICKUP].round() as usize).min(3),
            buffer: self.v[p::BUFFER] > 0.5,
            stack: if model == MUFF {
                (self.v[p::STACK].round() as usize).min(4)
            } else {
                0
            },
        }
    }

    fn controls(&self) -> Controls {
        Controls {
            fuzz: self.fuzz.value() as f64 / 100.0,
            vol: self.vol.value() as f64 / 100.0,
            tone: self.tone.value() as f64 / 100.0,
            mids: self.mids.value() as f64 / 100.0,
            bias: self.bias.value() as f64 / 100.0,
            temp: self.temp.value() as f64,
            leak: self.leak.value() as f64 / 100.0,
            volts: self.volts.value() as f64,
            sag: self.sag.value() as f64 / 100.0,
        }
    }

    /// Puts slot `s` of every channel on `cfg` at its DC point with the current controls.
    fn start_slot(&mut self, s: usize, cfg: Config) {
        let c = self.controls();
        let sr = self.sr;
        let idx = self
            .configs
            .iter()
            .position(|x| *x == cfg)
            .expect("a shipped configuration");
        for ch in &mut self.ch {
            let cc = &mut ch.pool[idx];
            cc.apply(&c, None);
            cc.nc.set_input(IN_SIG, 0.0);
            cc.nc.solve_dc();
            cc.emf.reset();
            let st = cc.stages;
            let slot = &mut ch.slots[s];
            slot.cfg = cfg;
            slot.idx = idx;
            slot.os.set_stages(st);
            slot.dry_os.set_stages(st);
            slot.tilt = Tilt::default();
            slot.tilt.set(sr);
            for k in 0..2 {
                slot.split_lo[k].reset();
                slot.split_hi[k].reset();
            }
        }
        self.split_set = f64::NAN;
    }

    /// The trimmed transistor's collector voltage (magnitude, 50 ms average): the `bias_v` meter.
    pub fn bias_v(&self) -> f32 {
        self.ch.first().map_or(0.0, |c| c.bias_v as f32)
    }

    /// The circuit's supply now (sag included): the `supply_v` meter.
    pub fn supply_v(&self) -> f32 {
        self.supply_v as f32
    }

    /// Solver statistics of the running circuit of channel 0 (tests, bench).
    pub fn stats(&self) -> Stats {
        let c = &self.ch[0];
        c.pool[c.slots[self.active].idx].nc.stats()
    }

    /// The running circuit of channel 0 (tests).
    #[doc(hidden)]
    pub fn circuit(&self) -> &NonlinearCircuit {
        let c = &self.ch[0];
        &c.pool[c.slots[self.active].idx].nc
    }
}

impl Default for Fuzz {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Fuzz {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate as f64;
        let n = channels.clamp(1, MAX_CH);
        // every configuration compiled once, its DC seed solved at the default controls
        let defaults = Controls {
            fuzz: 0.7,
            vol: 1.0,
            tone: 0.0,
            mids: 0.0,
            bias: 0.0,
            temp: 25.0,
            leak: 1.0,
            volts: 9.0,
            sag: 0.0,
        };
        let pool: Vec<Compiled> = Config::all()
            .into_iter()
            .map(|cfg| {
                let mut c = Compiled::new(cfg, self.sr, self.stages_override);
                c.apply(&defaults, None);
                c.nc.init_dc();
                c
            })
            .collect();
        let mut dc = HighPass::default();
        dc.set(5.0, self.sr);
        self.ch = (0..n)
            .map(|_| Chan {
                pool: pool.clone(),
                slots: [Slot::new(), Slot::new()],
                dc,
                bias_v: 0.0,
            })
            .collect();
        for e in [
            &mut self.fuzz,
            &mut self.tone,
            &mut self.level,
            &mut self.vol,
            &mut self.mids,
            &mut self.mix,
        ] {
            e.prepare(sample_rate);
        }
        for s in [
            &mut self.bias,
            &mut self.temp,
            &mut self.leak,
            &mut self.volts,
            &mut self.sag,
            &mut self.input,
            &mut self.bass_keep,
            &mut self.bypass,
        ] {
            s.prepare(sample_rate);
        }
        self.meter_coef = 1.0 - (-1.0 / (0.05 * self.sr)).exp();
        self.reset();
    }

    fn reset(&mut self) {
        let v = self.v;
        for (e, i) in [
            (&mut self.fuzz, p::FUZZ),
            (&mut self.tone, p::TONE),
            (&mut self.level, p::LEVEL),
            (&mut self.vol, p::GUITAR_VOL),
            (&mut self.mids, p::MIDS),
            (&mut self.mix, p::MIX),
        ] {
            e.settle(v[i]);
            e.reset();
        }
        for s in [
            &mut self.bias,
            &mut self.temp,
            &mut self.leak,
            &mut self.volts,
            &mut self.sag,
            &mut self.input,
            &mut self.bass_keep,
            &mut self.bypass,
        ] {
            s.settle();
        }
        self.now = 0;
        self.gains = (f32::NAN, f32::NAN, 1.0, 1.0);
        self.dry_on = false;
        self.active = 0;
        self.fade = 0.0;
        self.settle = 0;
        self.last = None;
        if self.ch.is_empty() {
            return;
        }
        let cfg = self.target();
        self.start_slot(0, cfg);
        self.last = Some(self.controls());
        let sr = self.sr;
        for c in &mut self.ch {
            c.dc.x1 = 0.0;
            c.dc.y1 = 0.0;
            c.dc.set(5.0, sr);
            for s in &mut c.slots {
                s.os.reset();
                s.dry_os.reset();
            }
            let cc = &mut c.pool[c.slots[0].idx];
            cc.nc.reset_stats();
            c.bias_v = cc.nc.probe(cc.def.bias_meter).abs();
            self.supply_v = cc.nc.probe(cc.def.supply_meter).abs();
        }
    }

    fn set_param(&mut self, id: &str, value: f32) {
        let Some(i) = PARAMS.iter().position(|q| q.id == id) else {
            return;
        };
        let q = &PARAMS[i];
        let v = value.clamp(q.min, q.max);
        self.v[i] = v;
        let now = self.now;
        match i {
            p::FUZZ => self.fuzz.set_target(v, now),
            p::TONE => self.tone.set_target(v, now),
            p::LEVEL => self.level.set_target(v, now),
            p::GUITAR_VOL => self.vol.set_target(v, now),
            p::MIDS => self.mids.set_target(v, now),
            p::MIX => self.mix.set_target(v, now),
            p::BIAS => self.bias.set_target(v),
            p::TEMP => self.temp.set_target(v),
            p::LEAKAGE => self.leak.set_target(v),
            p::VOLTAGE => self.volts.set_target(v),
            p::SAG => self.sag.set_target(v),
            p::INPUT => self.input.set_target(v),
            p::BASS_KEEP => self.bass_keep.set_target(v),
            p::BYPASS => self.bypass.set_target(if v > 0.5 { 1.0 } else { 0.0 }),
            _ => {}
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(self.ch.len());
        if nch == 0 {
            return;
        }
        let n = buffers[0].len();
        let sr = self.sr;
        for i in 0..n {
            // ---- structural changes: a fade to the other slot
            if self.fade == 0.0 {
                let tgt = self.target();
                if tgt != self.ch[0].slots[self.active].cfg {
                    self.start_slot(1 - self.active, tgt);
                    self.fade_step = 1.0 / (FADE_MS * 0.001 * sr);
                    self.settle = (SETTLE_MS * 0.001 * sr) as u32;
                    self.fade = 1e-12;
                }
            }
            // ---- continuous controls
            for e in [
                &mut self.fuzz,
                &mut self.tone,
                &mut self.level,
                &mut self.vol,
                &mut self.mids,
                &mut self.mix,
            ] {
                e.next();
            }
            for s in [
                &mut self.bias,
                &mut self.temp,
                &mut self.leak,
                &mut self.volts,
                &mut self.sag,
                &mut self.input,
                &mut self.bass_keep,
                &mut self.bypass,
            ] {
                s.next();
            }
            let running = [self.active, 1 - self.active];
            let n_run = if self.fade > 0.0 { 2 } else { 1 };
            let c = self.controls();
            if self.last != Some(c) || n_run == 2 {
                let last = self.last;
                for ch in &mut self.ch {
                    for &s in &running[..n_run] {
                        let idx = ch.slots[s].idx;
                        ch.pool[idx].apply(&c, last.as_ref());
                    }
                }
                self.last = Some(c);
            }
            let bk = self.bass_keep.value() as f64;
            if (bk - self.split_set).abs() > 0.05 || self.split_set.is_nan() {
                self.split_set = bk;
                if bk > 0.5 {
                    for ch in &mut self.ch {
                        for s in &mut ch.slots {
                            for k in 0..2 {
                                s.split_lo[k].set(
                                    SvfMode::LowPass,
                                    bk as f32,
                                    std::f32::consts::FRAC_1_SQRT_2,
                                    0.0,
                                    sr as f32,
                                );
                                s.split_hi[k].set(
                                    SvfMode::HighPass,
                                    bk as f32,
                                    std::f32::consts::FRAC_1_SQRT_2,
                                    0.0,
                                    sr as f32,
                                );
                            }
                        }
                    }
                }
            }
            let split = self.split_set > 0.5;
            // ---- gains
            let (inp, level) = (self.input.value(), self.level.value());
            if inp != self.gains.0 || level != self.gains.1 {
                self.gains = (
                    inp,
                    level,
                    10f64.powf(inp as f64 / 20.0),
                    10f64.powf(level as f64 / 20.0),
                );
            }
            let (in_g, lg) = (self.gains.2, self.gains.3);
            let mix = self.mix.value() as f64 / 100.0;
            // the dry path runs only while something hears it; it restarts from rest, its weight
            // then still ≈ 0 (the mix glides from 100 %)
            let need_dry = mix < 1.0 || split;
            if need_dry && !self.dry_on {
                for ch in &mut self.ch {
                    for s in &mut ch.slots {
                        s.dry_os.reset();
                    }
                }
            }
            self.dry_on = need_dry;
            let td = 6.0 * self.tone.value() as f64 / 100.0;
            let (gl, gh) = (10f64.powf(-td / 20.0), 10f64.powf(td / 20.0));
            let (wa, wb) = if self.settle > 0 {
                (1.0, 0.0)
            } else if self.fade > 0.0 {
                let h = self.fade.min(1.0) * std::f64::consts::FRAC_PI_2;
                (h.cos(), h.sin())
            } else {
                (1.0, 0.0)
            };
            let byp = self.bypass.value() as f64;
            let mc = self.meter_coef;
            let active = self.active;
            let k_lin = self.k_lin;
            for (ci, ch) in self.ch.iter_mut().enumerate().take(nch) {
                let x = buffers[ci][i] as f64;
                let xin = x * in_g;
                let mut y = 0.0;
                for (w, s) in [(wa, running[0]), (wb, running[1])].into_iter().take(n_run) {
                    if w == 0.0 && s == active {
                        continue;
                    }
                    let slot = &mut ch.slots[s];
                    let model = slot.cfg.model;
                    let cc = &mut ch.pool[slot.idx];
                    let mut buf = [0.0; MAX_FACTOR];
                    slot.os.up(xin, &mut buf);
                    let nf = 1usize << cc.stages;
                    for v in buf.iter_mut().take(nf) {
                        let e = cc.emf.process(*v);
                        cc.nc.set_input(IN_SIG, e);
                        cc.nc.process();
                        *v = cc.nc.output(cc.def.out);
                    }
                    let mut f = slot.os.down(&mut buf) * POLARITY[model] * k_lin[model];
                    let dry = if need_dry {
                        slot.dry_os.identity(x)
                    } else {
                        0.0
                    };
                    if model != MUFF && td != 0.0 {
                        f = slot.tilt.process(f, gl, gh);
                    } else if model != MUFF {
                        slot.tilt.process(f, 1.0, 1.0);
                    }
                    if split {
                        let (h0, d0) = (
                            slot.split_hi[0].process(f as f32),
                            slot.split_lo[0].process(dry as f32),
                        );
                        f = slot.split_hi[1].process(h0) as f64
                            + slot.split_lo[1].process(d0) as f64;
                    }
                    y += w * (mix * f + (1.0 - mix) * dry);
                    if s == active {
                        ch.bias_v += mc * (cc.nc.probe(cc.def.bias_meter).abs() - ch.bias_v);
                        if ci == 0 {
                            self.supply_v = cc.nc.probe(cc.def.supply_meter).abs();
                        }
                    }
                }
                let y = ch.dc.process(lg * y);
                let out = y + byp * (x - y);
                buffers[ci][i] = if out.is_finite() { out as f32 } else { 0.0 };
            }
            if self.settle > 0 {
                self.settle -= 1;
            } else if self.fade > 0.0 {
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
