// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-vibrato — golem's pitch vibrato (design: README.md).
//!
//! Stands for golem's `vibrato` block (ids `depth`, `speed` kept). Depth means the same thing at every
//! speed: in the delay models it is in **cents**, and the mean delay is the least that depth needs.
//!
//! Models:
//! - Clean: Lagrange-5 delay;
//! - BBD: gfx-dsp's clock-domain bucket brigade, 1024 stages (Boss VB-2);
//! - Varistor: Magnatone phase modulation, no delay line, zero latency;
//! - Vibe: gfx-dsp's Uni-Vibe core, wet only;
//! - Scanner: the Hammond line box, crossfading between taps 55 µs apart, up to V1 / V2 / V3.
//!
//! The mean delay of the delay models moves slowly (rising at most 4 ms/s, falling at 1 ms/s). The
//! detune it adds while it moves stays under ~7 cents. It is not reported as latency.

#![allow(clippy::needless_range_loop)]

use gfx_dsp::adaa::{Adaa1, Shaper};
use gfx_dsp::bbd::Bbd;
use gfx_dsp::delay::{DelayLine, Interp};
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::lfo::Rng;
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::transport::{Sync, TransportInfo};
use gfx_dsp::vibe::{Lamp, Vibe};
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
pub mod plugin;

const MAX_CH: usize = 2;
/// Longest mean delay (s).
const MAX_DELAY: f64 = 0.025;
/// Scanner: one ladder section (s), 19 sections.
const SCANNER_STEP: f64 = 55e-6;
const SCANNER_TAPS: usize = 19;
/// Rounded triangle: asin(k sin θ) / asin(k).
const ROUND_K: f64 = 0.97;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    Clean,
    Bbd,
    Varistor,
    Vibe,
    Scanner,
}

impl Model {
    pub const ALL: [Model; 5] = [Model::Clean, Model::Bbd, Model::Varistor, Model::Vibe, Model::Scanner];
}

pub static PARAMS: [ParamInfo; 11] = [
    ParamInfo::float("depth", "Depth", 0.0, 1.0, 0.4).with_exp(),
    ParamInfo::float("speed", "Speed", 0.1, 14.0, 5.0).with_exp(),
    ParamInfo::enumeration("model", "Model", 5, 0),
    ParamInfo::enumeration("shape", "Shape", 3, 0),
    ParamInfo::float("rise", "Rise Time", 0.0, 5000.0, 0.0),
    ParamInfo::float("tone", "Tone", 0.0, 1.0, 1.0),
    ParamInfo::float("mix", "Mix", 0.0, 1.0, 1.0),
    ParamInfo::enumeration("sync", "Sync", 17, 0),
    ParamInfo::float("stereo_phase", "Stereo Phase", 0.0, 180.0, 0.0),
    ParamInfo::float("drive", "Drive", 0.0, 1.0, 0.0),
    ParamInfo::float("level", "Level", -12.0, 12.0, 0.0),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub depth: f32,
    pub speed: f32,
    pub model: Model,
    /// 0 sine, 1 rounded triangle, 2 random
    pub shape: u32,
    pub rise: f32,
    pub tone: f32,
    pub mix: f32,
    pub sync: u32,
    pub stereo_phase: f32,
    pub drive: f32,
    pub level: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { depth: 0.4, speed: 5.0, model: Model::Clean, shape: 0, rise: 0.0, tone: 1.0, mix: 1.0, sync: 0, stereo_phase: 0.0, drive: 0.0, level: 0.0 }
    }
}

/// The delay models run in the pitch domain: the LFO is the pitch deviation, and the delay is its
/// integral. A shape's swing for a unit pitch deviation is 1 / (hz · k).
pub fn peak_slope(shape: u32) -> f64 {
    match shape {
        // sine: 1 / 2π; the rounded triangle's integral is smaller (between 1 / 2π and 1 / 8)
        0 | 1 => std::f64::consts::TAU,
        // random: at worst half a cycle at full deviation
        _ => 2.0,
    }
}

/// Delay swing (s) for a peak of `cents` at `hz` with a shape factor `k`.
pub fn swing_for(cents: f64, hz: f64, k: f64) -> f64 {
    ((cents / 1200.0).exp2() - 1.0) / (hz.max(0.01) * k)
}

#[derive(Clone)]
struct Channel {
    line: DelayLine,
    bbd: Bbd,
    ap: [(f32, f32); 2],
    vibe: Vibe,
    lamp: Lamp,
    scan_lp: Svf,
    tone: Svf,
    adaa: Adaa1,
    rnd_from: f64,
    rnd_to: f64,
    rng: Rng,
    last_phase: f64,
    /// the integrated delay offset (s) of the pitch-domain models
    offset: f64,
}

impl Channel {
    fn new(seed: u64) -> Self {
        Self {
            line: DelayLine::new(),
            bbd: Bbd::new(1024),
            ap: [(0.0, 0.0); 2],
            vibe: Vibe::default(),
            lamp: Lamp::default(),
            scan_lp: Svf::new(),
            tone: Svf::new(),
            adaa: Adaa1::new(),
            rnd_from: 0.0,
            rnd_to: 0.0,
            rng: Rng::new(seed),
            last_phase: 0.0,
            offset: 0.0,
        }
    }

    fn reset_model(&mut self, m: Model) {
        match m {
            Model::Bbd => self.bbd.reset(),
            Model::Varistor => self.ap = [(0.0, 0.0); 2],
            Model::Vibe => {
                self.vibe.reset();
                self.lamp.reset();
            }
            Model::Scanner => self.scan_lp.reset(),
            Model::Clean => {}
        }
    }

    /// Advances the random targets at each wrap of the phase.
    #[inline]
    fn advance(&mut self, p: f64) {
        if p < self.last_phase {
            self.rnd_from = self.rnd_to;
            self.rnd_to = self.rng.bipolar() as f64;
        }
        self.last_phase = p;
    }

    /// Bipolar waveform at phase p.
    #[inline]
    fn wave(&self, p: f64, shape: u32) -> f64 {
        match shape {
            1 => (ROUND_K * (std::f64::consts::TAU * p).sin()).asin() / ROUND_K.asin(),
            2 => self.rnd_from + (self.rnd_to - self.rnd_from) * 0.5 * (1.0 - (std::f64::consts::PI * p).cos()),
            _ => (std::f64::consts::TAU * p).sin(),
        }
    }
}

pub struct Vibrato {
    s: Settings,
    sr: f32,
    ch: [Channel; MAX_CH],
    phase: f64,
    transport: TransportInfo,
    depth: ExpSmoother,
    speed: ExpSmoother,
    tone: OnePole,
    mix: OnePole,
    drive: OnePole,
    sphase: OnePole,
    level: OnePole,
    /// mean delay (s), slew-limited
    centre: f64,
    model_fade: LinearRamp,
    prev_model: Model,
    shape_fade: LinearRamp,
    prev_shape: u32,
    rise_t: f64,
    /// the waveform's peak slope, smoothed through shape changes
    slope: OnePole,
    clock: u64,
}

impl Vibrato {
    pub fn new() -> Self {
        let d = Settings::default();
        Self {
            s: d,
            sr: 48000.0,
            ch: [Channel::new(0x71B), Channel::new(0x71C)],
            phase: 0.0,
            transport: TransportInfo::default(),
            depth: ExpSmoother::new(0.0, 1.0, d.depth),
            speed: ExpSmoother::new(0.1, 14.0, d.speed),
            tone: OnePole::new(20.0, d.tone, 1e-5),
            mix: OnePole::new(20.0, d.mix, 1e-5),
            drive: OnePole::new(20.0, d.drive, 1e-5),
            sphase: OnePole::new(30.0, d.stereo_phase, 1e-4),
            level: OnePole::new(20.0, d.level, 1e-5),
            centre: 0.002,
            model_fade: LinearRamp::new(30.0, 1.0),
            prev_model: d.model,
            shape_fade: LinearRamp::new(30.0, 1.0),
            prev_shape: d.shape,
            rise_t: 0.0,
            slope: OnePole::new(30.0, peak_slope(d.shape) as f32, 1e-6),
            clock: 0,
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn phase(&self) -> f64 {
        self.phase
    }

    /// The current mean delay (s) of the delay models.
    pub fn centre(&self) -> f64 {
        self.centre
    }

    pub fn set(&mut self, s: Settings) {
        if s.depth != self.s.depth {
            self.depth.set_target(s.depth.clamp(0.0, 1.0), self.clock);
        }
        if s.speed != self.s.speed {
            self.speed.set_target(s.speed.clamp(0.1, 14.0), self.clock);
        }
        self.tone.set_target(s.tone);
        self.mix.set_target(s.mix);
        self.drive.set_target(s.drive);
        self.sphase.set_target(s.stereo_phase);
        self.level.set_target(s.level);
        if s.model != self.s.model {
            self.prev_model = self.s.model;
            for c in self.ch.iter_mut() {
                c.reset_model(s.model);
            }
            self.model_fade.set_immediate(0.0);
            self.model_fade.set_target(1.0);
        }
        if s.shape != self.s.shape {
            self.prev_shape = self.s.shape;
            self.shape_fade.set_immediate(0.0);
            self.shape_fade.set_target(1.0);
            self.slope.set_target(peak_slope(s.shape) as f32);
        }
        self.s = s;
    }

    fn rate_hz(&self, speed: f32) -> f64 {
        match Sync::from_index(self.s.sync as usize).beats() {
            Some(b) => self.transport.bpm() / 60.0 / b,
            None => speed as f64,
        }
    }

    /// The mean delay the current speed needs at full depth (s).
    fn centre_target(&self, hz: f64) -> f64 {
        let guard = 4.0 / self.sr as f64 + self.ch[0].bbd.min_delay();
        (swing_for(100.0, hz, peak_slope(self.s.shape).min(self.slope.value() as f64)) + guard).min(MAX_DELAY)
    }

    /// One model, one sample. `w` bipolar LFO, `depth` 0 … 1 after rise.
    #[inline]
    fn model_out(&mut self, m: Model, c: usize, x: f32, w: f64, depth: f64) -> f32 {
        let sr = self.sr as f64;
        let guard = 4.0 / sr;
        let centre = self.centre;
        let ch = &mut self.ch[c];
        match m {
            Model::Clean | Model::Bbd => {
                let d = (centre + ch.offset).clamp(guard, MAX_DELAY * 1.15);
                if m == Model::Clean {
                    ch.line.read(d * sr, Interp::Lagrange5)
                } else {
                    ch.bbd.set_delay(d);
                    ch.bbd.process(x)
                }
            }
            Model::Varistor => {
                // R swept 25:1 on a log curve around 1 kHz, two first-order allpasses; slight AM
                let fc = 1000.0 * 5f64.powf(depth * w);
                let t = (std::f64::consts::PI * fc / sr).tan();
                let a = ((t - 1.0) / (t + 1.0)) as f32;
                let mut v = x;
                for st in ch.ap.iter_mut() {
                    let y = a * v + st.0 - a * st.1;
                    st.0 = v;
                    st.1 = y;
                    v = y;
                }
                v * (1.0 - 0.03 * (depth * w) as f32)
            }
            Model::Vibe => {
                let r = ch.lamp.next((0.5 + 0.5 * w) as f32, depth as f32, 30.0, self.sr);
                ch.vibe.process(x, r, self.sr)
            }
            Model::Scanner => {
                // depth sweeps the reach continuously: V1 ≈ 0.47 (tap 9), V2 ≈ 0.68 (13), V3 = 1 (19)
                let pos = SCANNER_TAPS as f64 * depth * (0.5 + 0.5 * w);
                let i = pos.floor() as usize;
                let f = (pos - i as f64) as f32;
                let tap = |k: usize| ch.line.read((k.min(SCANNER_TAPS) as f64 * SCANNER_STEP + guard) * sr, Interp::Lagrange3);
                let y = tap(i) + f * (tap(i + 1) - tap(i));
                ch.scan_lp.process(y)
            }
        }
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(MAX_CH);
        let n = buffers.first().map_or(0, |b| b.len());
        let sr = self.sr as f64;
        let beats = Sync::from_index(self.s.sync as usize).beats();
        for i in 0..n {
            self.clock += 1;
            let depth0 = self.depth.next() as f64;
            let speed = self.speed.next();
            let tone = self.tone.next();
            let mix = self.mix.next();
            let drive = self.drive.next();
            let sph = self.sphase.next() as f64 / 360.0;
            let lvl = self.level.next();
            let gain = if lvl == 0.0 { 1.0 } else { 10f32.powf(lvl / 20.0) };
            let mf = self.model_fade.next();
            let sf = self.shape_fade.next() as f64;
            let hz = self.rate_hz(speed);
            match beats.and_then(|b| self.transport.phase_at(i, b, 0.0, self.sr)) {
                Some(p) => self.phase = p,
                None => self.phase = (self.phase + hz / sr).fract(),
            }
            // mean delay: rises at 4 ms/s, falls at 1 ms/s
            let target = self.centre_target(hz);
            let step = if target > self.centre { 0.004 / sr } else { 0.001 / sr };
            self.centre = if (target - self.centre).abs() <= step { target } else { self.centre + step * (target - self.centre).signum() };
            // rise time: depth comes in over `rise` (63 % at `rise`)
            self.rise_t += 1.0 / sr;
            let rise = if self.s.rise > 0.0 { 1.0 - (-self.rise_t * 1000.0 / self.s.rise as f64).exp() } else { 1.0 };
            let depth = depth0 * rise;
            self.slope.next();
            let tone_hz = 1500.0 * 13.333f32.powf(tone);
            let kd = 1.0 + 4.0 * drive;
            for c in 0..nch {
                let x = buffers[c][i];
                // the tube colour fades in over the first 10 % of the knob: drive 0 is exactly clean
                let sat = self.ch[c].adaa.process(Shaper::Tanh, x * kd) / kd;
                let xin = x + (sat - x) * (10.0 * drive).min(1.0);
                self.ch[c].line.push(xin);
                let p = if c == 0 { self.phase } else { (self.phase + sph).fract() };
                self.ch[c].advance(p);
                let mut w = self.ch[c].wave(p, self.s.shape);
                if sf < 1.0 {
                    let old = self.ch[c].wave(p, self.prev_shape);
                    w = old + (w - old) * sf;
                }
                // pitch domain: delay' = 1 − ratio; a 10 s leak keeps the integral centred
                let ratio = (100.0 * depth * w / 1200.0).exp2();
                let ch = &mut self.ch[c];
                ch.offset += ((1.0 - ratio) - ch.offset / 10.0) / sr;
                let m = self.s.model;
                let mut wet = self.model_out(m, c, xin, w, depth);
                if mf < 1.0 {
                    let old = self.model_out(self.prev_model, c, xin, w, depth);
                    wet = old + (wet - old) * mf;
                }
                let ch = &mut self.ch[c];
                ch.tone.set(SvfMode::LowPass, tone_hz.min(0.45 * self.sr), std::f32::consts::FRAC_1_SQRT_2, 0.0, self.sr);
                let wet = ch.tone.process(wet);
                buffers[c][i] = (x + (wet - x) * mix) * gain;
            }
        }
    }
}

impl Default for Vibrato {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Vibrato {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        self.depth.prepare(sample_rate);
        self.speed.prepare(sample_rate);
        for o in [&mut self.tone, &mut self.mix, &mut self.drive, &mut self.sphase, &mut self.slope, &mut self.level] {
            o.prepare(sample_rate);
        }
        self.model_fade.prepare(sample_rate);
        self.shape_fade.prepare(sample_rate);
        let len = (MAX_DELAY * 1.2 * sample_rate as f64) as usize + 16;
        for c in self.ch.iter_mut() {
            c.line.prepare(len);
            c.bbd.prepare(sample_rate);
            c.bbd.set_filters(0.45, 9000.0);
            c.scan_lp.set(SvfMode::LowPass, 7000.0f32.min(0.45 * sample_rate), std::f32::consts::FRAC_1_SQRT_2, 0.0, sample_rate);
        }
    }

    fn reset(&mut self) {
        self.depth.settle(self.s.depth);
        self.speed.settle(self.s.speed);
        self.slope.set_target(peak_slope(self.s.shape) as f32);
        for o in [&mut self.tone, &mut self.mix, &mut self.drive, &mut self.sphase, &mut self.slope, &mut self.level] {
            o.settle();
        }
        self.model_fade.set_immediate(1.0);
        self.prev_model = self.s.model;
        self.shape_fade.set_immediate(1.0);
        self.prev_shape = self.s.shape;
        for c in self.ch.iter_mut() {
            c.line.reset();
            for m in Model::ALL {
                c.reset_model(m);
            }
            c.tone.reset();
            c.adaa.reset(Shaper::Tanh);
            c.rng.reset();
            c.rnd_from = 0.0;
            c.rnd_to = c.rng.bipolar() as f64;
            c.last_phase = 0.0;
            c.offset = 0.0;
        }
        self.phase = 0.0;
        let hz = self.rate_hz(self.s.speed);
        self.centre = self.centre_target(hz);
        // a sine starting at phase 0 integrates to A·cos: start the offset at its top to stay centred
        if self.s.shape == 0 && self.s.rise == 0.0 && self.s.sync == 0 {
            let a = swing_for(100.0 * self.s.depth as f64, hz, std::f64::consts::TAU);
            for c in self.ch.iter_mut() {
                c.offset = a;
            }
        }
        self.rise_t = 0.0;
        self.clock = 0;
    }

    fn set_param(&mut self, id: &str, v: f32) {
        let mut s = self.s;
        match id {
            "depth" => s.depth = v,
            "speed" => s.speed = v,
            "model" => s.model = Model::ALL[(v.round().max(0.0) as usize).min(4)],
            "shape" => s.shape = v.round().clamp(0.0, 2.0) as u32,
            "rise" => s.rise = v,
            "tone" => s.tone = v,
            "mix" => s.mix = v,
            "sync" => s.sync = v.round().clamp(0.0, 16.0) as u32,
            "stereo_phase" => s.stereo_phase = v,
            "drive" => s.drive = v,
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
