// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-delay — golem's delay (design: README.md).
//!
//! Stands for golem's `delay` block (ids `time` as note values, `feedback`, `mix` kept).
//!
//! Engines: Digital, Tape, BBD and Lo-Fi, each a bundle of loop colour, modulation and saturation.
//!
//! The loop carries everything, so effects combine:
//! - taps (Space Echo heads, golden ratio, The Edge's dotted + quarter);
//! - reverse windows;
//! - a pitch shifter (crystal and shimmer repeats) and a frequency shifter (ghost echoes);
//! - low and high cut, a filter sweep, diffusion, wear (`age`);
//! - bounded saturation, so feedback up to 110 % self-oscillates without running away.
//!
//! Around the loop:
//! - time changes glide (tape pitch bend) or crossfade between two heads;
//! - ducking, swell, hold;
//! - mono / stereo / ping-pong / dual;
//! - trails through the bypass parameter.
//!
//! Zero latency. Modulation and noise restart from phase 0 and fixed seeds on reset.

#![allow(clippy::needless_range_loop)]

use gfx_dsp::adaa::{Adaa1, Shaper};
use gfx_dsp::delay::{DelayLine, Interp};
use gfx_dsp::envelope::PeakFollower;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::lfo::Rng;
use gfx_dsp::pitch::{FreqShifter, GrainShifter};
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
mod plugin;

/// The line holds 8 s.
const MAX_S: f64 = 8.0;
const CTRL: u32 = 32;
const TAU: f64 = std::f64::consts::TAU;

/// `time` options in quarter notes, catalogue order first; the last one is Free.
pub const NOTES: [f64; 9] = [1.5, 1.0, 0.75, 0.5, 1.0 / 3.0, 0.25, 2.0, 2.0 / 3.0, 0.375];
pub const TIME_FREE: usize = 9;

/// Tap patterns: playback heads as fractions of the delay time.
pub const TAPS: [&[f64]; 8] = [
    &[1.0],
    &[1.0 / 3.0, 2.0 / 3.0],
    &[1.0 / 3.0, 2.0 / 3.0, 1.0],
    &[1.0 / 3.0, 1.0],
    &[0.382, 0.618, 1.0],
    &[2.0 / 3.0, 1.0],
    &[0.75, 1.0],
    &[0.5833, 1.0],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    Digital,
    Tape,
    Bbd,
    LoFi,
}

impl Model {
    pub const ALL: [Model; 4] = [Model::Digital, Model::Tape, Model::Bbd, Model::LoFi];
    /// Auto time change: tape and BBD glide, digital crossfades.
    fn glides(self) -> bool {
        matches!(self, Model::Tape | Model::Bbd)
    }
}

pub static PARAMS: [ParamInfo; 29] = [
    ParamInfo::enumeration("time", "Time", 10, 2),
    ParamInfo::float("feedback", "Feedback", 0.0, 110.0, 35.0).with_exp(),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 25.0).with_exp(),
    ParamInfo::enumeration("model", "Model", 4, 0),
    ParamInfo::float("time_ms", "Time (free)", 1.0, 4000.0, 375.0).with_exp(),
    ParamInfo::enumeration("glide", "Time Change", 3, 0),
    ParamInfo::float("glide_ms", "Glide Time", 10.0, 1000.0, 150.0),
    ParamInfo::float("low_cut", "Low Cut", 20.0, 1000.0, 80.0),
    ParamInfo::float("high_cut", "High Cut", 1000.0, 20000.0, 20000.0).with_exp(),
    ParamInfo::float("drive", "Drive", 0.0, 100.0, 0.0),
    ParamInfo::float("mod", "Modulation", 0.0, 100.0, 50.0),
    ParamInfo::float("diffusion", "Diffusion", 0.0, 100.0, 0.0),
    ParamInfo::enumeration("taps", "Taps", 8, 0),
    ParamInfo::enumeration("direction", "Direction", 2, 0),
    ParamInfo::float("loop_pitch", "Loop Pitch", -12.0, 12.0, 0.0),
    ParamInfo::float("loop_shift", "Loop Freq Shift", 0.0, 100.0, 50.0),
    ParamInfo::float("age", "Age", 0.0, 100.0, 0.0),
    ParamInfo::float("swell", "Swell", 0.0, 2000.0, 0.0),
    ParamInfo::float("sweep", "Loop Filter Sweep", 0.0, 100.0, 0.0),
    ParamInfo::float("duck", "Ducking", 0.0, 24.0, 0.0).with_exp(),
    ParamInfo::float("duck_thresh", "Duck Threshold", -60.0, 0.0, -30.0),
    ParamInfo::float("duck_release", "Duck Release", 50.0, 2000.0, 300.0),
    ParamInfo::enumeration("stereo", "Stereo", 4, 0),
    ParamInfo::float("ratio", "Dual Ratio", 0.5, 2.0, 1.5),
    ParamInfo::float("width", "Width", 0.0, 100.0, 100.0),
    ParamInfo::boolean("kill_dry", "Kill Dry", false),
    ParamInfo::boolean("hold", "Hold", false),
    ParamInfo::boolean("bypass", "Bypass", false),
    ParamInfo::boolean("trails", "Trails", true),
];

pub mod p {
    pub const TIME: usize = 0;
    pub const FEEDBACK: usize = 1;
    pub const MIX: usize = 2;
    pub const MODEL: usize = 3;
    pub const TIME_MS: usize = 4;
    pub const GLIDE: usize = 5;
    pub const GLIDE_MS: usize = 6;
    pub const LOW_CUT: usize = 7;
    pub const HIGH_CUT: usize = 8;
    pub const DRIVE: usize = 9;
    pub const MOD: usize = 10;
    pub const DIFFUSION: usize = 11;
    pub const TAPS: usize = 12;
    pub const DIRECTION: usize = 13;
    pub const LOOP_PITCH: usize = 14;
    pub const LOOP_SHIFT: usize = 15;
    pub const AGE: usize = 16;
    pub const SWELL: usize = 17;
    pub const SWEEP: usize = 18;
    pub const DUCK: usize = 19;
    pub const DUCK_THRESH: usize = 20;
    pub const DUCK_RELEASE: usize = 21;
    pub const STEREO: usize = 22;
    pub const RATIO: usize = 23;
    pub const WIDTH: usize = 24;
    pub const KILL_DRY: usize = 25;
    pub const HOLD: usize = 26;
    pub const BYPASS: usize = 27;
    pub const TRAILS: usize = 28;
    pub const N: usize = 29;
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

/// Schroeder all-pass for diffusion.
#[derive(Clone, Default)]
struct Diffuser {
    lines: [DelayLine; 4],
}

const DIFF_MS: [f64; 4] = [3.1, 4.7, 7.3, 11.3];

impl Diffuser {
    fn prepare(&mut self, sr: f32) {
        for (l, ms) in self.lines.iter_mut().zip(DIFF_MS) {
            l.prepare((ms * 0.001 * sr as f64) as usize + 4);
        }
    }
    fn reset(&mut self) {
        for l in self.lines.iter_mut() {
            l.reset();
        }
    }
    #[inline]
    fn process(&mut self, x: f32, g: f32, sr: f32) -> f32 {
        let mut v = x;
        for (l, ms) in self.lines.iter_mut().zip(DIFF_MS) {
            let d = (ms * 0.001 * sr as f64) as usize;
            let z = l.tap(d.saturating_sub(1));
            let w = v + g * z;
            l.push(w);
            v = z - g * w;
        }
        v
    }
}

/// One loop (one channel's line and everything inside its feedback).
#[derive(Clone)]
struct Loop {
    line: DelayLine,
    /// time change: heads a → b, crossfade position (1 = settled on b)
    head_a: f64,
    head_b: f64,
    xf: f64,
    /// glide: the current delay (samples)
    glide_d: f64,
    rev_ph: f64,
    shifter: GrainShifter,
    fshift: FreqShifter,
    hp: Svf,
    lp: Svf,
    model_lp: Svf,
    bump: Svf,
    diff: Diffuser,
    adaa: Adaa1,
    dc_x: f32,
    dc_y: f32,
    fb: f32,
    hold_sr: f32,
    hold_n: u32,
    rng: Rng,
    drop_g: f32,
    drop_t: f32,
    swell_g: f32,
    env_fast: f32,
    env_slow: f32,
}

impl Loop {
    fn new(seed: u64) -> Self {
        Self {
            line: DelayLine::new(),
            head_a: 0.0,
            head_b: 0.0,
            xf: 1.0,
            glide_d: 0.0,
            rev_ph: 0.0,
            shifter: GrainShifter::new(),
            fshift: FreqShifter::new(),
            hp: Svf::new(),
            lp: Svf::new(),
            model_lp: Svf::new(),
            bump: Svf::new(),
            diff: Diffuser::default(),
            adaa: Adaa1::new(),
            dc_x: 0.0,
            dc_y: 0.0,
            fb: 0.0,
            hold_sr: 0.0,
            hold_n: 0,
            rng: Rng::new(seed),
            drop_g: 1.0,
            drop_t: 1.0,
            swell_g: 1.0,
            env_fast: 0.0,
            env_slow: 0.0,
        }
    }

    fn reset(&mut self, d: f64) {
        self.line.reset();
        self.head_a = d;
        self.head_b = d;
        self.xf = 1.0;
        self.glide_d = d;
        self.rev_ph = 0.0;
        self.shifter.reset();
        self.fshift.reset();
        for f in [&mut self.hp, &mut self.lp, &mut self.model_lp, &mut self.bump] {
            f.reset();
        }
        self.diff.reset();
        self.adaa.reset(Shaper::Tanh);
        self.dc_x = 0.0;
        self.dc_y = 0.0;
        self.fb = 0.0;
        self.hold_sr = 0.0;
        self.hold_n = 0;
        self.rng.reset();
        self.drop_g = 1.0;
        self.drop_t = 1.0;
        self.swell_g = 1.0;
        self.env_fast = 0.0;
        self.env_slow = 0.0;
    }
}

pub struct Delay {
    s: Settings,
    sr: f32,
    // per-sample coefficients fixed in time, not in samples (0.999 / 0.9995 at 48 kHz, as before; they were that at every rate)
    duck_k: f32,
    dc_k: f32,
    lp_: [Loop; 2],
    transport: TransportInfo,
    feedback: ExpSmoother,
    mix: ExpSmoother,
    time_ms: ExpSmoother,
    high_cut: ExpSmoother,
    duck: ExpSmoother,
    low_cut: OnePole,
    drive: OnePole,
    modu: OnePole,
    diffusion: OnePole,
    pitch: OnePole,
    shift: OnePole,
    age: OnePole,
    sweep: OnePole,
    width: OnePole,
    ratio: OnePole,
    dry_g: OnePole,
    hold_g: OnePole,
    /// the shifters fade in and out on their target (off = exactly 0 st / 50 %), never on the glide
    pitch_amt: OnePole,
    shift_amt: OnePole,
    in_g: LinearRamp,
    wet_g: LinearRamp,
    /// structural changes (model, stereo, taps, direction) dip the wet 20 ms
    gate: LinearRamp,
    pending: bool,
    model: Model,
    stereo: usize,
    taps: usize,
    reverse: bool,
    duck_env: PeakFollower,
    duck_gr: f32,
    wow_ph: f64,
    flutter_ph: f64,
    sweep_ph: f64,
    drift: f64,
    drift_t: f64,
    rng: Rng,
    was_on: bool,
    /// the loop colour's group delay at low frequencies (samples): the read heads come that much
    /// earlier, so the repeats land on the beat
    comp_d: f64,
    ctrl: u32,
    clock: u64,
}

impl Delay {
    pub fn new() -> Self {
        let s = Settings::default();
        let v = s.v;
        Self {
            s,
            sr: 48000.0,
            duck_k: 0.999,
            dc_k: 0.9995,
            lp_: [Loop::new(0xD1), Loop::new(0xD2)],
            transport: TransportInfo::default(),
            feedback: ExpSmoother::new(0.0, 110.0, v[p::FEEDBACK]),
            mix: ExpSmoother::new(0.0, 100.0, v[p::MIX]),
            time_ms: ExpSmoother::new(1.0, 4000.0, v[p::TIME_MS]),
            high_cut: ExpSmoother::new(1000.0, 20000.0, v[p::HIGH_CUT]),
            duck: ExpSmoother::new(0.0, 24.0, v[p::DUCK]),
            low_cut: OnePole::new(30.0, v[p::LOW_CUT], 1e-3),
            drive: OnePole::new(30.0, v[p::DRIVE], 1e-4),
            modu: OnePole::new(50.0, v[p::MOD], 1e-4),
            diffusion: OnePole::new(30.0, v[p::DIFFUSION], 1e-4),
            pitch: OnePole::new(50.0, v[p::LOOP_PITCH], 1e-5),
            shift: OnePole::new(50.0, v[p::LOOP_SHIFT], 1e-4),
            age: OnePole::new(50.0, v[p::AGE], 1e-4),
            sweep: OnePole::new(30.0, v[p::SWEEP], 1e-4),
            width: OnePole::new(30.0, v[p::WIDTH], 1e-4),
            ratio: OnePole::new(50.0, v[p::RATIO], 1e-6),
            dry_g: OnePole::new(5.0, 1.0, 1e-6),
            hold_g: OnePole::new(20.0, 0.0, 1e-6),
            pitch_amt: OnePole::new(20.0, 0.0, 1e-6),
            shift_amt: OnePole::new(20.0, 0.0, 1e-6),
            in_g: LinearRamp::new(5.0, 1.0),
            wet_g: LinearRamp::new(20.0, 1.0),
            gate: LinearRamp::new(20.0, 1.0),
            pending: false,
            model: Model::Digital,
            stereo: 0,
            taps: 0,
            reverse: false,
            duck_env: PeakFollower::new(5.0, 300.0),
            duck_gr: 1.0,
            wow_ph: 0.0,
            flutter_ph: 0.0,
            sweep_ph: 0.0,
            drift: 0.0,
            drift_t: 0.0,
            rng: Rng::new(0xDE1A),
            was_on: true,
            comp_d: 0.0,
            ctrl: 0,
            clock: 0,
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    /// The delay time the settings ask for (samples), at the host tempo.
    pub fn target_samples(&self) -> f64 {
        let t = idx(self.s.v[p::TIME], 10);
        let s = if t == TIME_FREE {
            self.time_ms.value() as f64 * 0.001 * self.sr as f64
        } else {
            self.transport.note_samples(NOTES[t], self.sr)
        };
        s.clamp(1.0, MAX_S * self.sr as f64 - 8.0)
    }

    /// The main head's current delay (samples) on the left loop.
    pub fn current_samples(&self) -> f64 {
        let l = &self.lp_[0];
        if self.glides() {
            l.glide_d
        } else {
            l.head_b
        }
    }

    fn glides(&self) -> bool {
        match idx(self.s.v[p::GLIDE], 3) {
            1 => true,
            2 => false,
            _ => self.model.glides(),
        }
    }

    pub fn set(&mut self, s: Settings) {
        let v = &s.v;
        let old = self.s.v;
        for (i, e) in [(p::FEEDBACK, &mut self.feedback), (p::MIX, &mut self.mix), (p::TIME_MS, &mut self.time_ms), (p::HIGH_CUT, &mut self.high_cut), (p::DUCK, &mut self.duck)] {
            if v[i] != old[i] {
                e.set_target(v[i].clamp(PARAMS[i].min, PARAMS[i].max), self.clock);
            }
        }
        self.low_cut.set_target(v[p::LOW_CUT]);
        self.drive.set_target(v[p::DRIVE]);
        self.modu.set_target(v[p::MOD]);
        self.diffusion.set_target(v[p::DIFFUSION]);
        self.pitch.set_target(v[p::LOOP_PITCH]);
        self.shift.set_target(v[p::LOOP_SHIFT]);
        self.age.set_target(v[p::AGE]);
        self.sweep.set_target(v[p::SWEEP]);
        self.width.set_target(v[p::WIDTH]);
        self.ratio.set_target(v[p::RATIO]);
        self.dry_g.set_target(if v[p::KILL_DRY] >= 0.5 { 0.0 } else { 1.0 });
        self.hold_g.set_target(if v[p::HOLD] >= 0.5 { 1.0 } else { 0.0 });
        self.pitch_amt.set_target(if v[p::LOOP_PITCH].abs() > 1e-3 { 1.0 } else { 0.0 });
        self.shift_amt.set_target(if (v[p::LOOP_SHIFT] - 50.0).abs() > 1e-3 { 1.0 } else { 0.0 });
        let structural = Model::ALL[idx(v[p::MODEL], 4)] != self.model
            || idx(v[p::STEREO], 4) != self.stereo
            || idx(v[p::TAPS], 8) != self.taps
            || (idx(v[p::DIRECTION], 2) == 1) != self.reverse;
        if structural {
            self.pending = true;
            self.gate.set_target(0.0);
        }
        self.s = s;
    }

    fn apply_structure(&mut self) {
        let v = self.s.v;
        self.model = Model::ALL[idx(v[p::MODEL], 4)];
        self.stereo = idx(v[p::STEREO], 4);
        self.taps = idx(v[p::TAPS], 8);
        self.reverse = idx(v[p::DIRECTION], 2) == 1;
    }

    fn control(&mut self, tempo_d: f64) {
        let sr = self.sr;
        let lc = self.low_cut.value();
        let hc = self.high_cut.value();
        let sw = self.sweep.value() * 0.01;
        let age = self.age.value() * 0.01;
        // the sweep: a slow LFO moves the loop's band (0.15 Hz)
        let s = (0.5 + 0.5 * (TAU * self.sweep_ph).sin()) as f32;
        let hp_f = (lc * 2f32.powf(3.0 * sw * s)).min(4000.0);
        let lp_f = (hc * 2f32.powf(-3.0 * sw * (1.0 - s))).max(500.0).min(0.45 * sr);
        let ms = tempo_d / sr as f64 * 1000.0;
        let (mlp, bump_db) = match self.model {
            Model::Digital => (12000.0, 0.0),
            Model::Tape => (7000.0 * (1.0 - 0.6 * age), 2.5),
            // darker as the time grows: the clock slows
            Model::Bbd => ((4000.0 * (300.0 / ms.max(10.0)).sqrt() as f32).clamp(2000.0, 9000.0) * (1.0 - 0.3 * age), 0.0),
            Model::LoFi => (9000.0 * (1.0 - 0.5 * age), 0.0),
        };
        // ADAA's half sample + the model low-pass's DC group delay 1 / (Q·ω)
        self.comp_d = 0.5 + sr as f64 / (0.6 * TAU * mlp.min(0.45 * sr) as f64);
        for l in self.lp_.iter_mut() {
            l.hp.set(SvfMode::HighPass, hp_f, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            l.lp.set(SvfMode::LowPass, lp_f, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            l.model_lp.set(SvfMode::LowPass, mlp.min(0.45 * sr), 0.6, 0.0, sr);
            l.bump.set(SvfMode::Bell, 90.0, 1.0, bump_db, sr);
        }
        let v = &self.s.v;
        self.duck_env.set_times(5.0, v[p::DUCK_RELEASE].clamp(50.0, 2000.0), sr);
    }

    /// One loop, one sample: returns the loop's output (the wet) and leaves its feedback in `fb`.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn run_loop(&mut self, k: usize, write: f32, target: f64, glide_k: f64, xf_step: f64, mod_d: f64, hold: f32, fb: f32, drive: f32, diff: f32, pitch: (f32, f32), shift: (f64, f32), age: f32) -> f32 {
        let sr = self.sr;
        let dc_k = self.dc_k;
        let model = self.model;
        let taps = TAPS[self.taps];
        let reverse = self.reverse;
        let glides = self.glides();
        let l = &mut self.lp_[k];
        l.line.push(write);
        // time: glide (the head slews) or crossfade (two heads)
        if glides {
            l.glide_d = target + (l.glide_d - target) * glide_k;
            l.head_a = l.glide_d;
            l.head_b = l.glide_d;
            l.xf = 1.0;
        } else if l.xf >= 1.0 && (target - l.head_b).abs() > 0.5 {
            l.head_a = l.head_b;
            l.head_b = target;
            l.xf = 0.0;
        }
        if l.xf < 1.0 {
            l.xf = (l.xf + xf_step).min(1.0);
            if l.xf >= 1.0 {
                l.head_a = l.head_b;
            }
        }
        let comp = self.comp_d;
        let read = |l: &Loop, d: f64| l.line.read((d - comp + mod_d).clamp(4.0, l.line.max_delay() as f64 - 4.0), Interp::Lagrange3);
        let mut y = 0.0f32;
        if reverse {
            // each repeat plays its window backwards; two grains half a window apart, sin² faded
            let w = l.head_b.max(64.0);
            l.rev_ph = (l.rev_ph + 1.0 / w).fract();
            for g in 0..2 {
                let ph = (l.rev_ph + 0.5 * g as f64).fract();
                let fade = (std::f64::consts::PI * ph).sin().powi(2) as f32;
                y += fade * read(l, 4.0 + 2.0 * ph * w);
            }
        } else {
            let norm = 1.0 / taps.len() as f32;
            for &t in taps {
                let a = read(l, l.head_a * t);
                let b = if l.xf < 1.0 { read(l, l.head_b * t) } else { a };
                let x = (l.xf as f32) * b + (1.0 - l.xf as f32) * a;
                y += x * norm;
            }
        }
        // the loop's colour (bypassed by hold, so a held loop neither grows nor fades)
        let mut c = y;
        if pitch.1 > 0.0 {
            l.shifter.set_semitones(pitch.0 as f64);
            let ps = l.shifter.process(c);
            c += (ps - c) * pitch.1;
        }
        if shift.1 > 0.0 {
            let fs = l.fshift.process(c, shift.0, sr);
            c += (fs - c) * shift.1;
        }
        match model {
            Model::Tape => {
                c = l.bump.process(l.model_lp.process(c));
                // dropouts with wear: short dips at random moments (seeded)
                if age > 0.0 {
                    if l.rng.next_f64() < (age as f64) * 2.0 / sr as f64 {
                        l.drop_t = 1.0 - 0.6 * age;
                    } else if l.drop_g <= l.drop_t + 1e-3 {
                        l.drop_t = 1.0;
                    }
                    l.drop_g += (l.drop_t - l.drop_g) * 0.002;
                    c *= l.drop_g;
                }
            }
            Model::Bbd => {
                c = l.model_lp.process(c);
                if age > 0.0 {
                    c += 0.0005 * age * l.rng.bipolar();
                }
            }
            Model::LoFi => {
                // early digital: sample-rate and bit reduction inside the loop, each pass grainier
                let n = 2 + (6.0 * age) as u32;
                if l.hold_n == 0 {
                    let bits = 12.0 - 6.0 * age;
                    let q = 2f32.powf(-bits);
                    l.hold_sr = (c / q).round() * q;
                }
                l.hold_n = (l.hold_n + 1) % n;
                c = l.model_lp.process(l.hold_sr);
            }
            Model::Digital => c = l.model_lp.process(c),
        }
        c = l.lp.process(l.hp.process(c));
        if diff > 0.0 {
            let d = l.diff.process(c, 0.5 + 0.2 * diff, sr);
            c += (d - c) * diff.min(1.0);
        }
        // bounded saturation: the model's own plus `drive`; feedback above 100 % settles here
        let kd = match model {
            Model::Tape => 1.2,
            Model::Bbd => 1.0,
            _ => 0.6,
        } + 3.0 * drive;
        let sat = l.adaa.process(Shaper::Tanh, c * kd) / kd;
        let yd = sat - l.dc_x + dc_k * l.dc_y;
        l.dc_x = sat;
        l.dc_y = yd;
        let colored = yd;
        let out = colored + (y - colored) * hold;
        l.fb = out * (fb + (1.0 - fb) * hold);
        out
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(2);
        let n = buffers.first().map_or(0, |b| b.len());
        let sr = self.sr as f64;
        let on = self.s.v[p::BYPASS] < 0.5;
        let trails = self.s.v[p::TRAILS] >= 0.5;
        if on != self.was_on {
            self.in_g.set_target(if on { 1.0 } else { 0.0 });
            self.wet_g.set_target(if on || trails { 1.0 } else { 0.0 });
            self.was_on = on;
        }
        let swell_ms = self.s.v[p::SWELL];
        let duck_th = self.s.v[p::DUCK_THRESH];
        for i in 0..n {
            self.clock += 1;
            let target0 = self.target_samples();
            if self.ctrl == 0 {
                self.control(target0);
            }
            self.ctrl = (self.ctrl + 1) % CTRL;
            let g = self.gate.next();
            if g == 0.0 && self.gate.is_settled() {
                if self.pending {
                    self.apply_structure();
                    self.pending = false;
                }
                self.gate.set_target(1.0);
            }
            let gs = 0.5 - 0.5 * (std::f32::consts::PI * g).cos();
            let fb = self.feedback.next() * 0.01;
            let mix = self.mix.next() * 0.01;
            self.time_ms.next();
            self.high_cut.next();
            let duck_db = self.duck.next();
            for o in [&mut self.low_cut, &mut self.sweep] {
                o.next();
            }
            let drive = self.drive.next() * 0.01;
            let modu = self.modu.next() as f64 * 0.01;
            let diff = self.diffusion.next() * 0.01;
            let pitch = self.pitch.next();
            let shift = (self.shift.next() - 50.0) / 50.0;
            let age = self.age.next() * 0.01;
            let pitch_a = self.pitch_amt.next();
            let shift_a = self.shift_amt.next();
            let width = self.width.next() * 0.01;
            let ratio = self.ratio.next() as f64;
            let dry_g = self.dry_g.next();
            let hold = self.hold_g.next();
            let in_g = self.in_g.next() * (1.0 - hold);
            let wet_g = self.wet_g.next();
            if wet_g == 0.0 && self.wet_g.is_settled() && !on && !trails {
                // trails off: once faded, the line is cleared
                let t = self.target_samples();
                for l in self.lp_.iter_mut() {
                    l.reset(t);
                }
            }
            // frequency shift: ±50 Hz, log law
            let shift_hz = shift.signum() as f64 * (50f64.powf(shift.abs() as f64) - 1.0) * 50.0 / 49.0;
            // modulation: the model's own at 50 %, none at 0, twice at 100 % (deterministic)
            self.wow_ph = (self.wow_ph + 0.55 / sr).fract();
            self.flutter_ph = (self.flutter_ph + 6.8 / sr).fract();
            self.sweep_ph = (self.sweep_ph + 0.15 / sr).fract();
            if self.clock.is_multiple_of(4800) {
                self.drift_t = self.rng.bipolar() as f64;
            }
            self.drift += (self.drift_t - self.drift) * 1e-4;
            let m = 2.0 * modu;
            let ms = sr * 0.001;
            let mod_d = m * ms
                * match self.model {
                    Model::Tape => (0.35 * (1.0 + 2.0 * age as f64)) * (TAU * self.wow_ph).sin() + 0.03 * (TAU * self.flutter_ph).sin() + 0.15 * self.drift,
                    Model::Bbd => 0.12 * (1.0 - 4.0 * (self.wow_ph - 0.5).abs()),
                    Model::Digital => 0.01 * (TAU * self.wow_ph).sin(),
                    Model::LoFi => 0.0,
                };
            let glide_k = (-1.0 / (self.s.v[p::GLIDE_MS].max(10.0) as f64 * 0.001 * sr)).exp();
            let xf_step = 1.0 / (self.s.v[p::GLIDE_MS].max(10.0) as f64 * 0.001 * sr);
            // input: ducking detector, swell, routing
            let x0 = buffers[0][i];
            let x1 = if nch > 1 { buffers[1][i] } else { x0 };
            let mono = 0.5 * (x0 + x1);
            let e = self.duck_env.next(mono.abs().max(x0.abs()).max(x1.abs()));
            let edb = 20.0 * e.max(1e-6).log10();
            let over = ((edb - duck_th) / 6.0).clamp(0.0, 1.0);
            let duck_target = 10f32.powf(-duck_db * over / 20.0);
            self.duck_gr = duck_target + (self.duck_gr - duck_target) * self.duck_k;
            let swell = |l: &mut Loop, x: f32| -> f32 {
                if swell_ms <= 0.0 {
                    return x;
                }
                // a pick attack (fast envelope 6 dB over the slow one) restarts the swell from silence
                l.env_fast += (x.abs() - l.env_fast) * 0.05;
                l.env_slow += (x.abs() - l.env_slow) * 0.0005;
                if l.env_fast > 2.0 * l.env_slow + 1e-4 {
                    l.swell_g = 0.0;
                }
                l.swell_g = (l.swell_g + 1000.0 / (swell_ms * sr as f32)).min(1.0);
                x * l.swell_g * l.swell_g
            };
            let (inl, inr) = match self.stereo {
                1 => (x0, x1),
                _ => (mono, mono),
            };
            let inl = swell(&mut self.lp_[0], inl) * in_g;
            let inr = swell(&mut self.lp_[1], inr) * in_g;
            let target_r = if self.stereo == 3 { (target0 * ratio).min(MAX_S * sr - 8.0) } else { target0 };
            let (fbl, fbr) = (self.lp_[0].fb, self.lp_[1].fb);
            let (wl, wr) = match self.stereo {
                // ping-pong: the input enters left, each side feeds the other
                2 => {
                    let wl = self.run_loop(0, inl + fbr, target0, glide_k, xf_step, mod_d, hold, fb, drive, diff, (pitch, pitch_a), (shift_hz, shift_a), age);
                    let wr = self.run_loop(1, fbl, target0, glide_k, xf_step, mod_d, hold, fb, drive, diff, (pitch, pitch_a), (shift_hz, shift_a), age);
                    (wl, wr)
                }
                // mono: one loop, both outputs
                0 => {
                    let w = self.run_loop(0, inl + fbl, target0, glide_k, xf_step, mod_d, hold, fb, drive, diff, (pitch, pitch_a), (shift_hz, shift_a), age);
                    (w, w)
                }
                _ => {
                    let wl = self.run_loop(0, inl + fbl, target0, glide_k, xf_step, mod_d, hold, fb, drive, diff, (pitch, pitch_a), (shift_hz, shift_a), age);
                    let wr = self.run_loop(1, inr + fbr, target_r, glide_k, xf_step, mod_d, hold, fb, drive, diff, (pitch, pitch_a), (shift_hz, shift_a), age);
                    (wl, wr)
                }
            };
            // width (M/S, wet only), mix (wet level, dry at unity), ducking, trails
            let mid = 0.5 * (wl + wr);
            let side = 0.5 * (wl - wr) * width;
            let k = mix * self.duck_gr * wet_g * gs;
            let (ol, or) = (x0 * dry_g + (mid + side) * k, x1 * dry_g + (mid - side) * k);
            buffers[0][i] = if nch > 1 { ol } else { 0.5 * (ol + or) };
            if nch > 1 {
                buffers[1][i] = or;
            }
        }
    }
}

impl Default for Delay {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Delay {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        self.duck_k = 0.999f64.powf(48000.0 / sample_rate as f64) as f32;
        self.dc_k = 0.9995f64.powf(48000.0 / sample_rate as f64) as f32;
        for e in [&mut self.feedback, &mut self.mix, &mut self.time_ms, &mut self.high_cut, &mut self.duck] {
            e.prepare(sample_rate);
        }
        for o in [&mut self.low_cut, &mut self.drive, &mut self.modu, &mut self.diffusion, &mut self.pitch, &mut self.shift, &mut self.age, &mut self.sweep, &mut self.width, &mut self.ratio, &mut self.dry_g, &mut self.hold_g, &mut self.pitch_amt, &mut self.shift_amt] {
            o.prepare(sample_rate);
        }
        for r in [&mut self.in_g, &mut self.wet_g, &mut self.gate] {
            r.prepare(sample_rate);
        }
        self.duck_env.prepare(sample_rate);
        let len = (MAX_S * sample_rate as f64) as usize + 64;
        for l in self.lp_.iter_mut() {
            l.line.prepare(len);
            l.shifter.prepare(sample_rate, 60.0);
            l.diff.prepare(sample_rate);
        }
    }

    fn reset(&mut self) {
        let v = self.s.v;
        for (i, e) in [(p::FEEDBACK, &mut self.feedback), (p::MIX, &mut self.mix), (p::TIME_MS, &mut self.time_ms), (p::HIGH_CUT, &mut self.high_cut), (p::DUCK, &mut self.duck)] {
            e.settle(v[i].clamp(PARAMS[i].min, PARAMS[i].max));
        }
        for o in [&mut self.low_cut, &mut self.drive, &mut self.modu, &mut self.diffusion, &mut self.pitch, &mut self.shift, &mut self.age, &mut self.sweep, &mut self.width, &mut self.ratio, &mut self.dry_g, &mut self.hold_g, &mut self.pitch_amt, &mut self.shift_amt] {
            o.settle();
        }
        self.apply_structure();
        self.pending = false;
        self.gate.set_immediate(1.0);
        let on = v[p::BYPASS] < 0.5;
        self.was_on = on;
        self.in_g.set_immediate(if on { 1.0 } else { 0.0 });
        self.wet_g.set_immediate(if on || v[p::TRAILS] >= 0.5 { 1.0 } else { 0.0 });
        let t = self.target_samples();
        for l in self.lp_.iter_mut() {
            l.reset(t);
        }
        if self.stereo == 3 {
            let tr = (t * v[p::RATIO] as f64).min(MAX_S * self.sr as f64 - 8.0);
            self.lp_[1].reset(tr);
        }
        self.duck_env.reset();
        self.duck_gr = 1.0;
        self.wow_ph = 0.0;
        self.flutter_ph = 0.0;
        self.sweep_ph = 0.0;
        self.drift = 0.0;
        self.rng.reset();
        self.drift_t = 0.0;
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
