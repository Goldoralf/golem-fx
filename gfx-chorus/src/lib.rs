// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-chorus — golem's chorus (design: README.md, PARAMS.md, VOICING.md).
//!
//! Stands for golem's `chorus` block (ids `rate`, `depth` = peak-to-peak delay swing in ms, `mix` = wet
//! level with dry at unity). Profiles voice it per instrument. Modes are reference units: CE-2, Juno-60
//! I / II / I+II, Dimension D 1 / 2 / 3, string ensemble, doubler.
//!
//! Signal path:
//! - crossover: dry mono lows;
//! - drive → compressor → pre-emphasis;
//! - per voice: Clean (Lagrange / sinc), BBD (clock-domain, gfx-dsp) or Tape (wow and flutter);
//! - de-emphasis → expander → wet filters → cross-mix → width / mono-safe → mix law → level comp.
//!
//! Structural changes (profile, mode, engine, chip, shape, voices, crossover on / slope, mono-safe) dip
//! the wet for 20 ms, switch, and come back over 20 ms. The dry never moves. Continuous parameters glide.

#![allow(clippy::needless_range_loop)]

pub mod voicing;

use gfx_dsp::adaa::{Adaa1, Shaper};
use gfx_dsp::bbd::Bbd;
use gfx_dsp::delay::{DelayLine, Interp, SincTable};
use gfx_dsp::envelope::PeakFollower;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::lfo::Rng;
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::transport::{Sync, TransportInfo};
use gfx_dsp::util::one_pole_coef;
use gfx_dsp::{Effect, ParamInfo};
use voicing::{c, Engine, Structure, LOG, N_COLOUR, RANDOM, ROUNDED, SINE, TRIANGLE};

#[cfg(feature = "plugin")]
mod plugin;

const MAX_VOICES: usize = 6;
const MAX_DELAY_MS: f64 = 40.0;
const CTRL: u32 = 32;

/// Parameter indices (PARAMS order).
pub mod p {
    pub const PROFILE: usize = 0;
    pub const MODE: usize = 1;
    pub const RATE: usize = 2;
    pub const DEPTH: usize = 3;
    pub const WARMTH: usize = 4;
    pub const MIX: usize = 5;
    pub const XOVER_CUSTOM: usize = 6;
    pub const XOVER_ON: usize = 7;
    pub const XOVER_FREQ: usize = 8;
    pub const XOVER_SLOPE: usize = 9;
    pub const COLOR_CUSTOM: usize = 10;
    pub const ENGINE: usize = 11;
    pub const INTERP: usize = 12;
    pub const DRIVE: usize = 13;
    pub const BBD_CHIP: usize = 14;
    pub const BBD_BW: usize = 15;
    pub const EMPHASIS: usize = 16;
    pub const COMPANDER: usize = 17;
    pub const NOISE: usize = 18;
    pub const WOW: usize = 19;
    pub const FLUTTER: usize = 20;
    pub const MOD_CUSTOM: usize = 21;
    pub const DELAY_BASE: usize = 22;
    pub const LFO_SHAPE: usize = 23;
    pub const VOICES: usize = 24;
    pub const VOICE_SPREAD: usize = 25;
    pub const IMPERFECTION: usize = 26;
    pub const DETUNE_LOCK: usize = 27;
    pub const DRIFT: usize = 28;
    pub const CROSSMIX: usize = 29;
    pub const FEEDBACK: usize = 30;
    pub const SYNC: usize = 31;
    pub const ENV_DEPTH: usize = 32;
    pub const ENV_MIX: usize = 33;
    pub const STEREO_CUSTOM: usize = 34;
    pub const STEREO_PHASE: usize = 35;
    pub const WIDTH: usize = 36;
    pub const MONO_SAFE: usize = 37;
    pub const FILTER_CUSTOM: usize = 38;
    pub const WET_HPF: usize = 39;
    pub const WET_LPF: usize = 40;
    pub const MIX_LAW: usize = 41;
    pub const LEVEL_COMP: usize = 42;
    pub const OUTPUT: usize = 43;
    pub const N: usize = 44;
}

/// Expert defaults are Guitar Clean / Classic's anchors, so switching a group to Custom keeps the sound.
pub static PARAMS: [ParamInfo; p::N] = [
    ParamInfo::enumeration("profile", "Profile", 5, 0),
    ParamInfo::enumeration("mode", "Mode", 9, 0),
    ParamInfo::float("rate", "Rate", 0.05, 10.0, 0.6).with_exp(),
    ParamInfo::float("depth", "Depth", 0.1, 12.0, 4.0).with_exp(),
    ParamInfo::float("warmth", "Warmth", 0.0, 100.0, 50.0).with_exp(),
    ParamInfo::float("mix", "Mix", 0.0, 1.0, 0.4).with_exp(),
    ParamInfo::boolean("xover_custom", "Crossover Custom", false),
    ParamInfo::boolean("xover_on", "Crossover", false),
    ParamInfo::float("xover_freq", "Crossover Freq", 40.0, 500.0, 120.0),
    ParamInfo::enumeration("xover_slope", "Crossover Slope", 2, 1),
    ParamInfo::boolean("color_custom", "Color Custom", false),
    ParamInfo::enumeration("engine", "Engine", 3, 1),
    ParamInfo::enumeration("interp", "Interpolation", 3, 1),
    ParamInfo::float("drive", "Drive", 0.0, 100.0, 25.0),
    ParamInfo::enumeration("bbd_chip", "BBD Chip", 5, 2),
    ParamInfo::float("bbd_bw", "BBD Bandwidth", 2000.0, 15000.0, 6600.0),
    ParamInfo::float("emphasis", "Emphasis", 0.0, 100.0, 100.0),
    ParamInfo::float("compander", "Compander", 0.0, 100.0, 0.0),
    ParamInfo::float("noise", "Noise", 0.0, 100.0, 15.0),
    ParamInfo::float("wow", "Wow", 0.0, 100.0, 0.0),
    ParamInfo::float("flutter", "Flutter", 0.0, 100.0, 0.0),
    ParamInfo::boolean("mod_custom", "Modulation Custom", false),
    ParamInfo::float("delay_base", "Base Delay", 1.0, 30.0, 8.0),
    ParamInfo::enumeration("lfo_shape", "LFO Shape", 5, 1),
    ParamInfo::enumeration("voices", "Voices", 6, 0),
    ParamInfo::float("voice_spread", "Voice Spread", 0.0, 360.0, 90.0),
    ParamInfo::float("imperfection", "Imperfection", 0.0, 100.0, 10.0),
    ParamInfo::boolean("detune_lock", "Detune Lock", false),
    ParamInfo::float("drift", "Drift", 0.0, 100.0, 10.0),
    ParamInfo::float("crossmix", "Cross-Mix", 0.0, 100.0, 0.0),
    ParamInfo::float("feedback", "Feedback", 0.0, 50.0, 0.0),
    ParamInfo::enumeration("sync", "Sync", 17, 0),
    ParamInfo::float("env_depth", "Env → Depth", -100.0, 100.0, 0.0),
    ParamInfo::float("env_mix", "Env → Mix", -100.0, 100.0, 0.0),
    ParamInfo::boolean("stereo_custom", "Stereo Custom", false),
    ParamInfo::float("stereo_phase", "Stereo Phase", 0.0, 180.0, 90.0),
    ParamInfo::float("width", "Width", 0.0, 150.0, 100.0),
    ParamInfo::boolean("mono_safe", "Mono Safe", false),
    ParamInfo::boolean("filter_custom", "Filter Custom", false),
    ParamInfo::float("wet_hpf", "Wet Low Cut", 20.0, 1000.0, 80.0),
    ParamInfo::float("wet_lpf", "Wet High Cut", 1000.0, 20000.0, 8000.0),
    ParamInfo::enumeration("mix_law", "Mix Law", 3, 0),
    ParamInfo::boolean("level_comp", "Level Comp", true),
    ParamInfo::float("output", "Output", -12.0, 12.0, 0.0),
];

/// All settings, in PARAMS order (bools and enums as numbers).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub v: [f32; p::N],
}

impl Default for Settings {
    fn default() -> Self {
        Self { v: core::array::from_fn(|i| PARAMS[i].default) }
    }
}

impl Settings {
    pub fn with(mut self, i: usize, value: f32) -> Self {
        self.v[i] = value;
        self
    }
}

#[derive(Clone)]
struct Voice {
    line: DelayLine,
    bbd: Bbd,
    fb: f32,
    /// fixed tolerances (imperfection): phase (cycles) and depth errors
    phase_err: f64,
    amp_err: f32,
    /// random shape and drift
    rng: Rng,
    rnd_from: f64,
    rnd_to: f64,
    last_phase: f64,
    drift_from: f64,
    drift_to: f64,
    drift_ph: f64,
    /// RC smoothing of the triangle (CE-2: 72 Hz)
    rc: f64,
    delay_ms: f64,
}

impl Voice {
    fn new(seed: u64) -> Self {
        Self {
            line: DelayLine::new(),
            bbd: Bbd::with_capacity(4096, 1024),
            fb: 0.0,
            phase_err: 0.0,
            amp_err: 1.0,
            rng: Rng::new(seed),
            rnd_from: 0.0,
            rnd_to: 0.0,
            last_phase: 0.0,
            drift_from: 0.0,
            drift_to: 0.0,
            drift_ph: 0.0,
            rc: 0.0,
            delay_ms: 0.0,
        }
    }

    fn reset(&mut self, chip: usize) {
        self.line.reset();
        self.bbd.set_stages(chip);
        self.bbd.reset();
        self.fb = 0.0;
        self.rng.reset();
        // tolerances are drawn once per reset from the voice's own seed (deterministic)
        self.phase_err = self.rng.bipolar() as f64;
        self.amp_err = self.rng.bipolar();
        self.rnd_from = 0.0;
        self.rnd_to = self.rng.bipolar() as f64;
        self.last_phase = 0.0;
        self.drift_from = 0.0;
        self.drift_to = self.rng.bipolar() as f64;
        self.drift_ph = 0.0;
        self.rc = 0.0;
    }
}

#[inline]
fn shape_value(p: f64, shape: u32, v: &Voice) -> f64 {
    match shape {
        SINE => (std::f64::consts::TAU * p).sin(),
        ROUNDED => (0.97 * (std::f64::consts::TAU * p).sin()).asin() / 0.97f64.asin(),
        RANDOM => v.rnd_from + (v.rnd_to - v.rnd_from) * 0.5 * (1.0 - (std::f64::consts::PI * p).cos()),
        LOG => {
            // Strymon-style: a triangle mapped through an exponential (more time near the short end)
            let t = 1.0 - 4.0 * ((p + 0.25).rem_euclid(1.0) - 0.5).abs();
            let a = 2.5;
            2.0 * (((a * (t + 1.0) * 0.5).exp() - 1.0) / (a.exp() - 1.0)) - 1.0
        }
        _ => 1.0 - 4.0 * ((p + 0.25).rem_euclid(1.0) - 0.5).abs(),
    }
}

/// Wet-side state per output channel.
#[derive(Clone, Default)]
struct Out {
    deemph: Svf,
    hp: Svf,
    lp: Svf,
    cross_hp: Svf,
    exp_env: f32,
}

pub struct Chorus {
    s: Settings,
    sr: f32,
    st: Structure,
    pending: Option<Structure>,
    gate: LinearRamp,
    voices: Vec<Voice>,
    sinc: SincTable,
    out: [Out; 2],
    // dry side: crossover per channel (two cascaded sections) and the crossover blend
    xo: [[Svf; 2]; 2],
    xo_hp: [[Svf; 2]; 2],
    xo_amt: OnePole,
    // wet input
    adaa: Adaa1,
    emph: Svf,
    comp_env: f32,
    // follower
    env: PeakFollower,
    slow_db: f32,
    // modulation
    phase: f64,
    fast_phase: f64,
    transport: TransportInfo,
    rate: ExpSmoother,
    depth: ExpSmoother,
    warmth: ExpSmoother,
    mix: ExpSmoother,
    cont: [OnePole; N_COLOUR],
    feedback: OnePole,
    output: OnePole,
    gain_dry: OnePole,
    gain_wet: OnePole,
    gain_lc: OnePole,
    tape_rng: Rng,
    wow_ph: f64,
    flutter_ph: f64,
    ctrl: u32,
    clock: u64,
}

impl Chorus {
    pub fn new() -> Self {
        let s = Settings::default();
        let st = voicing::structure(&s.v);
        let col = voicing::colour(&s.v, 0.5).to_array();
        Self {
            s,
            sr: 48000.0,
            st,
            pending: None,
            gate: LinearRamp::new(20.0, 1.0),
            voices: (0..MAX_VOICES).map(|i| Voice::new(0xC0 + i as u64)).collect(),
            sinc: SincTable::new(),
            out: Default::default(),
            xo: Default::default(),
            xo_hp: Default::default(),
            xo_amt: OnePole::new(20.0, if st.xover_on { 1.0 } else { 0.0 }, 1e-5),
            adaa: Adaa1::new(),
            emph: Svf::new(),
            comp_env: 0.1,
            env: PeakFollower::new(5.0, 150.0),
            slow_db: -40.0,
            phase: 0.0,
            fast_phase: 0.0,
            transport: TransportInfo::default(),
            rate: ExpSmoother::new(0.05, 10.0, s.v[p::RATE]),
            depth: ExpSmoother::new(0.1, 12.0, s.v[p::DEPTH]),
            warmth: ExpSmoother::new(0.0, 100.0, s.v[p::WARMTH]),
            mix: ExpSmoother::new(0.0, 1.0, s.v[p::MIX]),
            // control-rate smoothers (prepared at sr / CTRL)
            cont: core::array::from_fn(|i| OnePole::new(if i == c::CENTRE { 100.0 } else { 30.0 }, col[i], 1e-6)),
            feedback: OnePole::new(30.0, 0.0, 1e-6),
            output: OnePole::new(20.0, 0.0, 1e-5),
            gain_dry: OnePole::new(10.0, 1.0, 1e-6),
            gain_wet: OnePole::new(10.0, 0.4, 1e-6),
            gain_lc: OnePole::new(10.0, 1.0, 1e-6),
            tape_rng: Rng::new(0x7A9E),
            wow_ph: 0.0,
            flutter_ph: 0.0,
            ctrl: 0,
            clock: 0,
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn structure(&self) -> Structure {
        self.st
    }

    /// The voices' current delays (ms), for tests.
    pub fn delays_ms(&self) -> [f64; MAX_VOICES] {
        core::array::from_fn(|i| self.voices[i].delay_ms)
    }

    pub fn set(&mut self, s: Settings) {
        let v = &s.v;
        if v[p::RATE] != self.s.v[p::RATE] {
            self.rate.set_target(v[p::RATE].clamp(0.05, 10.0), self.clock);
        }
        if v[p::DEPTH] != self.s.v[p::DEPTH] {
            self.depth.set_target(v[p::DEPTH].clamp(0.1, 12.0), self.clock);
        }
        if v[p::WARMTH] != self.s.v[p::WARMTH] {
            self.warmth.set_target(v[p::WARMTH].clamp(0.0, 100.0), self.clock);
        }
        if v[p::MIX] != self.s.v[p::MIX] {
            self.mix.set_target(v[p::MIX].clamp(0.0, 1.0), self.clock);
        }
        self.feedback.set_target(v[p::FEEDBACK].clamp(0.0, 50.0) * 0.01);
        self.output.set_target(v[p::OUTPUT]);
        let st = voicing::structure(v);
        if st != self.st || self.pending.is_some() {
            if st != self.st {
                self.pending = Some(st);
                self.gate.set_target(0.0);
            } else {
                self.pending = None;
                self.gate.set_target(1.0);
            }
        }
        self.xo_amt.set_target(if st.xover_on { 1.0 } else { 0.0 });
        self.s = s;
    }

    fn apply_structure(&mut self, st: Structure) {
        let sr = self.sr;
        self.st = st;
        for v in self.voices.iter_mut() {
            v.reset(st.chip);
            v.bbd.set_delay(0.005);
        }
        self.set_xover(self.cont[c::XOVER].value(), sr);
    }

    fn set_xover(&mut self, f: f32, sr: f32) {
        let q = if self.st.xover_24 { std::f32::consts::FRAC_1_SQRT_2 } else { 0.5 };
        for ch in self.xo.iter_mut() {
            for s in ch.iter_mut() {
                s.set(SvfMode::LowPass, f, q, 0.0, sr);
            }
        }
        for ch in self.xo_hp.iter_mut() {
            for s in ch.iter_mut() {
                s.set(SvfMode::HighPass, f, q, 0.0, sr);
            }
        }
    }

    /// Control-rate update: colour targets from the settings and the warmth, filters.
    fn control(&mut self) {
        let sr = self.sr;
        // while a structural switch is pending the colour holds; it jumps at the bottom of the dip
        if self.pending.is_none() {
            let col = voicing::colour(&self.s.v, self.warmth.value() * 0.01).to_array();
            for i in 0..N_COLOUR {
                self.cont[i].set_target(col[i]);
            }
        }
        for o in self.cont.iter_mut() {
            o.next();
        }
        let x = self.cont[c::XOVER].value();
        self.set_xover(x, sr);
        let emph_db = 15.0 * self.cont[c::EMPH].value();
        self.emph.set(SvfMode::HighShelf, 1000.0, std::f32::consts::FRAC_1_SQRT_2, emph_db, sr);
        let (hp, lp) = (self.cont[c::HPF].value(), self.cont[c::LPF].value().min(0.45 * sr));
        for o in self.out.iter_mut() {
            o.deemph.set(SvfMode::HighShelf, 1000.0, std::f32::consts::FRAC_1_SQRT_2, -emph_db, sr);
            o.hp.set(SvfMode::HighPass, hp, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            o.lp.set(SvfMode::LowPass, lp, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            // the Dimension's cross-mix is high-passed so the bass does not cancel
            o.cross_hp.set(SvfMode::HighPass, 300.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
        }
        let bw = self.cont[c::BW].value() as f64;
        for v in self.voices.iter_mut() {
            v.bbd.set_filters(0.45, bw);
        }
        let profile = voicing::Profile::ALL[voicing::idx(self.s.v[p::PROFILE], 5)];
        let (att, rel) = voicing::follower_times(profile);
        self.env.set_times(att, rel, sr);
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(2);
        let n = buffers.first().map_or(0, |b| b.len());
        let sr = self.sr as f64;
        let beats = Sync::from_index(voicing::idx(self.s.v[p::SYNC], 17)).beats();
        let slow_k = one_pole_coef(1.5 * sr) as f32;
        let comp_k = one_pole_coef(0.02 * sr) as f32;
        let detune_lock = self.s.v[p::DETUNE_LOCK] >= 0.5;
        let law = voicing::idx(self.s.v[p::MIX_LAW], 3);
        let comp_on = self.s.v[p::LEVEL_COMP] >= 0.5;
        let floor_ms = 1000.0 * (SincTable::HALF as f64 + 2.0) / sr;
        for i in 0..n {
            self.clock += 1;
            if self.ctrl == 0 {
                self.control();
            }
            self.ctrl = (self.ctrl + 1) % CTRL;

            // structural switch at the bottom of the dip
            let g = self.gate.next();
            if g == 0.0 && self.gate.is_settled() {
                if let Some(st) = self.pending.take() {
                    let col = voicing::colour(&self.s.v, self.warmth.value() * 0.01).to_array();
                    for k in 0..N_COLOUR {
                        self.cont[k].set_target(col[k]);
                        self.cont[k].settle();
                    }
                    self.apply_structure(st);
                    for o in self.out.iter_mut() {
                        o.deemph.reset();
                        o.hp.reset();
                        o.lp.reset();
                        o.cross_hp.reset();
                    }
                    self.emph.reset();
                    self.control();
                }
                self.gate.set_target(1.0);
            }

            // raised-cosine dip
            let gs = 0.5 - 0.5 * (std::f32::consts::PI * g).cos();
            let rate = self.rate.next() as f64;
            let depth0 = self.depth.next() as f64;
            let mix0 = self.mix.next();
            self.warmth.next();
            let fb = self.feedback.next();
            let out_db = self.output.next();
            let xk = self.xo_amt.next();
            let hz = match beats {
                Some(b) => self.transport.bpm() / 60.0 / b,
                None => rate,
            };
            match beats.and_then(|b| self.transport.phase_at(i, b, 0.0, self.sr)) {
                Some(ph) => self.phase = ph,
                None => self.phase = (self.phase + hz / sr).fract(),
            }
            self.fast_phase = (self.fast_phase + 6.0 / sr).fract();

            // input: dry per channel, crossover lows
            let x0 = buffers[0][i];
            let x1 = if nch > 1 { buffers[1][i] } else { x0 };
            // Linkwitz-Riley: LR4 = two Butterworth sections per band; LR2 = one Q 0.5 section, high band
            // inverted. low + high is an all-pass of the input.
            let mut low = [0.0f32; 2];
            let mut high = [0.0f32; 2];
            for (k, xv) in [x0, x1].into_iter().enumerate() {
                let a = self.xo[k][0].process(xv);
                let h = self.xo_hp[k][0].process(xv);
                if self.st.xover_24 {
                    low[k] = self.xo[k][1].process(a);
                    high[k] = self.xo_hp[k][1].process(h);
                } else {
                    low[k] = a;
                    high[k] = -h;
                }
            }
            let low_mono = 0.5 * (low[0] + low[1]);
            let high_mono = 0.5 * (high[0] + high[1]);
            let mono = 0.5 * (x0 + x1);

            // follower: playing level relative to its own long-term level
            let e = self.env.next(mono.abs());
            let edb = 20.0 * e.max(1e-6).log10();
            if edb > -70.0 {
                self.slow_db = edb + (self.slow_db - edb) * slow_k;
            }
            let rel_level = ((edb - self.slow_db + 6.0) / 12.0).clamp(0.0, 1.0);
            // hiss and clock bleed follow the playing (full at −40 dBFS and up): silence stays silent
            let hiss_gate = (e / 0.01).min(1.0);

            // wet input: highs, drive, compressor, pre-emphasis
            let hi = mono + xk * (high_mono - mono);
            let drive = self.cont[c::DRIVE].value();
            let kd = 1.0 + 4.0 * drive;
            let sat = self.adaa.process(Shaper::Tanh, hi * kd) / kd;
            let mut w_in = hi + (sat - hi) * (4.0 * drive).min(1.0);
            let comp = self.cont[c::COMP].value();
            self.comp_env = w_in.abs() + (self.comp_env - w_in.abs()) * comp_k;
            let ce = self.comp_env.max(1e-4);
            if comp > 0.0 {
                // NE570: 2:1 around −20 dBFS
                w_in *= (ce / 0.1).powf(-0.5 * comp).clamp(0.1, 10.0);
            }
            let w_in = self.emph.process(w_in);

            // modulation
            let depth = if detune_lock { depth0 * (0.6 / hz.max(0.05)).min(4.0) } else { depth0 };
            let depth = depth * (1.0 + self.cont[c::ENV_DEPTH].value() as f64 * rel_level as f64).clamp(0.0, 2.0);
            let centre = (self.cont[c::CENTRE].value() as f64).max(depth * 0.5 + floor_ms + self.cont[c::WOW].value() as f64);
            let spread = self.cont[c::SPREAD].value() as f64 / 360.0;
            let imperf = self.cont[c::IMPERF].value() as f64;
            let drift = self.cont[c::DRIFT].value() as f64;
            let wow = self.cont[c::WOW].value() as f64;
            let flutter = self.cont[c::FLUTTER].value() as f64;
            self.wow_ph = (self.wow_ph + 0.5 / sr).fract();
            self.flutter_ph = (self.flutter_ph + 7.3 / sr).fract();
            let tape_mod = wow * 0.5 * (std::f64::consts::TAU * self.wow_ph).sin() + flutter * 0.05 * (std::f64::consts::TAU * self.flutter_ph).sin();
            let nv = if self.st.mono { 1 } else { self.st.voices.max(1) };
            let mut wl = 0.0f32;
            let mut wr = 0.0f32;
            let (mut sum_l, mut sum_r) = (0.0f32, 0.0f32);
            let rc_k = one_pole_coef(sr / (std::f64::consts::TAU * 72.0));
            let engine = self.st.engine;
            let noise = self.cont[c::NOISE].value();
            let shape = self.st.shape;
            let fast = self.st.fast_lfo;
            let interp = self.st.interp;
            for k in 0..nv {
                let v = &mut self.voices[k];
                // drift: a slow random walk of phase and depth (0.1 Hz), per voice
                v.drift_ph += 0.1 / sr;
                if v.drift_ph >= 1.0 {
                    v.drift_ph -= 1.0;
                    v.drift_from = v.drift_to;
                    v.drift_to = v.rng.bipolar() as f64;
                }
                let dr = v.drift_from + (v.drift_to - v.drift_from) * 0.5 * (1.0 - (std::f64::consts::PI * v.drift_ph).cos());
                let ph = (self.phase + k as f64 * spread + imperf * 0.04 * v.phase_err + drift * 0.05 * dr).rem_euclid(1.0);
                if ph < v.last_phase && v.last_phase - ph > 0.5 {
                    v.rnd_from = v.rnd_to;
                    v.rnd_to = v.rng.bipolar() as f64;
                }
                v.last_phase = ph;
                let mut w = shape_value(ph, shape, v);
                if shape == TRIANGLE {
                    // the CE-2's RC smoothing of the triangle
                    v.rc = w + (v.rc - w) * rc_k;
                    w = v.rc;
                }
                if fast {
                    let f = (std::f64::consts::TAU * (self.fast_phase + k as f64 / 3.0)).sin();
                    w = (w + 0.2 * f) / 1.2;
                }
                let amp = (1.0 + imperf * 0.15 * v.amp_err as f64) * (1.0 + drift * 0.2 * dr);
                let d_ms = (centre + 0.5 * depth * amp * w + if engine == Engine::Tape { tape_mod } else { 0.0 }).clamp(floor_ms, MAX_DELAY_MS);
                v.delay_ms = d_ms;
                let input = w_in + fb * (v.fb).tanh();
                let y = match engine {
                    Engine::Bbd => {
                        v.bbd.set_noise(0.001 * noise * hiss_gate);
                        v.bbd.set_delay(d_ms * 0.001);
                        v.bbd.process(input)
                    }
                    _ => {
                        v.line.push(input);
                        let ds = d_ms * 0.001 * sr;
                        match (engine, interp) {
                            (Engine::Clean, 0) => v.line.read(ds, Interp::Lagrange3),
                            (Engine::Clean, 2) => self.sinc.read(&v.line, ds),
                            _ => v.line.read(ds, Interp::Lagrange5),
                        }
                    }
                };
                v.fb = y;
                // pan: one voice centre, two hard L / R, more spread across with a shared middle
                let pan = if nv == 1 { 0.5 } else { k as f32 / (nv - 1) as f32 };
                let (gl, gr) = if nv == 1 { (1.0, 1.0) } else { (1.0 - pan, pan) };
                wl += gl * y;
                wr += gr * y;
                sum_l += gl;
                sum_r += gr;
            }
            let mut wet = [wl / sum_l.max(1e-6), wr / sum_r.max(1e-6)];
            if engine == Engine::Tape {
                let hiss = 0.0005 * self.cont[c::NOISE].value() * hiss_gate;
                for wv in wet.iter_mut() {
                    *wv += hiss * self.tape_rng.bipolar();
                }
            }
            // de-emphasis, expander, filters
            for k in 0..2 {
                let o = &mut self.out[k];
                let mut y = o.deemph.process(wet[k]);
                if comp > 0.0 {
                    o.exp_env = y.abs() + (o.exp_env - y.abs()) * comp_k;
                    y *= (o.exp_env.max(1e-4) / 0.1).powf(comp).clamp(0.01, 10.0);
                }
                wet[k] = o.lp.process(o.hp.process(y));
            }
            let mid_pre = 0.5 * (wet[0] + wet[1]);
            // cross-mix: each side gets the other polarity-inverted and high-passed (Dimension)
            let cm = self.cont[c::CROSSMIX].value();
            if cm > 0.0 {
                let (l, r) = (wet[0], wet[1]);
                let hr = self.out[0].cross_hp.process(r);
                let hl = self.out[1].cross_hp.process(l);
                wet = [(l - cm * hr) / (1.0 + 0.5 * cm), (r - cm * hl) / (1.0 + 0.5 * cm)];
            }
            // width (M/S); mono-safe keeps the mid at the voices' mean, so the mono sum never hollows out
            let width = self.cont[c::WIDTH].value();
            let side = 0.5 * (wet[0] - wet[1]) * width;
            // mono-safe: the cross-mix only widens (it goes to the side), the mid stays the voices' mean
            let mid = if self.st.mono_safe { mid_pre } else { 0.5 * (wet[0] + wet[1]) };
            let wet = [mid + side, mid - side];

            // mix law, envelope on mix, level compensation
            let mix = (mix0 * (1.0 + self.cont[c::ENV_MIX].value() * rel_level).clamp(0.0, 2.0)).clamp(0.0, 2.0);
            let (a, b, lc) = match law {
                1 => (1.0 - mix.min(1.0), mix.min(1.0), 1.0 / ((1.0 - mix.min(1.0)).powi(2) + mix.min(1.0).powi(2)).sqrt()),
                2 => {
                    let t = mix.min(1.0) * std::f32::consts::FRAC_PI_2;
                    (t.cos(), t.sin(), 1.0)
                }
                _ => (1.0, mix, 1.0 / (1.0 + mix * mix).sqrt()),
            };
            self.gain_dry.set_target(a);
            self.gain_wet.set_target(b);
            self.gain_lc.set_target(if comp_on { lc } else { 1.0 });
            let (a, b, lc) = (self.gain_dry.next(), self.gain_wet.next(), self.gain_lc.next());
            let og = if out_db == 0.0 { 1.0 } else { 10f32.powf(out_db / 20.0) };
            for k in 0..nch {
                let x = buffers[k][i];
                // lows stay dry and mono under the crossover
                let dry = a * x + xk * (low_mono + a * high[k] - a * x);
                let w = if nch == 1 { 0.5 * (wet[0] + wet[1]) } else { wet[k] };
                buffers[k][i] = (dry + b * gs * w) * lc * og;
            }
        }
    }
}

impl Default for Chorus {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Chorus {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        for e in [&mut self.rate, &mut self.depth, &mut self.warmth, &mut self.mix] {
            e.prepare(sample_rate);
        }
        for o in self.cont.iter_mut() {
            o.prepare(sample_rate / CTRL as f32);
        }
        for o in [&mut self.feedback, &mut self.output, &mut self.gain_dry, &mut self.gain_wet, &mut self.gain_lc, &mut self.xo_amt] {
            o.prepare(sample_rate);
        }
        self.gate.prepare(sample_rate);
        self.env.prepare(sample_rate);
        let len = (MAX_DELAY_MS * 0.001 * sample_rate as f64 * 1.1) as usize + 32;
        for v in self.voices.iter_mut() {
            v.line.prepare(len);
            v.bbd.prepare(sample_rate);
        }
        self.apply_structure(self.st);
        self.control();
    }

    fn reset(&mut self) {
        if let Some(st) = self.pending.take() {
            self.st = st;
        }
        let v = self.s.v;
        self.rate.settle(v[p::RATE]);
        self.depth.settle(v[p::DEPTH]);
        self.warmth.settle(v[p::WARMTH]);
        self.mix.settle(v[p::MIX]);
        self.gate.set_immediate(1.0);
        self.apply_structure(self.st);
        let col = voicing::colour(&v, v[p::WARMTH] * 0.01).to_array();
        for i in 0..N_COLOUR {
            self.cont[i].set_target(col[i]);
            self.cont[i].settle();
        }
        for o in [&mut self.feedback, &mut self.output, &mut self.xo_amt] {
            o.settle();
        }
        let law = voicing::idx(v[p::MIX_LAW], 3);
        let m = v[p::MIX];
        let (a, b) = match law {
            1 => (1.0 - m, m),
            2 => ((m * std::f32::consts::FRAC_PI_2).cos(), (m * std::f32::consts::FRAC_PI_2).sin()),
            _ => (1.0, m),
        };
        self.gain_dry.set_immediate(a);
        self.gain_wet.set_immediate(b);
        let lc = match law {
            1 => 1.0 / ((1.0 - m).powi(2) + m * m).sqrt(),
            2 => 1.0,
            _ => 1.0 / (1.0 + m * m).sqrt(),
        };
        self.gain_lc.set_immediate(if v[p::LEVEL_COMP] >= 0.5 { lc } else { 1.0 });
        self.control();
        for ch in self.xo.iter_mut().chain(self.xo_hp.iter_mut()) {
            for s in ch.iter_mut() {
                s.reset();
            }
        }
        for o in self.out.iter_mut() {
            o.deemph.reset();
            o.hp.reset();
            o.lp.reset();
            o.cross_hp.reset();
            o.exp_env = 0.1;
        }
        self.adaa.reset(Shaper::Tanh);
        self.emph.reset();
        self.comp_env = 0.1;
        self.env.reset();
        self.slow_db = -40.0;
        self.phase = 0.0;
        self.fast_phase = 0.0;
        self.tape_rng.reset();
        self.wow_ph = 0.0;
        self.flutter_ph = 0.0;
        self.ctrl = 0;
        self.clock = 0;
    }

    fn set_param(&mut self, id: &str, value: f32) {
        if let Some(i) = PARAMS.iter().position(|pi| pi.id == id) {
            let s = self.s.with(i, value);
            self.set(s);
        }
    }

    fn set_transport(&mut self, transport: TransportInfo) {
        self.transport = transport;
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_block(buffers);
    }
}

#[cfg(test)]
mod tests;
