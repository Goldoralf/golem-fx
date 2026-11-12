// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-deesser — golem's de-esser (design: README.md).
//!
//! Stands for golem's `deesser` block (ids `thresh`, `freq`, `range`, `listen` kept). What it fixes in
//! the JSFX it replaces:
//! - **relative detection**: the sibilant band's level minus the full band's level, so the threshold
//!   holds whatever the input level (mic distance, an upstream compressor);
//! - a **capped detection band** (`freq` … 2.5 × `freq`, at most 16 kHz): air and cymbal bleed above do
//!   not trigger it;
//! - a **zero-crossing weight** (Vocal focus): sibilants are noise-like, vowels are not;
//! - a **bump-free reduction**: a dynamic matched shelf or bell (or a wide gain) that is exactly the
//!   input when nothing is reduced.
//!
//! Zero latency unless `lookahead` > 0 (then reported).

#![allow(clippy::needless_range_loop)]

use gfx_dsp::delay::DelayLine;
use gfx_dsp::matched::{design, Analog};
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::util::one_pole_coef;
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
mod plugin;

const MAX_CH: usize = 2;
const CTL: u64 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Shelf,
    Bell,
    Wide,
}

pub static PARAMS: [ParamInfo; 10] = [
    ParamInfo::float("thresh", "Threshold", -50.0, 0.0, -28.0),
    ParamInfo::float("freq", "Frequency", 1500.0, 14000.0, 6000.0),
    ParamInfo::float("range", "Range", 0.0, 24.0, 10.0),
    ParamInfo::boolean("listen", "Listen", false),
    ParamInfo::enumeration("detect", "Detection", 2, 0),
    ParamInfo::enumeration("shape", "Shape", 3, 0),
    ParamInfo::enumeration("focus", "Focus", 2, 0),
    ParamInfo::float("ratio", "Ratio", 1.0, 20.0, 6.0),
    ParamInfo::float("release", "Release", 10.0, 200.0, 60.0),
    ParamInfo::float("lookahead", "Lookahead", 0.0, 10.0, 0.0),
];

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub thresh: f32,
    pub freq: f32,
    pub range: f32,
    pub listen: bool,
    /// relative (true) or absolute
    pub relative: bool,
    pub shape: Shape,
    /// vocal focus (zero-crossing weight) or allround
    pub vocal: bool,
    pub ratio: f32,
    pub release: f32,
    pub lookahead: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            thresh: -28.0,
            freq: 6000.0,
            range: 10.0,
            listen: false,
            relative: true,
            shape: Shape::Shelf,
            vocal: true,
            ratio: 6.0,
            release: 60.0,
            lookahead: 0.0,
        }
    }
}

pub struct Deesser {
    s: Settings,
    sr: f32,
    freq: OnePole,
    det_hp: [Svf; 2],
    det_lp: [Svf; 2],
    dyn_filter: [Svf; MAX_CH],
    filter_on: bool,
    delay: [DelayLine; MAX_CH],
    la: usize,
    e_band: f64,
    e_full: f64,
    att: f64,
    rel_band: f64,
    full_rel: f64,
    red_db: f32,
    red_att: f32,
    red_rel: f32,
    // zero-crossing rate over 5 ms
    zc_hist: Vec<u8>,
    zc_pos: usize,
    zc_count: u32,
    last_sign: bool,
    listen_mix: LinearRamp,
    clock: u64,
    gr_meter: f32,
}

impl Deesser {
    pub fn new() -> Self {
        let d = Settings::default();
        Self {
            s: d,
            sr: 48000.0,
            freq: OnePole::new(20.0, d.freq.ln(), 1e-6),
            det_hp: Default::default(),
            det_lp: Default::default(),
            dyn_filter: Default::default(),
            filter_on: false,
            delay: Default::default(),
            la: 0,
            e_band: 0.0,
            e_full: 0.0,
            att: 0.0,
            rel_band: 0.0,
            full_rel: 0.0,
            red_db: 0.0,
            red_att: 0.0,
            red_rel: 0.0,
            zc_hist: Vec::new(),
            zc_pos: 0,
            zc_count: 0,
            last_sign: false,
            listen_mix: LinearRamp::new(10.0, 0.0),
            clock: 0,
            gr_meter: 0.0,
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn gain_reduction_db(&self) -> f32 {
        self.gr_meter
    }

    pub fn latency_samples(&self) -> usize {
        self.la
    }

    pub fn set(&mut self, s: Settings) {
        self.freq.set_target(s.freq.clamp(1500.0, 14000.0).ln());
        self.listen_mix.set_target(if s.listen { 1.0 } else { 0.0 });
        let la_changed = s.lookahead != self.s.lookahead;
        self.s = s;
        if la_changed {
            self.la = (s.lookahead.clamp(0.0, 10.0) * 0.001 * self.sr) as usize;
        }
        self.control_times();
    }

    fn control_times(&mut self) {
        let sr = self.sr as f64;
        self.att = one_pole_coef(0.0005 * sr);
        self.rel_band = one_pole_coef(self.s.release.max(10.0) as f64 * 0.001 * sr);
        self.full_rel = one_pole_coef(0.3 * sr);
        self.red_att = one_pole_coef(0.0005 * sr) as f32;
        self.red_rel = one_pole_coef(self.s.release.max(10.0) as f64 * 0.001 * sr) as f32;
    }

    fn control(&mut self) {
        let f = self.freq.value().exp();
        // the detection band, 4th-order edges (Butterworth): air above the band stays out of it
        let top = (2.5 * f).min(14000.0).min(0.42 * self.sr);
        for (k, q) in [0.541_196_1f32, 1.306_563].iter().enumerate() {
            self.det_hp[k].set(SvfMode::HighPass, f, *q, 0.0, self.sr);
            self.det_lp[k].set(SvfMode::LowPass, top, *q, 0.0, self.sr);
        }
        // the dynamic reduction filter
        let red = self.red_db as f64;
        if red > 0.0 && self.s.shape != Shape::Wide {
            let proto = match self.s.shape {
                Shape::Shelf => Analog::high_shelf(0.6, -red),
                _ => Analog::bell(1.4, -red),
            };
            let fc = if self.s.shape == Shape::Bell { (f * 1.4) as f64 } else { f as f64 };
            let b = design(proto, fc.min(0.45 * self.sr as f64), self.sr as f64);
            for d in self.dyn_filter.iter_mut() {
                if self.filter_on {
                    d.set_biquad_ramped(&b, CTL as u32);
                } else {
                    d.reset();
                    d.set_biquad(&b);
                }
            }
            self.filter_on = true;
        } else {
            self.filter_on = false;
        }
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(MAX_CH);
        let n = buffers.first().map_or(0, |b| b.len());
        let zc_len = self.zc_hist.len().max(1);
        let mut meter = 0.0f32;
        for i in 0..n {
            if self.clock.is_multiple_of(CTL) {
                for _ in 0..CTL {
                    self.freq.next();
                }
                self.control();
            }
            self.clock += 1;

            // detection on the linked input
            let mut x = 0.0f32;
            for c in 0..nch {
                if buffers[c][i].abs() > x.abs() {
                    x = buffers[c][i];
                }
            }
            let mut band = x;
            for f in self.det_hp.iter_mut().chain(self.det_lp.iter_mut()) {
                band = f.process(band);
            }
            let (b2, f2) = ((band as f64) * (band as f64), (x as f64) * (x as f64));
            let ab = if b2 > self.e_band { self.att } else { self.rel_band };
            self.e_band = b2 + (self.e_band - b2) * ab;
            let af = if f2 > self.e_full { self.att } else { self.full_rel };
            self.e_full = f2 + (self.e_full - f2) * af;
            // zero crossings over 5 ms (of the full signal)
            let sign = x >= 0.0;
            let crossed = (sign != self.last_sign) as u8;
            self.last_sign = sign;
            self.zc_count = self.zc_count + crossed as u32 - self.zc_hist[self.zc_pos] as u32;
            self.zc_hist[self.zc_pos] = crossed;
            self.zc_pos = (self.zc_pos + 1) % zc_len;

            let band_db = 10.0 * self.e_band.max(1e-20).log10() as f32;
            // below −80 dBFS in the band there is nothing to de-ess (and the ratio of two near-silent
            // envelopes means nothing)
            let level = if band_db < -80.0 {
                -200.0
            } else if self.s.relative {
                band_db - 10.0 * self.e_full.max(1e-20).log10() as f32
            } else {
                band_db
            };
            // soft-knee gain computer (6 dB), ratio, range
            let over = level - self.s.thresh;
            let w = 6.0;
            let slope = 1.0 - 1.0 / self.s.ratio.max(1.0);
            let mut target = if over <= -w * 0.5 {
                0.0
            } else if over < w * 0.5 {
                slope * (over + w * 0.5).powi(2) / (2.0 * w)
            } else {
                slope * over
            };
            if self.s.vocal {
                // noise-like content only: crossings per second, 2.5 k → 0, 6 k → 1
                let zcr = self.zc_count as f32 * self.sr / zc_len as f32;
                let t = ((zcr - 2500.0) / 3500.0).clamp(0.0, 1.0);
                target *= t * t * (3.0 - 2.0 * t);
            }
            let target = target.clamp(0.0, self.s.range);
            let k = if target > self.red_db { self.red_att } else { self.red_rel };
            self.red_db = target + (self.red_db - target) * k;
            if self.red_db < 1e-3 && target == 0.0 {
                self.red_db = 0.0;
            }
            meter = meter.max(self.red_db);

            let lm = self.listen_mix.next();
            let wide = if self.s.shape == Shape::Wide && self.red_db > 0.0 { 10f32.powf(-self.red_db * 0.05) } else { 1.0 };
            for c in 0..nch {
                let x = buffers[c][i];
                self.delay[c].push(x);
                let v = if self.la > 0 { self.delay[c].tap(self.la) } else { x };
                let y = if self.filter_on {
                    self.dyn_filter[c].process(v)
                } else if wide != 1.0 {
                    v * wide
                } else {
                    v
                };
                buffers[c][i] = if lm > 0.0 { y + (band - y) * lm } else { y };
            }
        }
        self.gr_meter = meter;
    }
}

impl Default for Deesser {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Deesser {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        self.freq.prepare(sample_rate);
        self.listen_mix.prepare(sample_rate);
        for d in self.delay.iter_mut() {
            d.prepare((0.0101 * sample_rate) as usize + 2);
        }
        self.zc_hist = vec![0; ((0.005 * sample_rate) as usize).max(1)];
        self.la = (self.s.lookahead.clamp(0.0, 10.0) * 0.001 * sample_rate) as usize;
        self.control_times();
    }

    fn reset(&mut self) {
        self.freq.settle();
        self.listen_mix.settle();
        self.det_hp.iter_mut().for_each(Svf::reset);
        self.det_lp.iter_mut().for_each(Svf::reset);
        self.dyn_filter.iter_mut().for_each(Svf::reset);
        self.filter_on = false;
        self.delay.iter_mut().for_each(DelayLine::reset);
        self.e_band = 0.0;
        self.e_full = 0.0;
        self.red_db = 0.0;
        self.zc_hist.iter_mut().for_each(|v| *v = 0);
        self.zc_pos = 0;
        self.zc_count = 0;
        self.last_sign = false;
        self.clock = 0;
        self.gr_meter = 0.0;
        self.control_times();
    }

    fn set_param(&mut self, id: &str, v: f32) {
        let mut s = self.s;
        match id {
            "thresh" => s.thresh = v,
            "freq" => s.freq = v,
            "range" => s.range = v,
            "listen" => s.listen = v >= 0.5,
            "detect" => s.relative = v < 0.5,
            "shape" => {
                s.shape = match v.round() as i32 {
                    1 => Shape::Bell,
                    2 => Shape::Wide,
                    _ => Shape::Shelf,
                }
            }
            "focus" => s.vocal = v < 0.5,
            "ratio" => s.ratio = v,
            "release" => s.release = v,
            "lookahead" => s.lookahead = v,
            _ => return,
        }
        self.set(s);
    }

    fn latency(&self) -> usize {
        self.la
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_block(buffers);
    }
}

#[cfg(test)]
mod tests;
