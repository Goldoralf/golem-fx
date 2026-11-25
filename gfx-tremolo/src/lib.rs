// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-tremolo — golem's tremolo (design: README.md).
//!
//! Stands for golem's `tremolo` block (ids `rate`, `depth`, `wave` kept). The amp tremolos as what they
//! are — an LFO driving a circuit (Strymon's tremolo white paper):
//! - **Clean**: linear amplitude modulation, Calf Pulsator's law (today's presets play the same);
//! - **Bias**: a push-pull pair whose bias the LFO moves — crossover distortion near the trough,
//!   compression at the top, level-dependent;
//! - **Optical**: incandescent lamp (thermal lag) → LDR (fast on, slow off) → attenuation;
//! - **Neon**: a neon lamp that strikes and goes out (hysteresis) → the same LDR: choppy;
//! - **Harmonic**: low and high bands (gapped) modulated in antiphase.
//!
//! Square waves have fixed 2 ms raised-cosine edges. The LFO runs from phase 0 at reset, a parameter
//! change never moves its phase, and when synced it follows the host's position. Zero latency.

#![allow(clippy::needless_range_loop)]

use gfx_dsp::envelope::PeakFollower;
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::transport::{Sync, TransportInfo};
use gfx_dsp::util::one_pole_coef;
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
pub mod plugin;

const MAX_CH: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    Clean,
    Bias,
    Optical,
    Neon,
    Harmonic,
}

impl Model {
    pub const ALL: [Model; 5] = [Model::Clean, Model::Bias, Model::Optical, Model::Neon, Model::Harmonic];
}

pub static PARAMS: [ParamInfo; 12] = [
    ParamInfo::float("rate", "Rate", 0.5, 15.0, 5.0).with_exp(),
    ParamInfo::float("depth", "Depth", 0.0, 1.0, 0.6).with_exp(),
    ParamInfo::enumeration("wave", "Wave", 3, 0),
    ParamInfo::enumeration("model", "Model", 5, 0),
    ParamInfo::enumeration("sync", "Sync", 17, 0),
    ParamInfo::float("duty", "Shape", 0.0, 1.0, 0.5),
    ParamInfo::float("xover", "Crossover", 100.0, 2000.0, 400.0),
    ParamInfo::float("blend", "Harmonic Mix", 0.0, 1.0, 1.0),
    ParamInfo::float("drive", "Tube Drive", 0.0, 1.0, 0.3),
    ParamInfo::float("duck", "Dynamics", 0.0, 1.0, 0.0),
    ParamInfo::float("stereo_phase", "Stereo Phase", 0.0, 180.0, 0.0),
    ParamInfo::float("level", "Level", 0.0, 12.0, 0.0),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub rate: f32,
    pub depth: f32,
    /// 0 sine, 1 triangle, 2 square
    pub wave: u32,
    pub model: Model,
    pub sync: u32,
    pub duty: f32,
    pub xover: f32,
    pub blend: f32,
    pub drive: f32,
    pub duck: f32,
    pub stereo_phase: f32,
    pub level: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            rate: 5.0,
            depth: 0.6,
            wave: 0,
            model: Model::Clean,
            sync: 0,
            duty: 0.5,
            xover: 400.0,
            blend: 1.0,
            drive: 0.3,
            duck: 0.0,
            stereo_phase: 0.0,
            level: 0.0,
        }
    }
}

/// The LFO's value (0 = trough, 1 = top) at phase `p` for a wave, a duty and an edge width.
pub fn lfo_value(p: f64, wave: u32, duty: f32, edge: f64) -> f32 {
    // duty leans the waveform: the first half of the cycle lasts `duty`
    let d = (duty as f64).clamp(0.05, 0.95);
    let q = if p < d { 0.5 * p / d } else { 0.5 + 0.5 * (p - d) / (1.0 - d) };
    let v = match wave {
        1 => 1.0 - 4.0 * ((q + 0.25).rem_euclid(1.0) - 0.5).abs(),
        2 => {
            // square, high on the first half, raised-cosine edges of `edge` (fraction of a cycle)
            let e = edge.clamp(1e-4, 0.25);
            let ramp = |x: f64| 0.5 - 0.5 * (std::f64::consts::PI * (x / e).clamp(0.0, 1.0)).cos();
            if q < 0.5 {
                if q < e { 2.0 * ramp(q) - 1.0 } else { 1.0 }
            } else if q - 0.5 < e {
                1.0 - 2.0 * ramp(q - 0.5)
            } else {
                -1.0
            }
        }
        _ => (std::f64::consts::TAU * q).sin(),
    };
    (0.5 + 0.5 * v) as f32
}

#[inline]
fn push_pull_f(u: f64) -> f64 {
    if u > 0.0 {
        u * u.sqrt()
    } else {
        0.0
    }
}

#[inline]
fn push_pull_af(u: f64) -> f64 {
    if u > 0.0 {
        0.4 * u * u * u.sqrt()
    } else {
        0.0
    }
}

/// ADAA1 on an arbitrary argument sequence of T(u) = max(0, u)^1.5.
#[derive(Clone, Copy, Default)]
struct PpAdaa {
    a1: f64,
    f1: f64,
}

impl PpAdaa {
    #[inline]
    fn next(&mut self, a: f64) -> f64 {
        let fa = push_pull_af(a);
        let d = a - self.a1;
        let y = if d.abs() < 1e-7 { push_pull_f(0.5 * (a + self.a1)) } else { (fa - self.f1) / d };
        self.a1 = a;
        self.f1 = fa;
        y
    }
}

#[derive(Clone, Default)]
struct Channel {
    pos_pp: PpAdaa,
    neg_pp: PpAdaa,
    lp: Svf,
    hp: Svf,
    /// lamp and LDR state per light (0 optical, 1 neon)
    lamp: [f32; 2],
    ldr: [f32; 2],
    neon_on: bool,
}

pub struct Tremolo {
    s: Settings,
    sr: f32,
    ch: [Channel; MAX_CH],
    phase: f64,
    transport: TransportInfo,
    rate: OnePole,
    depth: OnePole,
    duty: OnePole,
    blend: OnePole,
    drive: OnePole,
    level: OnePole,
    sphase: OnePole,
    duck: OnePole,
    /// Wave changes crossfade the two LFO values over 20 ms.
    wave_fade: LinearRamp,
    prev_wave: u32,
    model_fade: LinearRamp,
    prev_model: Model,
    env: PeakFollower,
    slow_db: f32,
    clock: u64,
}

impl Tremolo {
    pub fn new() -> Self {
        let d = Settings::default();
        Self {
            s: d,
            sr: 48000.0,
            ch: Default::default(),
            phase: 0.0,
            transport: TransportInfo::default(),
            rate: OnePole::new(30.0, d.rate, 1e-6),
            depth: OnePole::new(20.0, d.depth, 1e-6),
            duty: OnePole::new(30.0, d.duty, 1e-6),
            blend: OnePole::new(20.0, d.blend, 1e-6),
            drive: OnePole::new(20.0, d.drive, 1e-6),
            level: OnePole::new(20.0, d.level, 1e-6),
            sphase: OnePole::new(30.0, d.stereo_phase, 1e-6),
            duck: OnePole::new(30.0, d.duck, 1e-6),
            wave_fade: LinearRamp::new(20.0, 1.0),
            prev_wave: d.wave,
            model_fade: LinearRamp::new(20.0, 1.0),
            prev_model: d.model,
            env: PeakFollower::new(5.0, 150.0),
            slow_db: -40.0,
            clock: 0,
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn phase(&self) -> f64 {
        self.phase
    }

    pub fn set(&mut self, s: Settings) {
        self.rate.set_target(s.rate.clamp(0.5, 15.0));
        self.depth.set_target(s.depth.clamp(0.0, 1.0));
        self.duty.set_target(s.duty);
        self.blend.set_target(s.blend);
        self.drive.set_target(s.drive);
        self.level.set_target(s.level);
        self.sphase.set_target(s.stereo_phase);
        self.duck.set_target(s.duck);
        if s.wave != self.s.wave {
            self.prev_wave = self.s.wave;
            self.wave_fade.set_immediate(0.0);
            self.wave_fade.set_target(1.0);
        }
        if s.model != self.s.model {
            self.prev_model = self.s.model;
            self.model_fade.set_immediate(0.0);
            self.model_fade.set_target(1.0);
        }
        if s.xover != self.s.xover {
            self.set_bands(s.xover);
        }
        self.s = s;
    }

    fn set_bands(&mut self, xover: f32) {
        let sr = self.sr;
        for c in self.ch.iter_mut() {
            c.lp.set(SvfMode::LowPass, 0.5 * xover, 0.707, 0.0, sr);
            c.hp.set(SvfMode::HighPass, 1.6 * xover, 0.707, 0.0, sr);
        }
    }

    /// One model's output for input x at LFO value u (1 = loud) and depth.
    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn model_out(&mut self, model: Model, c: usize, x: f32, u: f32, depth: f32, drive: f32, blend: f32, att_ldr: f32, rel_ldr: f32, lamp_k: f32) -> f32 {
        let ch = &mut self.ch[c];
        match model {
            Model::Clean => x * (1.0 - depth * (1.0 - u)),
            Model::Bias | Model::Harmonic => {
                // push-pull: the LFO moves the bias; small-signal gain ∝ √b
                let gain_target = 1.0 - depth * (1.0 - u);
                let b = (gain_target.max(0.0) as f64).powi(2);
                let k = 0.3 + 2.0 * drive as f64;
                let xi = x as f64 * k;
                let y = ch.pos_pp.next(b + xi) - ch.neg_pp.next(b - xi);
                let bias = (y / (3.0 * k)) as f32;
                if model == Model::Harmonic {
                    let lo = ch.lp.process(x);
                    let hi = ch.hp.process(x);
                    // antiphase bands, each through a mild curve
                    let gl = 1.0 - depth * (1.0 - u);
                    let gh = 1.0 - depth * u;
                    let sat = |v: f32| (v * (1.0 + drive)).tanh() / (1.0 + drive);
                    let harmonic = sat(lo * gl) + sat(hi * gh);
                    bias + (harmonic - bias) * blend
                } else {
                    bias
                }
            }
            Model::Optical | Model::Neon => {
                let light = if model == Model::Neon {
                    // the neon strikes above one level and goes out below a lower one: shorter dips
                    // with less depth, never shallower ones
                    let on_at = 1.0 - 0.6 * depth;
                    let off_at = on_at - 0.15;
                    let lfo_down = 1.0 - u;
                    if !ch.neon_on && lfo_down > on_at && depth > 0.0 {
                        ch.neon_on = true;
                    } else if ch.neon_on && lfo_down < off_at {
                        ch.neon_on = false;
                    }
                    if ch.neon_on { 1.0 } else { 0.0 }
                } else {
                    depth * (1.0 - u)
                };
                let j = (model == Model::Neon) as usize;
                // incandescent lamp: thermal lag
                ch.lamp[j] += (light - ch.lamp[j]) * lamp_k;
                // LDR: fast when the light rises, slow when it falls
                let k = if ch.lamp[j] > ch.ldr[j] { att_ldr } else { rel_ldr };
                ch.ldr[j] = ch.lamp[j] + (ch.ldr[j] - ch.lamp[j]) * k;
                let dip_db = if model == Model::Neon { 24.0 * depth.max(0.25) } else { 30.0 };
                x * 10f32.powf(-dip_db * ch.ldr[j] / 20.0)
            }
        }
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(MAX_CH);
        let n = buffers.first().map_or(0, |b| b.len());
        let sr = self.sr as f64;
        let att_ldr = one_pole_coef(0.005 * sr) as f32;
        let rel_ldr = one_pole_coef(0.06 * sr) as f32;
        let lamp_k = 1.0 - one_pole_coef(0.03 * sr) as f32;
        let slow = one_pole_coef(1.5 * sr) as f32;
        let beats = Sync::from_index(self.s.sync as usize).beats();
        for i in 0..n {
            self.clock += 1;
            let rate = self.rate.next();
            let depth0 = self.depth.next();
            let duty = self.duty.next();
            let blend = self.blend.next();
            let drive = self.drive.next();
            let lvl = self.level.next();
            let sph = self.sphase.next() as f64 / 360.0;
            let fade = self.model_fade.next();
            let wfade = self.wave_fade.next();
            let duck = self.duck.next();
            // phase: from the host's position when synced and playing, else free-running
            match beats.and_then(|b| self.transport.phase_at(i, b, 0.0, self.sr)) {
                Some(p) => self.phase = p,
                None => {
                    let hz = match beats {
                        Some(b) => self.transport.bpm() / 60.0 / b,
                        None => rate as f64,
                    };
                    self.phase = (self.phase + hz / sr).fract();
                }
            }
            let hz_now = match beats {
                Some(b) => self.transport.bpm() / 60.0 / b,
                None => rate as f64,
            };
            let edge = 0.002 * hz_now;
            // dynamics: playing harder (relative to the playing's own level) reduces the depth
            let mut level = 0.0f32;
            for c in 0..nch {
                level = level.max(buffers[c][i].abs());
            }
            let e = self.env.next(level);
            let edb = 20.0 * e.max(1e-6).log10();
            if edb > -70.0 {
                self.slow_db = edb + (self.slow_db - edb) * slow;
            }
            let rel_level = ((edb - self.slow_db + 6.0) / 12.0).clamp(0.0, 1.0);
            let depth = depth0 * (1.0 - duck * rel_level);
            let gain_out = if lvl == 0.0 { 1.0 } else { 10f32.powf(lvl / 20.0) };
            for c in 0..nch {
                let p = if c == 0 { self.phase } else { (self.phase + sph).fract() };
                let mut u = lfo_value(p, self.s.wave, duty, edge);
                if wfade < 1.0 {
                    let old = lfo_value(p, self.prev_wave, duty, edge);
                    u = old + (u - old) * wfade;
                }
                let x = buffers[c][i];
                let model = self.s.model;
                let pm = self.prev_model;
                let pair = matches!((model, pm), (Model::Bias, Model::Harmonic) | (Model::Harmonic, Model::Bias));
                if fade < 1.0 && pair {
                    let b = if model == Model::Harmonic { blend * fade } else { blend * (1.0 - fade) };
                    buffers[c][i] = self.model_out(Model::Harmonic, c, x, u, depth, drive, b, att_ldr, rel_ldr, lamp_k) * gain_out;
                    continue;
                }
                let mut y = self.model_out(model, c, x, u, depth, drive, blend, att_ldr, rel_ldr, lamp_k);
                if fade < 1.0 {
                    let old = self.model_out(pm, c, x, u, depth, drive, blend, att_ldr, rel_ldr, lamp_k);
                    y = old + (y - old) * fade;
                }
                buffers[c][i] = y * gain_out;
            }
        }
    }
}

impl Default for Tremolo {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Tremolo {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        for o in [&mut self.rate, &mut self.depth, &mut self.duty, &mut self.blend, &mut self.drive, &mut self.level, &mut self.sphase, &mut self.duck] {
            o.prepare(sample_rate);
        }
        self.model_fade.prepare(sample_rate);
        self.wave_fade.prepare(sample_rate);
        self.env.prepare(sample_rate);
        self.set_bands(self.s.xover);
    }

    fn reset(&mut self) {
        for o in [&mut self.rate, &mut self.depth, &mut self.duty, &mut self.blend, &mut self.drive, &mut self.level, &mut self.sphase, &mut self.duck] {
            o.settle();
        }
        self.model_fade.set_immediate(1.0);
        self.wave_fade.set_immediate(1.0);
        self.prev_wave = self.s.wave;
        self.prev_model = self.s.model;
        for c in self.ch.iter_mut() {
            c.pos_pp = PpAdaa::default();
            c.neg_pp = PpAdaa::default();
            c.lp.reset();
            c.hp.reset();
            c.lamp = [0.0; 2];
            c.ldr = [0.0; 2];
            c.neon_on = false;
        }
        self.phase = 0.0;
        self.env.reset();
        self.slow_db = -40.0;
        self.clock = 0;
    }

    fn set_param(&mut self, id: &str, v: f32) {
        let mut s = self.s;
        match id {
            "rate" => s.rate = v,
            "depth" => s.depth = v,
            "wave" => s.wave = v.round().clamp(0.0, 2.0) as u32,
            "model" => s.model = Model::ALL[(v.round().max(0.0) as usize).min(4)],
            "sync" => s.sync = v.round().clamp(0.0, 16.0) as u32,
            "duty" => s.duty = v,
            "xover" => s.xover = v,
            "blend" => s.blend = v,
            "drive" => s.drive = v,
            "duck" => s.duck = v,
            "stereo_phase" => s.stereo_phase = v,
            "level" => s.level = v,
            _ => return,
        }
        self.set(s);
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
