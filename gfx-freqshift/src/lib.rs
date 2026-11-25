// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-freqshift — golem's frequency shifter (design: README.md).
//!
//! Stands for golem's `surge-freqshift` block (ids `shift`, `shift_r`, `time`, `fbk`, `mix` kept, in real
//! units: README §3.4 converts the catalogue's values once).
//!
//! - **Core** (gfx-dsp `ssb`): a Hilbert pair designed for the actual rate (15 Hz – fs / 2 − 15 Hz,
//!   8 + 8 sections up to 50 kHz, 10 + 10 above, image −87 dB at 48 kHz), the complex product with a
//!   continuous-phase oscillator (signed: through zero without a step), the sideband balance, and the
//!   same pair a second time on the complex signal (`zero_cut`): partials shifted below 0 Hz or past
//!   Nyquist are removed. With `zero_cut` off, a 4th-order low-pass that follows the upward shift keeps
//!   wraps from being trapped in the loop.
//! - **Loop** (per channel): buf = x + sat(g · cuts(wet[n − 1])), wet = SSB(P(read(buf, time))), with
//!   P = a 20 Hz high-pass (plus a 12-stage all-pass chain when `loop` = Phaser). `time` 0 reads the
//!   sample just written: the classic undelayed Bode shifter.
//! - **Stereo**: the right shift is `shift_r` × the left's; at +100 % with `stereo_phase` 0 the right
//!   oscillator phase-locks onto the left, then the right channel is set equal (bit-exact L = R on equal
//!   inputs, computed once).
//! - **Modulation**: an LFO (gfx-glitch's shapes) and the input envelope, both in Hz on the shift.
//! - **Dry**: untouched, or through the wet's own static path (`dry_match`). Zero latency.
//!
//! Real-time safe, deterministic (LFO phase 0 and fixed seeds on reset).

#![allow(clippy::needless_range_loop)]

#[cfg(feature = "plugin")]
pub mod plugin;

#[cfg(test)]
mod tests;

use gfx_dsp::delay::{DelayLine, Interp};
use gfx_dsp::envelope::PeakFollower;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::lfo::Rng;
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::ssb::{phase_dist, Allpass, HilbertDesign, Phasor, SsbShifter};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::transport::{Sync, TransportInfo};
use gfx_dsp::{Effect, ParamInfo};

pub const NP: usize = 23;

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::float("shift", "Shift L", -2000.0, 2000.0, 2.0).with_exp(),
    ParamInfo::float("fbk", "Feedback", 0.0, 110.0, 0.0).with_exp(),
    ParamInfo::float("time", "Time", 0.0, 2000.0, 0.0).with_exp(),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 50.0).with_exp(),
    ParamInfo::float("shift_r", "Shift R", -100.0, 100.0, 100.0).with_exp(),
    ParamInfo::enumeration("loop", "Loop", 2, 0),
    ParamInfo::float("sideband", "Sideband", -100.0, 100.0, 100.0).with_exp(),
    ParamInfo::boolean("zero_cut", "Zero Cut", true),
    ParamInfo::boolean("dry_match", "Dry Match", false),
    ParamInfo::enumeration("time_sync", "Time Sync", 17, 0),
    ParamInfo::float("low_cut", "Loop Low Cut", 20.0, 1000.0, 20.0),
    ParamInfo::float("high_cut", "Loop High Cut", 1000.0, 20000.0, 20000.0).with_exp(),
    ParamInfo::float("lfo_depth", "LFO Depth", 0.0, 500.0, 0.0).with_exp(),
    ParamInfo::float("rate", "LFO Rate", 0.01, 20.0, 0.5).with_exp(),
    ParamInfo::enumeration("sync", "LFO Sync", 17, 0),
    ParamInfo::enumeration("lfo_shape", "LFO Shape", 7, 0),
    ParamInfo::float("stereo_phase", "Stereo Phase", 0.0, 180.0, 0.0),
    ParamInfo::float("env_depth", "Envelope Depth", -1000.0, 1000.0, 0.0).with_exp(),
    ParamInfo::float("env_sens", "Env Sensitivity", 0.0, 100.0, 50.0),
    ParamInfo::float("env_att", "Env Attack", 1.0, 100.0, 5.0),
    ParamInfo::float("env_rel", "Env Release", 20.0, 2000.0, 250.0),
    ParamInfo::boolean("bypass", "Bypass", false),
    ParamInfo::boolean("trails", "Trails", true),
];

pub mod p {
    pub const SHIFT: usize = 0;
    pub const FBK: usize = 1;
    pub const TIME: usize = 2;
    pub const MIX: usize = 3;
    pub const SHIFT_R: usize = 4;
    pub const LOOP: usize = 5;
    pub const SIDEBAND: usize = 6;
    pub const ZERO_CUT: usize = 7;
    pub const DRY_MATCH: usize = 8;
    pub const TIME_SYNC: usize = 9;
    pub const LOW_CUT: usize = 10;
    pub const HIGH_CUT: usize = 11;
    pub const LFO_DEPTH: usize = 12;
    pub const RATE: usize = 13;
    pub const SYNC: usize = 14;
    pub const LFO_SHAPE: usize = 15;
    pub const STEREO_PHASE: usize = 16;
    pub const ENV_DEPTH: usize = 17;
    pub const ENV_SENS: usize = 18;
    pub const ENV_ATT: usize = 19;
    pub const ENV_REL: usize = 20;
    pub const BYPASS: usize = 21;
    pub const TRAILS: usize = 22;
}

/// `loop` options (enum order, final; append only).
pub const LOOPS: [&str; 2] = ["Delay", "Phaser"];
/// LFO shapes (gfx-glitch's list and order, final).
pub const SHAPES: [&str; 7] = ["Sine", "Triangle", "Saw Up", "Saw Down", "Square", "Random Steps", "Random Smooth"];

/// The line holds 2 s at any rate.
const MAX_TIME_S: f64 = 2.0;
/// `time` glides as a 100 ms one-pole, at most one sample per sample (Doppler between a freeze and an
/// octave up).
const GLIDE_S: f64 = 0.1;
const MAX_GLIDE_SPEED: f64 = 1.0;
/// The glide's speed itself follows a 10 ms one-pole: the Doppler starts and ends without a corner.
const GLIDE_ACC_S: f64 = 0.01;
/// The phaser chain: 12 first-order all-passes, corners log-spaced 100 Hz – 3.2 kHz (to be voiced).
const PHASER_STAGES: usize = 12;
const PHASER_LO: f64 = 100.0;
const PHASER_HI: f64 = 3200.0;
/// The wet's input high-pass (2nd-order Butterworth).
const HP_HZ: f32 = 20.0;
/// A parked oscillator returns to phase 0 at this rate at most (Hz).
const PARK_HZ: f64 = 1.0;
/// The right oscillator reaches the left's over this time, then the right channel crossfades onto
/// the left's output over `MERGE_MS` and takes its state.
const LOCK_S: f64 = 0.05;
const MERGE_MS: f32 = 20.0;
/// Control-rate period (samples) of the loop filters' coefficients.
const CTRL: u64 = 16;

#[inline]
fn idx(v: f32, n: usize) -> usize {
    (v.round().max(0.0) as usize).min(n - 1)
}

/// The return's soft clip: linear below −6 dBFS, ceiling 0 dBFS, slope 1 at the knee.
#[inline]
fn sat(v: f32) -> f32 {
    let a = v.abs();
    if a <= 0.5 {
        v
    } else {
        (0.5 + 0.5 * ((a - 0.5) * 2.0).tanh()).copysign(v)
    }
}

/// The LFO: phase in f64, seeded random shapes (gfx-glitch's), output smoothed by the caller.
#[derive(Clone, Debug)]
struct ModLfo {
    phase: f64,
    rng: Rng,
    from: f32,
    to: f32,
    out: f32,
}

impl ModLfo {
    fn new() -> Self {
        Self { phase: 0.0, rng: Rng::new(0xF5_0F_15_ED), from: 0.0, to: 0.0, out: 0.0 }
    }
    fn reset(&mut self) {
        *self = Self::new();
    }
    /// Moves to `phase`; a wrap draws the next random value.
    fn set_phase(&mut self, phase: f64) {
        if phase < self.phase {
            self.from = self.to;
            self.to = self.rng.bipolar();
        }
        self.phase = phase;
    }
    fn raw(&self, shape: usize) -> f32 {
        let p = self.phase as f32;
        match shape {
            0 => (2.0 * std::f32::consts::PI * p).sin(),
            1 => 1.0 - 4.0 * (p - 0.5).abs(),
            2 => 2.0 * p - 1.0,
            3 => 1.0 - 2.0 * p,
            4 => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            5 => self.to,
            _ => self.from + (self.to - self.from) * (0.5 - 0.5 * (std::f32::consts::PI * p).cos()),
        }
    }
}

/// First-order all-pass, f64: H = (a + z⁻¹) / (1 + a z⁻¹).
#[derive(Clone, Copy, Debug, Default)]
struct Ap1 {
    a: f64,
    x1: f64,
    y1: f64,
}

impl Ap1 {
    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let y = self.a * x + (self.x1 - self.a * self.y1);
        self.x1 = x;
        self.y1 = y;
        y
    }
}

/// Per-sample controls shared by the channels.
#[derive(Clone, Copy, Debug)]
struct Ctl {
    feed: f32,
    fb: f32,
    d_target: f64,
    glide_a: f64,
    glide_v: f64,
    loopx: f32,
    cutx: f32,
    dmx: f32,
    balance: f64,
}

/// One channel: its loop, its shifter, its oscillator and its matched dry.
#[derive(Clone, Debug)]
struct Chan {
    line: DelayLine,
    /// the current delay (samples)
    d: f64,
    /// its speed (samples per sample)
    dv: f64,
    hp: Svf,
    phaser: [Ap1; PHASER_STAGES],
    phaser_live: bool,
    ssb: SsbShifter,
    cut_live: bool,
    aw: [Svf; 2],
    aw_fc: f32,
    aw_live: bool,
    lc: Svf,
    hc: Svf,
    /// the last wet sample (fed back)
    wet: f32,
    dhp: Svf,
    da: [Allpass; 2],
    dry_live: bool,
    osc: Phasor,
}

impl Chan {
    fn new() -> Self {
        Self {
            line: DelayLine::new(),
            d: 0.0,
            dv: 0.0,
            hp: Svf::new(),
            phaser: [Ap1::default(); PHASER_STAGES],
            phaser_live: false,
            ssb: SsbShifter::default(),
            cut_live: false,
            aw: [Svf::new(), Svf::new()],
            aw_fc: 0.0,
            aw_live: false,
            lc: Svf::new(),
            hc: Svf::new(),
            wet: 0.0,
            dhp: Svf::new(),
            da: [Allpass::default(); 2],
            dry_live: false,
            osc: Phasor::default(),
        }
    }

    fn prepare(&mut self, sr: f32, design: &HilbertDesign) {
        self.line.prepare((MAX_TIME_S * sr as f64) as usize + 8);
        self.ssb = SsbShifter::new(design);
        self.da = [Allpass::path_a(design); 2];
        self.hp.set(SvfMode::HighPass, HP_HZ, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
        self.dhp.set(SvfMode::HighPass, HP_HZ, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
        let srd = sr as f64;
        for (k, s) in self.phaser.iter_mut().enumerate() {
            let f = PHASER_LO * (PHASER_HI / PHASER_LO).powf(k as f64 / (PHASER_STAGES - 1) as f64);
            let t = (std::f64::consts::PI * f / srd).tan();
            s.a = (t - 1.0) / (t + 1.0);
        }
        self.aw_fc = 0.0;
    }

    fn reset(&mut self, d: f64) {
        self.line.reset();
        self.d = d;
        self.dv = 0.0;
        let [a0, a1] = &mut self.aw;
        for f in [&mut self.hp, a0, a1, &mut self.lc, &mut self.hc, &mut self.dhp] {
            f.reset();
        }
        for s in self.phaser.iter_mut() {
            s.x1 = 0.0;
            s.y1 = 0.0;
        }
        self.phaser_live = false;
        self.ssb.reset();
        self.cut_live = false;
        self.aw_live = false;
        self.wet = 0.0;
        for a in self.da.iter_mut() {
            a.reset();
        }
        self.dry_live = false;
        self.osc.reset();
    }

    /// Takes another channel's whole state, without allocating.
    fn copy_from(&mut self, o: &Chan) {
        self.line.copy_from(&o.line);
        self.d = o.d;
        self.dv = o.dv;
        self.hp.clone_from(&o.hp);
        self.phaser = o.phaser;
        self.phaser_live = o.phaser_live;
        self.ssb = o.ssb;
        self.cut_live = o.cut_live;
        self.aw.clone_from(&o.aw);
        self.aw_fc = o.aw_fc;
        self.aw_live = o.aw_live;
        self.lc.clone_from(&o.lc);
        self.hc.clone_from(&o.hc);
        self.wet = o.wet;
        self.dhp.clone_from(&o.dhp);
        self.da = o.da;
        self.dry_live = o.dry_live;
        self.osc = o.osc;
    }

    /// The anti-wrap low-pass: min(20 kHz, fs / 2 − the upward shift − 1 kHz).
    #[inline]
    fn set_antiwrap(&mut self, up_hz: f64, sr: f32) {
        let fc = (0.5 * sr as f64 - up_hz.max(0.0) - 1000.0).clamp(1000.0, 20000.0) as f32;
        if (fc - self.aw_fc).abs() > 1.0 {
            self.aw_fc = fc;
            self.aw[0].set(SvfMode::LowPass, fc, 0.541_196, 0.0, sr);
            self.aw[1].set(SvfMode::LowPass, fc, 1.306_563, 0.0, sr);
        }
    }

    /// One sample: returns (wet, matched dry).
    #[inline]
    fn tick(&mut self, x: f32, c: &Ctl, cos: f64, sin: f64, up_hz: f64, sr: f32) -> (f32, f32) {
        // the return: loop cuts, gain, soft clip
        let ret = sat(c.fb * self.hc.process(self.lc.process(self.wet)));
        self.line.push(x * c.feed + ret);
        if self.d != c.d_target {
            let v = ((c.d_target - self.d) * c.glide_a).clamp(-MAX_GLIDE_SPEED, MAX_GLIDE_SPEED);
            self.dv += (v - self.dv) * c.glide_v;
            self.d = (self.d + self.dv).max(0.0);
            if (c.d_target - self.d).abs() < 1e-6 && self.dv.abs() < 1e-6 {
                self.d = c.d_target;
                self.dv = 0.0;
            }
        }
        // below one sample (a glide to or from 0) the read is linear, at 0 the sample itself
        let r = if self.d < 1.0 { self.line.read(self.d, Interp::Linear) } else { self.line.read(self.d, Interp::Hermite) };
        let mut pin = self.hp.process(r) as f64;
        if c.loopx > 0.0 {
            let mut v = pin;
            for s in self.phaser.iter_mut() {
                v = s.process(v);
            }
            pin += (v - pin) * c.loopx as f64;
            self.phaser_live = true;
        } else if self.phaser_live {
            for s in self.phaser.iter_mut() {
                s.x1 = 0.0;
                s.y1 = 0.0;
            }
            self.phaser_live = false;
        }
        let w = self.ssb.shift(pin, cos, sin, c.balance);
        let mut wet = 0.0f64;
        if c.cutx > 0.0 {
            wet += c.cutx as f64 * self.ssb.zero_cut(w);
            self.cut_live = true;
        } else if self.cut_live {
            self.ssb.reset_cut();
            self.cut_live = false;
        }
        if c.cutx < 1.0 {
            self.set_antiwrap(up_hz, sr);
            let lp0 = self.aw[0].process(w.0 as f32);
            let lp = self.aw[1].process(lp0);
            wet += (1.0 - c.cutx) as f64 * lp as f64;
            self.aw_live = true;
        } else if self.aw_live {
            self.aw[0].reset();
            self.aw[1].reset();
            self.aw_live = false;
        }
        let wet = wet as f32;
        self.wet = wet;
        // the matched dry: the wet's static path (high-pass, path A once, twice with the cut)
        let mut dry = x;
        if c.dmx > 0.0 {
            let h = self.dhp.process(x) as f64;
            let a1 = self.da[0].process(h);
            let m = if c.cutx > 0.0 { a1 + (self.da[1].process(a1) - a1) * c.cutx as f64 } else { a1 };
            if c.cutx <= 0.0 {
                self.da[1].reset();
            }
            dry = x + (m as f32 - x) * c.dmx;
            self.dry_live = true;
        } else if self.dry_live {
            self.dhp.reset();
            self.da[0].reset();
            self.da[1].reset();
            self.dry_live = false;
        }
        (wet, dry)
    }
}

pub struct FreqShift {
    v: [f32; NP],
    sr: f32,
    nch: usize,
    design: HilbertDesign,
    ch: [Chan; 2],
    transport: TransportInfo,
    shift: ExpSmoother,
    fbk: ExpSmoother,
    mix: ExpSmoother,
    shift_r: ExpSmoother,
    sideband: OnePole,
    high_cut: ExpSmoother,
    low_cut: OnePole,
    lfo_depth: ExpSmoother,
    rate: ExpSmoother,
    env_depth: ExpSmoother,
    stereo_phase: OnePole,
    loopx: LinearRamp,
    cutx: LinearRamp,
    dmx: LinearRamp,
    byp: LinearRamp,
    merge: LinearRamp,
    lfo: [ModLfo; 2],
    lfo_phase: f64,
    lfo_k: f32,
    env: PeakFollower,
    env_out: f32,
    /// right oscillator: samples left to reach the left's phase; locked; right state = left state
    lock_left: u32,
    locked: bool,
    same: bool,
    cleared: bool,
    clock: u64,
}

impl Default for FreqShift {
    fn default() -> Self {
        Self::new()
    }
}

impl FreqShift {
    pub fn new() -> Self {
        let v: [f32; NP] = core::array::from_fn(|i| PARAMS[i].default);
        let es = |i: usize| ExpSmoother::new(PARAMS[i].min, PARAMS[i].max, v[i]);
        Self {
            v,
            sr: 48000.0,
            nch: 1,
            design: HilbertDesign::for_rate(48000.0),
            ch: [Chan::new(), Chan::new()],
            transport: TransportInfo::default(),
            shift: es(p::SHIFT),
            fbk: es(p::FBK),
            mix: es(p::MIX),
            shift_r: es(p::SHIFT_R),
            sideband: OnePole::new(20.0, v[p::SIDEBAND], 1e-4),
            high_cut: es(p::HIGH_CUT),
            low_cut: OnePole::new(30.0, v[p::LOW_CUT], 1e-3),
            lfo_depth: es(p::LFO_DEPTH),
            rate: es(p::RATE),
            env_depth: es(p::ENV_DEPTH),
            stereo_phase: OnePole::new(30.0, v[p::STEREO_PHASE], 1e-4),
            loopx: LinearRamp::new(20.0, 0.0),
            cutx: LinearRamp::new(20.0, 1.0),
            dmx: LinearRamp::new(20.0, 0.0),
            byp: LinearRamp::new(20.0, 0.0),
            merge: LinearRamp::new(MERGE_MS, 0.0),
            lfo: [ModLfo::new(), ModLfo::new()],
            lfo_phase: 0.0,
            lfo_k: 0.0,
            env: PeakFollower::new(5.0, 250.0),
            env_out: 0.0,
            lock_left: 0,
            locked: true,
            same: true,
            cleared: false,
            clock: 0,
        }
    }

    fn bool_v(&self, i: usize) -> f32 {
        if self.v[i] >= 0.5 {
            1.0
        } else {
            0.0
        }
    }

    /// The delay the settings ask for (samples): `time`, or `time_sync` at the host tempo; ≤ 2 s.
    pub fn target_samples(&self) -> f64 {
        let sr = self.sr as f64;
        let s = match Sync::from_index(idx(self.v[p::TIME_SYNC], 17)).beats() {
            Some(b) => self.transport.note_samples(b, self.sr),
            None => self.v[p::TIME] as f64 * 0.001 * sr,
        };
        s.clamp(0.0, MAX_TIME_S * sr)
    }

    fn lock_wanted(&self) -> bool {
        self.v[p::SHIFT_R] == 100.0 && self.v[p::STEREO_PHASE] == 0.0
    }

    fn apply(&mut self, i: usize) {
        let v = self.v[i];
        let now = self.clock;
        match i {
            p::SHIFT => self.shift.set_target(v, now),
            p::FBK => self.fbk.set_target(v, now),
            p::MIX => self.mix.set_target(v, now),
            p::SHIFT_R => self.shift_r.set_target(v, now),
            p::SIDEBAND => self.sideband.set_target(v),
            p::HIGH_CUT => self.high_cut.set_target(v, now),
            p::LOW_CUT => self.low_cut.set_target(v),
            p::LFO_DEPTH => self.lfo_depth.set_target(v, now),
            p::RATE => self.rate.set_target(v, now),
            p::ENV_DEPTH => self.env_depth.set_target(v, now),
            p::STEREO_PHASE => self.stereo_phase.set_target(v),
            p::LOOP => self.loopx.set_target(idx(v, 2) as f32),
            p::ZERO_CUT => self.cutx.set_target(self.bool_v(i)),
            p::DRY_MATCH => self.dmx.set_target(self.bool_v(i)),
            p::BYPASS => self.byp.set_target(self.bool_v(i)),
            p::ENV_ATT | p::ENV_REL => self.env.set_times(self.v[p::ENV_ATT], self.v[p::ENV_REL], self.sr),
            _ => {}
        }
    }

    fn set_loop_filters(&mut self) {
        let sr = self.sr;
        let lc = self.low_cut.value();
        let hc = self.high_cut.value().min(0.45 * sr);
        for c in self.ch.iter_mut() {
            c.lc.set(SvfMode::HighPass, lc, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            c.hc.set(SvfMode::LowPass, hc, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
        }
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(self.nch).min(2);
        if nch == 0 {
            return;
        }
        let n = buffers[0].len();
        let sr = self.sr;
        let srd = sr as f64;
        let v = self.v;
        let shape = idx(v[p::LFO_SHAPE], 7);
        let beats = Sync::from_index(idx(v[p::SYNC], 17)).beats();
        let sens = v[p::ENV_SENS] * 0.01;
        let trails = v[p::TRAILS] >= 0.5;
        let lock_wanted = self.lock_wanted();
        let park_l = v[p::SHIFT] == 0.0 && v[p::LFO_DEPTH] == 0.0 && v[p::ENV_DEPTH] == 0.0;
        let park_r = park_l || v[p::SHIFT_R] == 0.0;
        let glide_a = 1.0 - (-1.0 / (GLIDE_S * srd)).exp();
        let glide_v = 1.0 - (-1.0 / (GLIDE_ACC_S * srd)).exp();
        let lock_len = (LOCK_S * srd) as u32;
        if !lock_wanted {
            self.locked = false;
            self.same = false;
            self.lock_left = lock_len;
            self.merge.set_immediate(0.0);
        }
        let d_target = self.target_samples();

        for i in 0..n {
            self.clock += 1;
            // ---- modulation
            let rate = self.rate.next() as f64;
            let prev = self.lfo_phase;
            self.lfo_phase = match beats.and_then(|b| self.transport.phase_at(i, b, 0.0, sr)) {
                Some(ph) => ph,
                None => {
                    let hz = match beats {
                        Some(b) => self.transport.bpm() / 60.0 / b,
                        None => rate,
                    };
                    (prev + hz / srd).rem_euclid(1.0)
                }
            };
            let sp = self.stereo_phase.next() as f64 / 360.0;
            for (k, l) in self.lfo.iter_mut().enumerate() {
                let off = if k == 0 { 0.0 } else { sp };
                l.set_phase((self.lfo_phase + off).rem_euclid(1.0));
                let t = l.raw(shape);
                l.out += (t - l.out) * self.lfo_k;
            }
            let mut lvl = 0.0f32;
            for c in 0..nch {
                lvl = lvl.max(buffers[c][i].abs());
            }
            let e = self.env.next(lvl);
            let floor = -20.0 - 40.0 * sens;
            self.env_out = ((20.0 * e.max(1e-6).log10() - floor) / 30.0).clamp(0.0, 1.0);

            // ---- smoothed controls
            let shift = self.shift.next() as f64;
            let ld = self.lfo_depth.next() as f64;
            let ed = self.env_depth.next() as f64 * self.env_out as f64;
            let ratio = self.shift_r.next() as f64 * 0.01;
            let balance = self.sideband.next() as f64 * 0.01;
            let fb = self.fbk.next() * 0.01;
            let mix = self.mix.next() * 0.01;
            self.high_cut.next();
            self.low_cut.next();
            if self.clock.is_multiple_of(CTRL) && (!self.high_cut.is_settled() || !self.low_cut.is_settled()) {
                self.set_loop_filters();
            }
            let byp = self.byp.next();
            let ctl = Ctl {
                feed: 1.0 - byp,
                fb,
                d_target,
                glide_a,
                glide_v,
                loopx: self.loopx.next(),
                cutx: self.cutx.next(),
                dmx: self.dmx.next() * (1.0 - byp),
                balance,
            };
            let wet_g = if trails { mix } else { mix * (1.0 - byp) };
            let dry_g = (1.0 - mix) + mix * byp;
            if !trails && byp >= 1.0 && self.byp.is_settled() {
                if !self.cleared {
                    for c in self.ch.iter_mut() {
                        c.reset(d_target);
                    }
                    self.cleared = true;
                }
            } else {
                self.cleared = false;
            }

            // ---- oscillators
            let s_l = shift + ld * self.lfo[0].out as f64 + ed;
            let s_r = ratio * (shift + ld * self.lfo[1].out as f64 + ed);
            if park_l {
                self.ch[0].osc.slew_to(0.0, PARK_HZ, srd);
            } else {
                self.ch[0].osc.advance(s_l, srd);
            }
            let up = |s: f64| {
                let mut u = 0.0f64;
                if balance > -1.0 {
                    u = u.max(s);
                }
                if balance < 1.0 {
                    u = u.max(-s);
                }
                u
            };

            if nch == 1 {
                let (c, s) = self.ch[0].osc.cos_sin();
                let x = buffers[0][i];
                let (w, d) = self.ch[0].tick(x, &ctl, c, s, up(s_l), sr);
                buffers[0][i] = d * dry_g + w * wet_g;
                continue;
            }

            // right oscillator: its own shift, or locked onto the left
            if lock_wanted && !self.locked {
                if park_r {
                    self.ch[1].osc.slew_to(0.0, PARK_HZ, srd);
                } else {
                    self.ch[1].osc.advance(s_r, srd);
                }
                let (pl, pr) = (self.ch[0].osc.phase, self.ch[1].osc.phase);
                if self.lock_left > 0 {
                    self.ch[1].osc.phase = (pr + phase_dist(pr, pl) / self.lock_left as f64).rem_euclid(1.0);
                    self.lock_left -= 1;
                } else {
                    self.ch[1].osc.phase = pl;
                    if self.shift_r.is_settled() && self.stereo_phase.is_settled() {
                        self.locked = true;
                    }
                }
            } else if self.locked {
                self.ch[1].osc.phase = self.ch[0].osc.phase;
            } else if park_r {
                self.ch[1].osc.slew_to(0.0, PARK_HZ, srd);
            } else {
                self.ch[1].osc.advance(s_r, srd);
            }

            let (xl, xr) = (buffers[0][i], buffers[1][i]);
            let (cl, sl) = self.ch[0].osc.cos_sin();
            let equal = xl.to_bits() == xr.to_bits();
            if self.same && equal {
                // right = left, bit for bit: computed once
                let (w, d) = self.ch[0].tick(xl, &ctl, cl, sl, up(s_l), sr);
                let o = d * dry_g + w * wet_g;
                buffers[0][i] = o;
                buffers[1][i] = o;
                continue;
            }
            if self.same {
                // the inputs part: the right resumes from the left's state (it was not computed)
                let (a, b) = self.ch.split_at_mut(1);
                b[0].copy_from(&a[0]);
            }
            let (cr, srr) = self.ch[1].osc.cos_sin();
            let (wl, dl) = self.ch[0].tick(xl, &ctl, cl, sl, up(s_l), sr);
            let (mut wr, mut dr) = self.ch[1].tick(xr, &ctl, cr, srr, up(s_r), sr);
            self.same = false;
            if self.locked && equal {
                // equal inputs, locked: the right crossfades onto the left, then takes its state
                self.merge.set_target(1.0);
                let m = self.merge.next();
                wr += (wl - wr) * m;
                dr += (dl - dr) * m;
                if m >= 1.0 && self.merge.is_settled() {
                    let (a, b) = self.ch.split_at_mut(1);
                    b[0].copy_from(&a[0]);
                    self.same = true;
                    self.merge.set_immediate(0.0);
                }
            } else {
                self.merge.set_immediate(0.0);
            }
            buffers[0][i] = dl * dry_g + wl * wet_g;
            buffers[1][i] = dr * dry_g + wr * wet_g;
        }
    }
}

impl Effect for FreqShift {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate;
        self.nch = channels.clamp(1, 2);
        self.design = HilbertDesign::for_rate(sample_rate as f64);
        for c in self.ch.iter_mut() {
            c.prepare(sample_rate, &self.design);
        }
        for e in [&mut self.shift, &mut self.fbk, &mut self.mix, &mut self.shift_r, &mut self.high_cut, &mut self.lfo_depth, &mut self.rate, &mut self.env_depth] {
            e.prepare(sample_rate);
        }
        for o in [&mut self.sideband, &mut self.low_cut, &mut self.stereo_phase] {
            o.prepare(sample_rate);
        }
        for r in [&mut self.loopx, &mut self.cutx, &mut self.dmx, &mut self.byp, &mut self.merge] {
            r.prepare(sample_rate);
        }
        self.lfo_k = 1.0 - (-1.0 / (0.003 * sample_rate)).exp();
        self.env.prepare(sample_rate);
        self.env.set_times(self.v[p::ENV_ATT], self.v[p::ENV_REL], sample_rate);
        self.reset();
    }

    fn reset(&mut self) {
        let v = self.v;
        for (i, e) in [(p::SHIFT, &mut self.shift), (p::FBK, &mut self.fbk), (p::MIX, &mut self.mix), (p::SHIFT_R, &mut self.shift_r), (p::HIGH_CUT, &mut self.high_cut), (p::LFO_DEPTH, &mut self.lfo_depth), (p::RATE, &mut self.rate), (p::ENV_DEPTH, &mut self.env_depth)] {
            e.settle(v[i]);
            e.reset();
        }
        for (i, o) in [(p::SIDEBAND, &mut self.sideband), (p::LOW_CUT, &mut self.low_cut), (p::STEREO_PHASE, &mut self.stereo_phase)] {
            o.set_target(v[i]);
            o.settle();
        }
        self.loopx.set_immediate(idx(v[p::LOOP], 2) as f32);
        self.cutx.set_immediate(self.bool_v(p::ZERO_CUT));
        self.dmx.set_immediate(self.bool_v(p::DRY_MATCH));
        self.byp.set_immediate(self.bool_v(p::BYPASS));
        self.merge.set_immediate(0.0);
        let d = self.target_samples();
        for c in self.ch.iter_mut() {
            c.reset(d);
        }
        self.set_loop_filters();
        for l in self.lfo.iter_mut() {
            l.reset();
        }
        self.lfo_phase = 0.0;
        self.env.set_times(v[p::ENV_ATT], v[p::ENV_REL], self.sr);
        self.env.reset();
        self.env_out = 0.0;
        let lw = self.lock_wanted();
        self.locked = lw;
        self.same = lw;
        self.lock_left = (LOCK_S * self.sr as f64) as u32;
        self.cleared = false;
        self.clock = 0;
    }

    fn set_param(&mut self, id: &str, value: f32) {
        if let Some(i) = PARAMS.iter().position(|pi| pi.id == id) {
            let pi = &PARAMS[i];
            let v = value.clamp(pi.min, pi.max);
            if v != self.v[i] {
                self.v[i] = v;
                self.apply(i);
            }
        }
    }

    fn set_transport(&mut self, transport: TransportInfo) {
        self.transport = transport;
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_block(buffers);
    }
}
