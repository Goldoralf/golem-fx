// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-phaser — golem's phaser (design: README.md).
//!
//! Stands for golem's `phaser-js` block. It keeps the ids `rate`, `low`, `high`, `feedback` and `wet`;
//! `low` and `high` are now exact frequencies.
//!
//! Engine:
//! - A cascade of TPT first-order all-passes, prewarped so that frequencies are exact.
//! - The sweep is exponential: f = low · (high / low)^u.
//! - The feedback loop is linear, so it is solved without its one-sample delay (zero-delay
//!   feedback): y = (A·x + B) / (1 − k·A).
//!
//! Models:
//! - Classic, Script 90, Block 90, Stone and Deep 12 are configurations of the cascade.
//! - Vibe: gfx-dsp's Uni-Vibe core.
//! - Bi-Phase: two 6-stage phasers, in series or in parallel.
//! - Barber ↑ / ↓: two cascades on sawtooth sweeps, crossfaded.
//!
//! The LFO runs freely: phase 0 on reset, and no parameter change ever moves it. Zero latency.

#![allow(clippy::needless_range_loop)]

use gfx_dsp::envelope::PeakFollower;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::lfo::Rng;
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::transport::{Sync, TransportInfo};
use gfx_dsp::vibe::{Lamp, Vibe};
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
mod plugin;

const MAX_STAGES: usize = 16;
pub const STAGES: [usize; 7] = [2, 4, 6, 8, 10, 12, 16];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    Classic,
    Script90,
    Block90,
    Stone,
    Vibe,
    BiPhase,
    Deep12,
    BarberUp,
    BarberDown,
}

impl Model {
    pub const ALL: [Model; 9] = [Model::Classic, Model::Script90, Model::Block90, Model::Stone, Model::Vibe, Model::BiPhase, Model::Deep12, Model::BarberUp, Model::BarberDown];
}

pub static PARAMS: [ParamInfo; 28] = [
    ParamInfo::float("rate", "Rate", 0.05, 8.0, 0.5).with_exp(),
    ParamInfo::float("low", "Low", 20.0, 4000.0, 440.0),
    ParamInfo::float("high", "High", 100.0, 16000.0, 1600.0),
    ParamInfo::float("feedback", "Feedback", -60.0, -1.0, -3.0).with_exp(),
    ParamInfo::float("wet", "Wet", -60.0, 6.0, 0.0),
    ParamInfo::enumeration("model", "Model", 9, 0),
    ParamInfo::enumeration("stages", "Stages", 7, 1),
    ParamInfo::float("spread", "Spread", 0.0, 100.0, 0.0),
    ParamInfo::enumeration("fb_polarity", "Feedback Polarity", 2, 0),
    ParamInfo::float("dry", "Dry", -60.0, 0.0, 0.0),
    ParamInfo::float("manual", "Manual", 0.0, 100.0, 50.0).with_exp(),
    ParamInfo::float("depth", "Depth", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::enumeration("shape", "LFO Shape", 8, 0),
    ParamInfo::float("skew", "Skew", 0.0, 100.0, 50.0),
    ParamInfo::enumeration("sweep_src", "Sweep Source", 4, 0),
    ParamInfo::float("env_sens", "Envelope Sensitivity", 0.0, 100.0, 50.0),
    ParamInfo::float("env_att", "Envelope Attack", 1.0, 100.0, 10.0),
    ParamInfo::float("env_rel", "Envelope Release", 20.0, 2000.0, 300.0),
    ParamInfo::float("stereo_phase", "Stereo Phase", 0.0, 180.0, 90.0),
    ParamInfo::enumeration("sync", "Sync", 17, 0),
    ParamInfo::float("color", "Color", 0.0, 100.0, 50.0),
    ParamInfo::enumeration("lfo2", "Second LFO", 3, 0),
    ParamInfo::float("rate2", "Rate 2", 0.05, 8.0, 0.3),
    ParamInfo::enumeration("routing", "Routing", 2, 0),
    ParamInfo::float("lamp_lag", "Lamp Lag", 5.0, 100.0, 30.0),
    ParamInfo::enumeration("vibe_mode", "Vibe Mode", 2, 0),
    ParamInfo::float("tone", "Tone", 0.0, 100.0, 50.0),
    ParamInfo::float("bass_keep", "Bass Keep", 0.0, 300.0, 0.0),
];

pub mod p {
    pub const RATE: usize = 0;
    pub const LOW: usize = 1;
    pub const HIGH: usize = 2;
    pub const FEEDBACK: usize = 3;
    pub const WET: usize = 4;
    pub const MODEL: usize = 5;
    pub const STAGES: usize = 6;
    pub const SPREAD: usize = 7;
    pub const FB_POLARITY: usize = 8;
    pub const DRY: usize = 9;
    pub const MANUAL: usize = 10;
    pub const DEPTH: usize = 11;
    pub const SHAPE: usize = 12;
    pub const SKEW: usize = 13;
    pub const SWEEP_SRC: usize = 14;
    pub const ENV_SENS: usize = 15;
    pub const ENV_ATT: usize = 16;
    pub const ENV_REL: usize = 17;
    pub const STEREO_PHASE: usize = 18;
    pub const SYNC: usize = 19;
    pub const COLOR: usize = 20;
    pub const LFO2: usize = 21;
    pub const RATE2: usize = 22;
    pub const ROUTING: usize = 23;
    pub const LAMP_LAG: usize = 24;
    pub const VIBE_MODE: usize = 25;
    pub const TONE: usize = 26;
    pub const BASS_KEEP: usize = 27;
    pub const N: usize = 28;
}

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

#[inline]
fn idx(v: f32, n: usize) -> usize {
    (v.round().max(0.0) as usize).min(n - 1)
}

#[inline]
fn db(v: f32) -> f32 {
    if v <= -60.0 {
        0.0
    } else {
        10f32.powf(v / 20.0)
    }
}

/// What a structural change switches (crossfaded through a dip of the wet).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Structure {
    model: Model,
    stages: usize,
    routing: usize,
    lfo2: usize,
    vibrato: bool,
}

/// A cascade of first-order TPT all-passes with zero-delay feedback.
#[derive(Clone, Copy)]
struct Cascade {
    s: [f32; MAX_STAGES],
    /// fixed per-stage tolerance (JFET / OTA mismatch), ±1
    tol: [f32; MAX_STAGES],
}

impl Default for Cascade {
    fn default() -> Self {
        Self { s: [0.0; MAX_STAGES], tol: [0.0; MAX_STAGES] }
    }
}

impl Cascade {
    fn reset(&mut self, rng: &mut Rng) {
        self.s = [0.0; MAX_STAGES];
        for t in self.tol.iter_mut() {
            *t = rng.bipolar();
        }
    }

    /// One sample: `n` stages at base frequency `f` (Hz), stage k at f · mult(k). `k_fb` loop gain,
    /// `sat` input saturation amount (0 = linear). Returns the cascade's output.
    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn process(&mut self, x: f32, n: usize, f: f32, spread: f32, mismatch: f32, k_fb: f32, sat: f32, sr: f32) -> f32 {
        let mut gs = [0.0f32; MAX_STAGES];
        // A, B: the cascade as y = A·x + B from its states
        let (mut a, mut b) = (1.0f32, 0.0f32);
        for i in 0..n {
            let pos = if n > 1 { i as f32 / (n - 1) as f32 - 0.5 } else { 0.0 };
            let m = (2.0f32).powf(2.0 * spread * pos) * (1.0 + mismatch * self.tol[i]);
            let fc = (f * m).clamp(5.0, 0.49 * sr);
            let g = (std::f32::consts::PI * fc / sr).tan();
            let gg = g / (1.0 + g);
            gs[i] = gg;
            // stage: y = (2G − 1)·x + 2(1 − G)·s
            let (ai, bi) = (2.0 * gg - 1.0, 2.0 * (1.0 - gg) * self.s[i]);
            a *= ai;
            b = ai * b + bi;
        }
        // zero-delay feedback: y = A (x + k y) + B
        let y = (a * x + b) / (1.0 - k_fb * a);
        let mut v = x + k_fb * y;
        if sat > 0.0 {
            // faded in: any saturation, however small, would otherwise jump from v to tanh(v)
            let kd = 1.0 + 3.0 * sat;
            v += ((v * kd).tanh() / kd - v) * (10.0 * sat).min(1.0);
        }
        for i in 0..n {
            let gg = gs[i];
            let vv = gg * (v - self.s[i]);
            let lp = vv + self.s[i];
            self.s[i] = lp + vv;
            v = 2.0 * lp - v;
        }
        v
    }
}

#[derive(Clone)]
struct Channel {
    a: Cascade,
    b: Cascade,
    vibe: Vibe,
    lamp: Lamp,
    tone_lp: Svf,
    tone_hp: Svf,
    lo: [Svf; 2],
    rng: Rng,
    rnd_from: f64,
    rnd_to: f64,
    last_phase: f64,
    steps_smooth: f64,
    u_smooth: f64,
}

impl Channel {
    fn new(seed: u64) -> Self {
        Self {
            a: Cascade::default(),
            b: Cascade::default(),
            vibe: Vibe::default(),
            lamp: Lamp::default(),
            tone_lp: Svf::new(),
            tone_hp: Svf::new(),
            lo: Default::default(),
            rng: Rng::new(seed),
            rnd_from: 0.0,
            rnd_to: 0.0,
            last_phase: 0.0,
            steps_smooth: 0.0,
            u_smooth: 0.0,
        }
    }

    fn reset(&mut self) {
        self.rng.reset();
        self.a.reset(&mut self.rng);
        self.b.reset(&mut self.rng);
        self.vibe.reset();
        self.lamp.reset();
        self.tone_lp.reset();
        self.tone_hp.reset();
        for f in self.lo.iter_mut() {
            f.reset();
        }
        self.rnd_from = 0.0;
        self.rnd_to = self.rng.next_f64();
        self.last_phase = 0.0;
        self.steps_smooth = 0.0;
        self.u_smooth = f64::NAN;
    }
}

/// Unipolar LFO (0 … 1) at phase p (already skewed); every shape starts at its bottom.
#[inline]
fn lfo(p: f64, shape: u32, ch: &Channel) -> f64 {
    let tri = 1.0 - (1.0 - 2.0 * p).abs();
    let a = 3.0;
    match shape {
        1 => tri,
        2 => 1.0 - ((a * (1.0 - tri)).exp() - 1.0) / (a.exp() - 1.0),
        3 => ((a * tri).exp() - 1.0) / (a.exp() - 1.0),
        // smoothed square: a steep sine-clipped wave
        4 => 0.5 + 0.5 * (6.0 * -(std::f64::consts::TAU * p).cos()).tanh() / 6f64.tanh(),
        5 => p,
        6 => ch.rnd_from + (ch.rnd_to - ch.rnd_from) * 0.5 * (1.0 - (std::f64::consts::PI * p).cos()),
        7 => ch.rnd_to,
        _ => 0.5 - 0.5 * (std::f64::consts::TAU * p).cos(),
    }
}

#[inline]
fn skew(p: f64, s: f64) -> f64 {
    let s = s.clamp(0.05, 0.95);
    if p < s {
        0.5 * p / s
    } else {
        0.5 + 0.5 * (p - s) / (1.0 - s)
    }
}

pub struct Phaser {
    s: Settings,
    sr: f32,
    st: Structure,
    pending: Option<Structure>,
    gate: LinearRamp,
    ch: [Channel; 2],
    env: PeakFollower,
    phase: f64,
    phase2: f64,
    transport: TransportInfo,
    rate: ExpSmoother,
    feedback: ExpSmoother,
    manual: ExpSmoother,
    depth: ExpSmoother,
    low: OnePole,
    high: OnePole,
    spread: OnePole,
    wet: OnePole,
    dry: OnePole,
    color: OnePole,
    tone: OnePole,
    sphase: OnePole,
    bass: OnePole,
    bk_amt: OnePole,
    pol: OnePole,
    ctrl: u32,
    clock: u64,
}

const CTRL: u32 = 32;

impl Phaser {
    pub fn new() -> Self {
        let s = Settings::default();
        let v = s.v;
        Self {
            s,
            sr: 48000.0,
            st: Self::structure(&s),
            pending: None,
            gate: LinearRamp::new(15.0, 1.0),
            ch: [Channel::new(0x9A1), Channel::new(0x9A2)],
            env: PeakFollower::new(10.0, 300.0),
            phase: 0.0,
            phase2: 0.0,
            transport: TransportInfo::default(),
            rate: ExpSmoother::new(0.05, 8.0, v[p::RATE]),
            feedback: ExpSmoother::new(-60.0, -1.0, v[p::FEEDBACK]),
            manual: ExpSmoother::new(0.0, 100.0, v[p::MANUAL]),
            depth: ExpSmoother::new(0.0, 100.0, v[p::DEPTH]),
            low: OnePole::new(30.0, v[p::LOW].ln(), 1e-6),
            high: OnePole::new(30.0, v[p::HIGH].ln(), 1e-6),
            spread: OnePole::new(30.0, 0.0, 1e-6),
            wet: OnePole::new(20.0, v[p::WET], 1e-4),
            dry: OnePole::new(20.0, v[p::DRY], 1e-4),
            color: OnePole::new(30.0, 0.5, 1e-6),
            tone: OnePole::new(30.0, 0.5, 1e-6),
            sphase: OnePole::new(50.0, v[p::STEREO_PHASE], 1e-4),
            bass: OnePole::new(30.0, 100.0, 1e-3),
            bk_amt: OnePole::new(20.0, 0.0, 1e-6),
            pol: OnePole::new(10.0, 1.0, 1e-6),
            ctrl: 0,
            clock: 0,
        }
    }

    fn structure(s: &Settings) -> Structure {
        Structure {
            model: Model::ALL[idx(s.v[p::MODEL], 9)],
            stages: STAGES[idx(s.v[p::STAGES], 7)],
            routing: idx(s.v[p::ROUTING], 2),
            lfo2: idx(s.v[p::LFO2], 3),
            vibrato: idx(s.v[p::VIBE_MODE], 2) == 1,
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn phase(&self) -> f64 {
        self.phase
    }

    pub fn set(&mut self, s: Settings) {
        let v = &s.v;
        let old = self.s.v;
        if v[p::RATE] != old[p::RATE] {
            self.rate.set_target(v[p::RATE].clamp(0.05, 8.0), self.clock);
        }
        if v[p::FEEDBACK] != old[p::FEEDBACK] {
            self.feedback.set_target(v[p::FEEDBACK].clamp(-60.0, -1.0), self.clock);
        }
        if v[p::MANUAL] != old[p::MANUAL] {
            self.manual.set_target(v[p::MANUAL].clamp(0.0, 100.0), self.clock);
        }
        if v[p::DEPTH] != old[p::DEPTH] {
            self.depth.set_target(v[p::DEPTH].clamp(0.0, 100.0), self.clock);
        }
        self.set_targets(&s);
        let st = Self::structure(&s);
        let target = self.pending.unwrap_or(self.st);
        if st != target {
            if st == self.st {
                self.pending = None;
                self.gate.set_target(1.0);
            } else {
                self.pending = Some(st);
                self.gate.set_target(0.0);
            }
        }
        self.s = s;
    }

    fn set_targets(&mut self, s: &Settings) {
        let v = &s.v;
        self.low.set_target(v[p::LOW].clamp(20.0, 4000.0).ln());
        self.high.set_target(v[p::HIGH].clamp(100.0, 16000.0).ln());
        self.spread.set_target(v[p::SPREAD] * 0.01);
        self.wet.set_target(v[p::WET]);
        self.dry.set_target(v[p::DRY]);
        self.color.set_target(v[p::COLOR] * 0.01);
        self.tone.set_target(v[p::TONE] * 0.01);
        self.sphase.set_target(v[p::STEREO_PHASE]);
        if v[p::BASS_KEEP] >= 20.0 {
            self.bass.set_target(v[p::BASS_KEEP]);
        }
        self.bk_amt.set_target(if v[p::BASS_KEEP] >= 20.0 { 1.0 } else { 0.0 });
        self.pol.set_target(if idx(v[p::FB_POLARITY], 2) == 1 { -1.0 } else { 1.0 });
    }

    fn control(&mut self) {
        let sr = self.sr;
        let t = self.tone.next();
        let bk = self.bass.next();
        // tone: 50 % flat; below, a low-pass falls from 20 kHz to 1 kHz; above, a high-pass rises to 2 kHz
        let lp = if t < 0.5 { 1000.0 * 20f32.powf(t * 2.0) } else { 20000.0 };
        let hp = if t > 0.5 { 20.0 * 100f32.powf((t - 0.5) * 2.0) } else { 20.0 };
        for c in self.ch.iter_mut() {
            c.tone_lp.set(SvfMode::LowPass, lp.min(0.45 * sr), std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            c.tone_hp.set(SvfMode::HighPass, hp, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            for f in c.lo.iter_mut() {
                f.set(SvfMode::LowPass, bk, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            }
        }
        let v = &self.s.v;
        self.env.set_times(v[p::ENV_ATT].max(1.0), v[p::ENV_REL].max(20.0), sr);
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(2);
        let n = buffers.first().map_or(0, |b| b.len());
        let sr = self.sr;
        let srd = sr as f64;
        let beats = Sync::from_index(idx(self.s.v[p::SYNC], 17)).beats();
        let src = idx(self.s.v[p::SWEEP_SRC], 4);
        let shape = idx(self.s.v[p::SHAPE], 8) as u32;
        let skew_v = self.s.v[p::SKEW] as f64 * 0.01;
        let sens = self.s.v[p::ENV_SENS] * 0.01;
        let rate2 = self.s.v[p::RATE2].clamp(0.05, 8.0) as f64;
        let lamp_lag = self.s.v[p::LAMP_LAG];
        let steps_k = (-1.0 / (0.002 * srd)).exp();
        let u_k = (-1.0 / (0.01 * srd)).exp();
        for i in 0..n {
            self.clock += 1;
            if self.ctrl == 0 {
                self.control();
            }
            self.ctrl = (self.ctrl + 1) % CTRL;
            let g = self.gate.next();
            if g == 0.0 && self.gate.is_settled() {
                if let Some(st) = self.pending.take() {
                    self.st = st;
                    for c in self.ch.iter_mut() {
                        c.reset();
                    }
                }
                self.gate.set_target(1.0);
            }
            let gs = 0.5 - 0.5 * (std::f32::consts::PI * g).cos();
            let rate = self.rate.next() as f64;
            let fb_db = self.feedback.next();
            let manual = self.manual.next() as f64 * 0.01;
            let depth = self.depth.next() as f64 * 0.01;
            let (lo_ln, hi_ln) = (self.low.next(), self.high.next());
            let spread = self.spread.next();
            let wet_g = db(self.wet.next());
            let dry_g = db(self.dry.next());
            let color = self.color.next();
            let sph = self.sphase.next() as f64 / 360.0;
            let bk = self.bk_amt.next();
            let pol = self.pol.next();
            let tone_amt = ((self.tone.value() - 0.5).abs() * 20.0).min(1.0);
            let st = self.st;
            // the Vibe's rate drifts up with intensity (DAFx-19)
            let rate = if st.model == Model::Vibe { rate * (1.0 + 0.15 * depth) } else { rate };
            let hz = match beats {
                Some(b) => self.transport.bpm() / 60.0 / b,
                None => rate,
            };
            match beats.and_then(|b| self.transport.phase_at(i, b, 0.0, sr)) {
                Some(ph) => self.phase = ph,
                None => self.phase = (self.phase + hz / srd).fract(),
            }
            self.phase2 = (self.phase2 + rate2 / srd).fract();
            let mut lvl_in = 0.0f32;
            for c in 0..nch {
                lvl_in = lvl_in.max(buffers[c][i].abs());
            }
            let e = self.env.next(lvl_in);
            let floor = -20.0 - 40.0 * sens;
            let u_env = ((20.0 * e.max(1e-6).log10() - floor) / 30.0).clamp(0.0, 1.0) as f64;
            // feedback by model; colour: 0 = ideal, 0.5 = the model's own, 1 = twice that
            let k_user = pol * db(fb_db).min(0.95);
            let (k_fb, sat, mismatch) = match st.model {
                // these are ideal at 50 % (their own character is linear); colour above adds saturation
                Model::Classic | Model::Deep12 | Model::BarberUp | Model::BarberDown | Model::BiPhase => (k_user, 0.4 * (color - 0.5).max(0.0), 0.0),
                Model::Script90 => (0.0, 0.6 * color, 0.06 * color),
                Model::Block90 => (0.35, 0.6 * color, 0.06 * color),
                Model::Stone => (if color > 0.5 { 1.4 * (color - 0.5) } else { 0.0 }, 0.4 * color, 0.04 * color),
                Model::Vibe => (0.0, 0.0, 0.0),
            };
            let n_st = match st.model {
                Model::Classic | Model::BarberUp | Model::BarberDown => st.stages,
                Model::Deep12 => 12,
                Model::BiPhase => 6,
                _ => 4,
            };
            for c in 0..nch {
                let x = buffers[c][i];
                let chs = &mut self.ch[c];
                let ph = skew(if c == 0 { self.phase } else { (self.phase + sph).fract() }, skew_v);
                if ph < chs.last_phase && chs.last_phase - ph > 0.5 {
                    chs.rnd_from = chs.rnd_to;
                    chs.rnd_to = chs.rng.next_f64();
                }
                chs.last_phase = ph;
                let mut u_lfo = lfo(ph, shape, chs);
                if shape == 7 {
                    chs.steps_smooth = u_lfo + (chs.steps_smooth - u_lfo) * steps_k;
                    u_lfo = chs.steps_smooth;
                }
                let u_src = match src {
                    1 => u_env,
                    2 => manual,
                    3 => (u_lfo + u_env).min(1.0),
                    _ => u_lfo,
                };
                let u = if src == 2 { manual } else { (1.0 - depth) * manual + depth * u_src };
                // the position glides (~10 ms): source changes sweep instead of jumping
                chs.u_smooth = if chs.u_smooth.is_nan() { u } else { u + (chs.u_smooth - u) * u_k };
                let u = chs.u_smooth;
                // bass keep: complementary split, the lows bypass the phaser
                let l0 = chs.lo[0].process(x);
                let lo = chs.lo[1].process(l0);
                let xin = x - bk * lo;
                let fexp = |u: f64| (lo_ln + (hi_ln - lo_ln) * u as f32).exp();
                let wet = match st.model {
                    Model::Vibe => {
                        let r = chs.lamp.next(u as f32, depth.max(0.05) as f32, lamp_lag, sr);
                        chs.vibe.process(xin, r, sr)
                    }
                    Model::BiPhase => {
                        let u2 = match st.lfo2 {
                            1 => 1.0 - u,
                            2 => (1.0 - depth) * manual + depth * lfo(self.phase2, shape, chs),
                            _ => u,
                        };
                        if st.routing == 0 {
                            let a = chs.a.process(xin, n_st, fexp(u), spread, mismatch, k_fb, sat, sr);
                            chs.b.process(a, n_st, fexp(u2), spread, mismatch, k_fb, sat, sr)
                        } else if nch == 1 {
                            // parallel on a mono output: both phasers, averaged
                            let a = chs.a.process(xin, n_st, fexp(u), spread, mismatch, k_fb, sat, sr);
                            0.5 * (a + chs.b.process(xin, n_st, fexp(u2), spread, mismatch, k_fb, sat, sr))
                        } else if c == 0 {
                            chs.a.process(xin, n_st, fexp(u), spread, mismatch, k_fb, sat, sr)
                        } else {
                            chs.b.process(xin, n_st, fexp(u2), spread, mismatch, k_fb, sat, sr)
                        }
                    }
                    Model::BarberUp | Model::BarberDown => {
                        let mut y = 0.0;
                        for (k, cas) in [&mut chs.a, &mut chs.b].into_iter().enumerate() {
                            let r = (ph + 0.5 * k as f64).fract();
                            let ramp = if st.model == Model::BarberUp { r } else { 1.0 - r };
                            let w = (std::f64::consts::PI * r).sin().powi(2) as f32;
                            y += w * cas.process(xin, n_st, fexp(ramp * depth + (1.0 - depth) * manual), spread, 0.0, k_fb, sat, sr);
                        }
                        y
                    }
                    _ => chs.a.process(xin, n_st, fexp(u), spread, mismatch, k_fb, sat, sr),
                };
                // tone is exactly flat at 50 % (the filters fade in over the first 5 % either side)
                let toned = chs.tone_hp.process(chs.tone_lp.process(wet));
                let wet = wet + (toned - wet) * tone_amt;
                // Vibe vibrato: wet only
                let (dg, wg) = if st.model == Model::Vibe && st.vibrato { (0.0, wet_g) } else { (dry_g, wet_g) };
                buffers[c][i] = bk * lo + dg * xin + wg * gs * wet;
            }
        }
    }
}

impl Default for Phaser {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Phaser {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        for e in [&mut self.rate, &mut self.feedback, &mut self.manual, &mut self.depth] {
            e.prepare(sample_rate);
        }
        for o in [&mut self.low, &mut self.high, &mut self.spread, &mut self.wet, &mut self.dry, &mut self.color, &mut self.sphase, &mut self.bk_amt, &mut self.pol] {
            o.prepare(sample_rate);
        }
        for o in [&mut self.tone, &mut self.bass] {
            o.prepare(sample_rate / CTRL as f32);
        }
        self.gate.prepare(sample_rate);
        self.env.prepare(sample_rate);
        self.control();
    }

    fn reset(&mut self) {
        if let Some(st) = self.pending.take() {
            self.st = st;
        }
        let v = self.s.v;
        self.rate.settle(v[p::RATE]);
        self.feedback.settle(v[p::FEEDBACK].clamp(-60.0, -1.0));
        self.manual.settle(v[p::MANUAL]);
        self.depth.settle(v[p::DEPTH]);
        let s = self.s;
        self.set_targets(&s);
        for o in [&mut self.low, &mut self.high, &mut self.spread, &mut self.wet, &mut self.dry, &mut self.color, &mut self.tone, &mut self.sphase, &mut self.bass, &mut self.bk_amt, &mut self.pol] {
            o.settle();
        }
        self.gate.set_immediate(1.0);
        for c in self.ch.iter_mut() {
            c.reset();
        }
        self.env.reset();
        self.phase = 0.0;
        self.phase2 = 0.0;
        self.ctrl = 0;
        self.clock = 0;
        self.control();
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
