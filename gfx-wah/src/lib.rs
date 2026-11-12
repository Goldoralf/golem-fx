// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-wah — golem's wah (design: README.md).
//!
//! Stands for golem's `wah` and `wah-color` blocks (ids `wah`, `volume`, `model`, `freq`, `mix`, `mode`).
//! Each pedal is played from its **measured response** — peak frequency, Q, peak gain and the level under
//! the peak at seven pedal positions — through a TPT state-variable filter (resonant low-pass +
//! band-pass), behind the pedal's own DC blocker. The tables are those golem measured on guitarix's
//! circuit models (golem_wah.jsfx, golem_colwah.jsfx, MIT); three voices are added (Classic: Smith's
//! fitted Cry Baby; Clyde: Fractal's settings; Bass 105Q).
//!
//! What it fixes: each pedal keeps **its own pot law** (the JSFX applied one log law to all), the Vox
//! runs heel = low like every other pedal, and the pedal position is smoothed by the golem contract's
//! EXP smoother (no staircase on a resonance).

#![allow(clippy::needless_range_loop)]

use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::util::one_pole_coef;
use gfx_dsp::{Effect, ParamInfo};
pub use gfx_dsp::wah::{lookup, Law, Model, Voice, GCB95, MODELS, W};
use gfx_dsp::wah::{freq_at, logpot};

#[cfg(feature = "plugin")]
mod plugin;

const MAX_CH: usize = 2;

pub static PARAMS: [ParamInfo; 18] = [
    ParamInfo::float("wah", "Wah", 0.0, 1.0, 0.5).with_exp(),
    ParamInfo::float("volume", "Volume", -20.0, 16.0, 0.0),
    ParamInfo::enumeration("model", "Model", 11, GCB95 as u32),
    ParamInfo::float("freq", "Alien Rate", 24.0, 360.0, 24.0),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 100.0),
    ParamInfo::enumeration("mode", "Mode", 3, 0),
    ParamInfo::float("q", "Q", 25.0, 400.0, 100.0),
    ParamInfo::float("range", "Range", 50.0, 200.0, 100.0),
    ParamInfo::enumeration("taper", "Taper", 5, 0),
    ParamInfo::float("heel", "Heel", 0.0, 100.0, 0.0),
    ParamInfo::float("toe", "Toe", 0.0, 100.0, 100.0),
    ParamInfo::float("drive", "Drive", 0.0, 100.0, 0.0),
    ParamInfo::boolean("auto_engage", "Auto Engage", false),
    ParamInfo::float("engage_pos", "Engage Position", 0.0, 20.0, 3.0),
    ParamInfo::float("engage_wait", "Engage Wait", 0.0, 1000.0, 300.0),
    ParamInfo::float("env_sens", "Envelope Sensitivity", 0.0, 100.0, 50.0),
    ParamInfo::float("env_att", "Envelope Attack", 1.0, 100.0, 10.0),
    ParamInfo::float("env_rel", "Envelope Release", 20.0, 2000.0, 100.0),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub wah: f32,
    pub volume: f32,
    pub model: usize,
    pub freq: f32,
    pub mix: f32,
    /// 0 manual, 1 auto, 2 alien
    pub mode: u32,
    pub q: f32,
    pub range: f32,
    /// 0 model, 1 linear, 2 log, 3 reverse log, 4 S
    pub taper: u32,
    pub heel: f32,
    pub toe: f32,
    pub drive: f32,
    pub auto_engage: bool,
    pub engage_pos: f32,
    pub engage_wait: f32,
    pub env_sens: f32,
    pub env_att: f32,
    pub env_rel: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            wah: 0.5,
            volume: 0.0,
            model: GCB95,
            freq: 24.0,
            mix: 100.0,
            mode: 0,
            q: 100.0,
            range: 100.0,
            taper: 0,
            heel: 0.0,
            toe: 100.0,
            drive: 0.0,
            auto_engage: false,
            engage_pos: 3.0,
            engage_wait: 300.0,
            env_sens: 50.0,
            env_att: 10.0,
            env_rel: 100.0,
        }
    }
}

pub struct Wah {
    s: Settings,
    sr: f32,
    pedal: ExpSmoother,
    voices: [Voice; MAX_CH],
    env: f32,
    lfo_phase: f64,
    engaged: bool,
    below: u32,
    engage_fade: LinearRamp,
    model_fade: LinearRamp,
    prev_model: usize,
    prev_voices: [Voice; MAX_CH],
    mix: OnePole,
    volume: OnePole,
    q: OnePole,
    range: OnePole,
    heel: OnePole,
    toe: OnePole,
    drive: OnePole,
    /// the table position W (5 ms)
    pos: OnePole,
    clock: u64,
}

impl Wah {
    pub fn new() -> Self {
        let d = Settings::default();
        Self {
            s: d,
            sr: 48000.0,
            pedal: ExpSmoother::new(0.0, 1.0, d.wah),
            voices: Default::default(),
            env: 0.0,
            lfo_phase: 0.0,
            engaged: true,
            below: 0,
            engage_fade: LinearRamp::new(15.0, 1.0),
            model_fade: LinearRamp::new(20.0, 1.0),
            prev_model: d.model,
            prev_voices: Default::default(),
            mix: OnePole::new(20.0, d.mix, 1e-5),
            volume: OnePole::new(20.0, d.volume, 1e-5),
            q: OnePole::new(20.0, d.q, 1e-4),
            range: OnePole::new(20.0, d.range, 1e-4),
            heel: OnePole::new(20.0, d.heel, 1e-4),
            toe: OnePole::new(20.0, d.toe, 1e-4),
            drive: OnePole::new(20.0, d.drive, 1e-5),
            pos: OnePole::new(5.0, 0.5, 1e-7),
            clock: 0,
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn set(&mut self, s: Settings) {
        if s.wah != self.s.wah {
            self.pedal.set_target(s.wah.clamp(0.0, 1.0), self.clock);
        }
        if s.model != self.s.model {
            self.prev_model = self.s.model;
            self.prev_voices = self.voices.clone();
            self.model_fade.set_immediate(0.0);
            self.model_fade.set_target(1.0);
        }
        self.mix.set_target(s.mix);
        self.volume.set_target(s.volume);
        self.q.set_target(s.q);
        self.range.set_target(s.range);
        self.heel.set_target(s.heel);
        self.toe.set_target(s.toe);
        self.drive.set_target(s.drive);
        self.s = s;
    }

    /// The table position: guitarix's own auto law in auto mode with the model's taper (the linear
    /// envelope, bounded and inverted per pedal, as golem's wah-color presets played it), else the
    /// position `u` through the taper.
    fn target_w(&self, model: usize, u: f32) -> f32 {
        if self.s.mode == 1 && self.s.taper == 0 {
            let (lo, hi, inv) = MODELS[model].auto;
            let g = 4f32.powf((self.s.env_sens - 50.0) / 50.0);
            let w = (self.env * g).clamp(lo, hi);
            if inv {
                1.0 - w
            } else {
                w
            }
        } else {
            self.position_to_w(model, u)
        }
    }

    /// Toe position (0 heel … 1 toe) → the model's W, through the taper.
    fn position_to_w(&self, model: usize, u: f32) -> f32 {
        let m = &MODELS[model];
        if self.s.taper == 0 {
            return m.law.apply(u);
        }
        let t = match self.s.taper {
            1 => u,
            2 => logpot(4.0, u),
            3 => 1.0 - logpot(4.0, 1.0 - u),
            _ => u * u * (3.0 - 2.0 * u),
        };
        // the model's direction: W towards the heel is where its table is lowest
        let heel_low = freq_at(m, 6) < freq_at(m, 0);
        if heel_low {
            1.0 - t
        } else {
            t
        }
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(MAX_CH);
        let n = buffers.first().map_or(0, |b| b.len());
        let sr = self.sr;
        let att = one_pole_coef(self.s.env_att.max(1.0) as f64 * 0.001 * sr as f64) as f32;
        let rel = one_pole_coef(self.s.env_rel.max(20.0) as f64 * 0.001 * sr as f64) as f32;
        let wait = (self.s.engage_wait * 0.001 * sr) as u32;
        for i in 0..n {
            self.clock += 1;
            let raw = self.pedal.next();
            // auto-engage on the pedal position, with hysteresis
            if self.s.auto_engage {
                let pos = self.s.engage_pos * 0.01;
                if raw <= pos {
                    self.below = self.below.saturating_add(1);
                    if self.below >= wait && self.engaged {
                        self.engaged = false;
                        self.engage_fade.set_target(0.0);
                    }
                } else if raw > pos + 0.02 {
                    self.below = 0;
                    if !self.engaged {
                        self.engaged = true;
                        self.engage_fade.set_target(1.0);
                    }
                }
            } else if !self.engaged {
                self.engaged = true;
                self.engage_fade.set_target(1.0);
            }
            let ef = self.engage_fade.next();

            // the position by mode
            let mut level = 0.0f32;
            for c in 0..nch {
                level = level.max(buffers[c][i].abs());
            }
            let k = if level > self.env { att } else { rel };
            self.env = level + (self.env - level) * k;
            let (heel, toe) = (self.heel.next(), self.toe.next());
            let u_pedal = heel * 0.01 + (toe - heel) * 0.01 * raw;
            let u = match self.s.mode {
                1 => {
                    // dB window: higher sensitivity reacts to softer playing
                    let db = 20.0 * self.env.max(1e-6).log10();
                    let floor = -20.0 - 40.0 * self.s.env_sens * 0.01;
                    ((db - floor) / 30.0).clamp(0.0, 1.0)
                }
                2 => {
                    self.lfo_phase = (self.lfo_phase + self.s.freq as f64 / 60.0 / sr as f64).fract();
                    (0.5 + 0.5 * (std::f64::consts::TAU * self.lfo_phase).sin()) as f32
                }
                _ => u_pedal,
            };
            let drive = self.drive.next() * 0.01;
            let q_s = self.q.next() * 0.01;
            let r_s = self.range.next() * 0.01;
            // the table position glides (5 ms): mode and taper changes sweep instead of jumping
            let w_target = self.target_w(self.s.model, u);
            self.pos.set_target(w_target);
            let w = self.pos.next();
            let m = &MODELS[self.s.model];
            let dc = (-std::f32::consts::TAU * m.dc / sr).exp();
            let fade = self.model_fade.next();
            let (pm, pw, pdc) = if fade < 1.0 {
                let pmodel = &MODELS[self.prev_model];
                (pmodel, self.target_w(self.prev_model, u), (-std::f32::consts::TAU * pmodel.dc / sr).exp())
            } else {
                (m, w, dc)
            };
            let mix = self.mix.next() * 0.01;
            let vol = {
                let v = self.volume.next();
                if v == 0.0 {
                    1.0
                } else {
                    10f32.powf(v / 20.0)
                }
            };
            for c in 0..nch {
                let x = buffers[c][i];
                self.voices[c].set(m, w, q_s, r_s, sr);
                let mut wet = self.voices[c].process(x, dc, drive);
                if fade < 1.0 {
                    self.prev_voices[c].set(pm, pw, q_s, r_s, sr);
                    let old = self.prev_voices[c].process(x, pdc, drive);
                    wet = old + (wet - old) * fade;
                }
                let y = (x + (wet - x) * mix) * vol;
                buffers[c][i] = if ef >= 1.0 { y } else { x + (y - x) * ef };
            }
        }
    }
}

impl Default for Wah {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Wah {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        self.pedal.prepare(sample_rate);
        self.engage_fade.prepare(sample_rate);
        self.model_fade.prepare(sample_rate);
        self.mix.prepare(sample_rate);
        self.volume.prepare(sample_rate);
        for o in [&mut self.q, &mut self.range, &mut self.heel, &mut self.toe, &mut self.drive, &mut self.pos] {
            o.prepare(sample_rate);
        }
    }

    fn reset(&mut self) {
        self.pedal.reset();
        self.voices.iter_mut().for_each(Voice::reset);
        self.prev_voices.iter_mut().for_each(Voice::reset);
        self.env = 0.0;
        self.lfo_phase = 0.0;
        self.engaged = true;
        self.below = 0;
        self.engage_fade.set_immediate(1.0);
        self.model_fade.set_immediate(1.0);
        self.prev_model = self.s.model;
        self.mix.settle();
        self.volume.settle();
        for o in [&mut self.q, &mut self.range, &mut self.heel, &mut self.toe, &mut self.drive] {
            o.settle();
        }
        let u0 = self.s.heel * 0.01 + (self.s.toe - self.s.heel) * 0.01 * self.s.wah;
        let w0 = self.position_to_w(self.s.model, u0);
        self.pos.set_immediate(w0);
        self.clock = 0;
    }

    fn set_param(&mut self, id: &str, v: f32) {
        let mut s = self.s;
        match id {
            "wah" => s.wah = v,
            "volume" => s.volume = v,
            "model" => s.model = (v.round().max(0.0) as usize).min(MODELS.len() - 1),
            "freq" => s.freq = v,
            "mix" => s.mix = v,
            "mode" => s.mode = v.round().clamp(0.0, 2.0) as u32,
            "q" => s.q = v,
            "range" => s.range = v,
            "taper" => s.taper = v.round().clamp(0.0, 4.0) as u32,
            "heel" => s.heel = v,
            "toe" => s.toe = v,
            "drive" => s.drive = v,
            "auto_engage" => s.auto_engage = v >= 0.5,
            "engage_pos" => s.engage_pos = v,
            "engage_wait" => s.engage_wait = v,
            "env_sens" => s.env_sens = v,
            "env_att" => s.env_att = v,
            "env_rel" => s.env_rel = v,
            _ => return,
        }
        self.set(s);
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_block(buffers);
    }
}

#[cfg(test)]
mod tests;
