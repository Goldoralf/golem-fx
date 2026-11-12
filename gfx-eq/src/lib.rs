// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-eq — golem's EQ (design: README.md).
//!
//! Stands for golem's `eq` block (ids `lowcut`, `midgain`, `midfreq`, `highs` kept). Every band is a
//! **matched** second-order section (gfx-dsp `matched`: analog-like up to Nyquist, no cramping at
//! 48 kHz) realised in the TPT state-variable filter, its coefficients recomputed on a fixed 32-sample
//! grid and ramped in between: click-free under the expression pedal.
//!
//! Bands, in order: low cut (12 / 24 dB / oct) · low shelf · bell (proportional Q) · bell 2 ·
//! de-harsh (dynamic cut-only bell, detection relative to the full band) · high shelf · air (boost-only
//! broad shelf) · tilt (shelf pair around 1 kHz) · high cut (12 / 24) · output, static auto gain.
//! A band at neutral is bypassed: the EQ is bit-transparent when flat.

#![allow(clippy::needless_range_loop)]

use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::matched::{design, Analog, Biquad};
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::util::one_pole_coef;
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
mod plugin;

const MAX_CH: usize = 2;
const CTL: u64 = 32;
const NP: usize = 23;

/// Parameter indices.
pub mod p {
    pub const LOWCUT: usize = 0;
    pub const MIDGAIN: usize = 1;
    pub const MIDFREQ: usize = 2;
    pub const HIGHS: usize = 3;
    pub const LOWCUT_SLOPE: usize = 4;
    pub const LOWS: usize = 5;
    pub const LOWFREQ: usize = 6;
    pub const MIDQ: usize = 7;
    pub const MIDPROP: usize = 8;
    pub const MID2GAIN: usize = 9;
    pub const MID2FREQ: usize = 10;
    pub const MID2Q: usize = 11;
    pub const HIGHFREQ: usize = 12;
    pub const AIR: usize = 13;
    pub const AIRFREQ: usize = 14;
    pub const TILT: usize = 15;
    pub const HIGHCUT: usize = 16;
    pub const HIGHCUT_SLOPE: usize = 17;
    pub const HARSH_FREQ: usize = 18;
    pub const HARSH_THRESH: usize = 19;
    pub const HARSH_RANGE: usize = 20;
    pub const OUTPUT: usize = 21;
    pub const AUTO_GAIN: usize = 22;
}

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::float("lowcut", "Low Cut", 10.0, 500.0, 80.0),
    ParamInfo::float("midgain", "Mid Gain", -18.0, 18.0, 0.0).with_exp(),
    ParamInfo::float("midfreq", "Mid Freq", 100.0, 8000.0, 800.0).with_exp(),
    ParamInfo::float("highs", "Highs", -18.0, 18.0, 0.0).with_exp(),
    ParamInfo::enumeration("lowcut_slope", "Low Cut Slope", 2, 0),
    ParamInfo::float("lows", "Lows", -18.0, 18.0, 0.0),
    ParamInfo::float("lowfreq", "Low Freq", 40.0, 500.0, 120.0),
    // ReaEQ's band 3 under golem: 2 octaves = Q 0.67 (read by the rack session)
    ParamInfo::float("midq", "Mid Q", 0.3, 6.0, 0.67),
    ParamInfo::float("midprop", "Proportional Q", 0.0, 100.0, 50.0),
    ParamInfo::float("mid2gain", "Mid 2 Gain", -18.0, 18.0, 0.0).with_exp(),
    ParamInfo::float("mid2freq", "Mid 2 Freq", 100.0, 8000.0, 2500.0).with_exp(),
    ParamInfo::float("mid2q", "Mid 2 Q", 0.3, 6.0, 0.7),
    ParamInfo::float("highfreq", "High Freq", 1000.0, 16000.0, 5000.0),
    ParamInfo::float("air", "Air", 0.0, 10.0, 0.0),
    ParamInfo::float("airfreq", "Air Freq", 5000.0, 20000.0, 12000.0),
    ParamInfo::float("tilt", "Tilt", -6.0, 6.0, 0.0).with_exp(),
    ParamInfo::float("highcut", "High Cut", 1000.0, 22000.0, 22000.0).with_exp(),
    ParamInfo::enumeration("highcut_slope", "High Cut Slope", 2, 0),
    ParamInfo::float("harsh_freq", "De-harsh Freq", 1500.0, 8000.0, 3500.0),
    ParamInfo::float("harsh_thresh", "De-harsh Threshold", -40.0, 0.0, 0.0),
    ParamInfo::float("harsh_range", "De-harsh Range", 0.0, 12.0, 6.0),
    ParamInfo::float("output", "Output", -18.0, 18.0, 0.0).with_exp(),
    ParamInfo::boolean("auto_gain", "Auto Gain", false),
];

/// Smoothed in ln(value) (frequencies and Qs).
const LOG_PARAMS: [usize; 10] = [p::LOWCUT, p::MIDFREQ, p::LOWFREQ, p::MIDQ, p::MID2FREQ, p::MID2Q, p::HIGHFREQ, p::AIRFREQ, p::HIGHCUT, p::HARSH_FREQ];
/// Switches (not smoothed).
const SWITCHES: [usize; 3] = [p::LOWCUT_SLOPE, p::HIGHCUT_SLOPE, p::AUTO_GAIN];

// sections, in processing order
const LC1: usize = 0;
const LC2: usize = 1;
const LSH: usize = 2;
const BELL1: usize = 3;
const BELL2: usize = 4;
const HARSH: usize = 5;
const HSH: usize = 6;
const AIR: usize = 7;
const TLO: usize = 8;
const THI: usize = 9;
const HC1: usize = 10;
const HC2: usize = 11;
const NS: usize = 12;

const BW2: [f64; 2] = [0.541_196_1, 1.306_563];

pub struct Eq {
    sr: f32,
    values: [f32; NP],
    smooth: Vec<ExpSmoother>,
    log: [bool; NP],
    sections: [[Svf; NS]; MAX_CH],
    active: [bool; NS],
    designs: [Option<Biquad>; NS],
    /// A band switching in or out fades over 5 ms (its filter starts from rest under the fade);
    /// fully faded out, it is bypassed again (bit-transparent).
    fade: Vec<LinearRamp>,
    // de-harsh detector
    harsh_bp: [Svf; MAX_CH],
    e_band: f64,
    e_full: f64,
    cut_db: f32,
    det_att: f64,
    det_rel: f64,
    cut_att: f32,
    cut_rel: f32,
    // output and static auto gain
    auto_db: OnePole,
    ag_tick: u32,
    clock: u64,
    gr_meter: f32,
}

#[inline]
fn db_gain(db: f32) -> f32 {
    if db == 0.0 {
        1.0
    } else {
        10f32.powf(db * 0.05)
    }
}

impl Eq {
    pub fn new() -> Self {
        let mut log = [false; NP];
        LOG_PARAMS.iter().for_each(|&i| log[i] = true);
        let values: [f32; NP] = std::array::from_fn(|i| PARAMS[i].default);
        let smooth = (0..NP)
            .map(|i| {
                let pi = PARAMS[i];
                if log[i] {
                    ExpSmoother::new(pi.min.ln(), pi.max.ln(), pi.default.ln())
                } else {
                    ExpSmoother::new(pi.min, pi.max, pi.default)
                }
            })
            .collect();
        Self {
            sr: 48000.0,
            values,
            smooth,
            log,
            sections: Default::default(),
            active: [false; NS],
            designs: [None; NS],
            fade: (0..NS).map(|_| LinearRamp::new(5.0, 0.0)).collect(),
            harsh_bp: Default::default(),
            e_band: 0.0,
            e_full: 0.0,
            cut_db: 0.0,
            det_att: 0.0,
            det_rel: 0.0,
            cut_att: 0.0,
            cut_rel: 0.0,
            auto_db: OnePole::new(50.0, 0.0, 1e-6),
            ag_tick: 0,
            clock: 0,
            gr_meter: 0.0,
        }
    }

    pub fn harsh_reduction_db(&self) -> f32 {
        self.gr_meter
    }

    pub fn value(&self, i: usize) -> f32 {
        self.values[i]
    }

    /// Sets parameter `i` to a plain value.
    pub fn set_index(&mut self, i: usize, v: f32) {
        let pi = PARAMS[i];
        let v = v.clamp(pi.min, pi.max);
        if v == self.values[i] {
            return;
        }
        self.values[i] = v;
        if !SWITCHES.contains(&i) {
            let t = if self.log[i] { v.ln() } else { v };
            self.smooth[i].set_target(t, self.clock);
        }
    }

    #[inline]
    fn cur(&self, i: usize) -> f32 {
        if SWITCHES.contains(&i) {
            return self.values[i];
        }
        let v = self.smooth[i].value();
        if self.log[i] {
            v.exp()
        } else {
            v
        }
    }

    /// The designs of every section from the current (smoothed) values. `None` = bypassed.
    fn compute_designs(&self) -> [Option<Biquad>; NS] {
        let sr = self.sr as f64;
        let mut d: [Option<Biquad>; NS] = [None; NS];
        let lc = self.cur(p::LOWCUT) as f64;
        if lc > 10.5 {
            if self.values[p::LOWCUT_SLOPE] >= 0.5 {
                d[LC1] = Some(design(Analog::high_pass(BW2[0]), lc, sr));
                d[LC2] = Some(design(Analog::high_pass(BW2[1]), lc, sr));
            } else {
                // 12 dB / oct with ReaEQ's default 2-octave shape (Q 0.67): golem's presets were voiced on it
                d[LC1] = Some(design(Analog::high_pass(0.667), lc, sr));
            }
        }
        let lows = self.cur(p::LOWS) as f64;
        if lows != 0.0 {
            d[LSH] = Some(design(Analog::low_shelf(0.707, lows), self.cur(p::LOWFREQ) as f64, sr));
        }
        let prop = self.cur(p::MIDPROP) as f64 * 0.01;
        let bell = |g: f64, f: f64, q: f64| {
            // proportional Q: narrows with gain (× 2 at ±12 dB with 100 %)
            let qe = q * 10f64.powf(prop * g.abs() / 40.0);
            design(Analog::bell(qe, g), f, sr)
        };
        let g1 = self.cur(p::MIDGAIN) as f64;
        if g1 != 0.0 {
            d[BELL1] = Some(bell(g1, self.cur(p::MIDFREQ) as f64, self.cur(p::MIDQ) as f64));
        }
        let g2 = self.cur(p::MID2GAIN) as f64;
        if g2 != 0.0 {
            d[BELL2] = Some(bell(g2, self.cur(p::MID2FREQ) as f64, self.cur(p::MID2Q) as f64));
        }
        if self.cut_db > 0.0 {
            d[HARSH] = Some(design(Analog::bell(1.5, -self.cut_db as f64), self.cur(p::HARSH_FREQ) as f64, sr));
        }
        let hs = self.cur(p::HIGHS) as f64;
        if hs != 0.0 {
            d[HSH] = Some(design(Analog::high_shelf(0.707, hs), self.cur(p::HIGHFREQ) as f64, sr));
        }
        let air = self.cur(p::AIR) as f64;
        if air > 0.0 {
            d[AIR] = Some(design(Analog::high_shelf(0.5, air), self.cur(p::AIRFREQ) as f64, sr));
        }
        let tilt = self.cur(p::TILT) as f64;
        if tilt != 0.0 {
            d[TLO] = Some(design(Analog::low_shelf(0.5, -tilt * 0.5), 1000.0, sr));
            d[THI] = Some(design(Analog::high_shelf(0.5, tilt * 0.5), 1000.0, sr));
        }
        let hc = self.cur(p::HIGHCUT) as f64;
        if hc < 21500.0 {
            if self.values[p::HIGHCUT_SLOPE] >= 0.5 {
                d[HC1] = Some(design(Analog::low_pass(BW2[0]), hc, sr));
                d[HC2] = Some(design(Analog::low_pass(BW2[1]), hc, sr));
            } else {
                d[HC1] = Some(design(Analog::low_pass(std::f64::consts::FRAC_1_SQRT_2), hc, sr));
            }
        }
        d
    }

    /// Average power gain (dB) of the curve over 40 Hz – 18 kHz, equal weight per octave (pink).
    fn curve_average_db(designs: &[Option<Biquad>; NS], sr: f64) -> f32 {
        let n = 24;
        let mut sum = 0.0;
        for k in 0..n {
            let f = 40.0 * ((0.42 * sr).min(18000.0) / 40.0).powf(k as f64 / (n - 1) as f64);
            let mut m2 = 1.0;
            for b in designs.iter().flatten() {
                m2 *= b.mag2(f, sr);
            }
            sum += m2;
        }
        // power average: what pink noise (equal energy per octave) hears
        (10.0 * (sum / n as f64).max(1e-20).log10()) as f32
    }

    fn control(&mut self) {
        let designs = self.compute_designs();
        for s in 0..NS {
            match designs[s] {
                Some(b) => {
                    let first = !self.active[s];
                    for ch in self.sections.iter_mut() {
                        if first {
                            ch[s].reset();
                            ch[s].set_biquad(&b);
                        } else if self.designs[s] != Some(b) {
                            ch[s].set_biquad_ramped(&b, CTL as u32);
                        }
                    }
                    self.active[s] = true;
                    self.fade[s].set_target(1.0);
                }
                None => {
                    // keep the last design while it fades out
                    if self.active[s] {
                        self.fade[s].set_target(0.0);
                    }
                }
            }
        }
        // the de-harsh detector's band
        let hf = self.cur(p::HARSH_FREQ);
        for bp in self.harsh_bp.iter_mut() {
            bp.set(SvfMode::BandPass, hf, 1.5, 0.0, self.sr);
        }
        if self.values[p::AUTO_GAIN] >= 0.5 {
            self.ag_tick += 1;
            if self.ag_tick >= 8 {
                self.ag_tick = 0;
                self.auto_db.set_target(-Self::curve_average_db(&designs, self.sr as f64));
            }
        } else {
            self.auto_db.set_target(0.0);
        }
        for s in 0..NS {
            if designs[s].is_some() {
                self.designs[s] = designs[s];
            }
        }
    }

    #[allow(clippy::needless_range_loop)]
    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(MAX_CH);
        let n = buffers.first().map_or(0, |b| b.len());
        let mut meter = 0.0f32;
        for i in 0..n {
            if self.clock.is_multiple_of(CTL) {
                // filter parameters advance in steps of the grid (their coefficients ramp in
                // between); the output gain advances per sample below
                for _ in 0..CTL {
                    for (k, s) in self.smooth.iter_mut().enumerate() {
                        if k != p::OUTPUT {
                            s.next();
                        }
                    }
                }
                self.control();
            }
            self.clock += 1;

            // de-harsh detection: band level relative to the full band (linked over channels)
            let thresh = self.values[p::HARSH_THRESH];
            if thresh < 0.0 {
                let (mut band, mut full) = (0.0f64, 0.0f64);
                for c in 0..nch {
                    let x = buffers[c][i];
                    let b = self.harsh_bp[c].process(x) as f64;
                    band = band.max(b * b);
                    full = full.max((x as f64) * (x as f64));
                }
                let ab = if band > self.e_band { self.det_att } else { self.det_rel };
                self.e_band = band + (self.e_band - band) * ab;
                let af = if full > self.e_full { self.det_att } else { self.det_rel };
                self.e_full = full + (self.e_full - full) * af;
                let rel = if self.e_full > 1e-12 { 10.0 * (self.e_band / self.e_full).max(1e-12).log10() } else { -120.0 };
                let target = ((rel as f32 - thresh) * 0.75).clamp(0.0, self.values[p::HARSH_RANGE]);
                let k = if target > self.cut_db { self.cut_att } else { self.cut_rel };
                self.cut_db = target + (self.cut_db - target) * k;
                if self.cut_db < 1e-3 && target == 0.0 {
                    self.cut_db = 0.0;
                }
            } else if self.cut_db > 0.0 {
                // switched off: the cut releases instead of vanishing
                self.cut_db *= self.cut_rel;
                if self.cut_db < 1e-3 {
                    self.cut_db = 0.0;
                }
            }
            meter = meter.max(self.cut_db);

            let mut fades = [0.0f32; NS];
            for s in 0..NS {
                if self.active[s] {
                    fades[s] = self.fade[s].next();
                    if fades[s] == 0.0 && self.fade[s].is_settled() {
                        self.active[s] = false;
                        self.designs[s] = None;
                    }
                }
            }
            let out_db = self.smooth[p::OUTPUT].next() + self.auto_db.next();
            let out = db_gain(out_db);
            for c in 0..nch {
                let mut x = buffers[c][i];
                for s in 0..NS {
                    if self.active[s] {
                        let y = self.sections[c][s].process(x);
                        let m = fades[s];
                        x = if m >= 1.0 { y } else { x + (y - x) * m };
                    }
                }
                buffers[c][i] = if out == 1.0 { x } else { x * out };
            }
        }
        self.gr_meter = meter;
    }
}

impl Default for Eq {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Eq {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        for s in self.smooth.iter_mut() {
            s.prepare(sample_rate);
        }
        self.auto_db.prepare(sample_rate);
        for f in self.fade.iter_mut() {
            f.prepare(sample_rate);
        }
        let sr = sample_rate as f64;
        self.det_att = one_pole_coef(0.001 * sr);
        self.det_rel = one_pole_coef(0.08 * sr);
        self.cut_att = one_pole_coef(0.001 * sr) as f32;
        self.cut_rel = one_pole_coef(0.08 * sr) as f32;
    }

    fn reset(&mut self) {
        for s in self.smooth.iter_mut() {
            s.reset();
        }
        for ch in self.sections.iter_mut() {
            ch.iter_mut().for_each(Svf::reset);
        }
        self.harsh_bp.iter_mut().for_each(Svf::reset);
        self.active = [false; NS];
        self.designs = [None; NS];
        // bands present at reset start fully in, as if they had always been there
        let d = self.compute_designs();
        for s in 0..NS {
            if let Some(b) = d[s] {
                for ch in self.sections.iter_mut() {
                    ch[s].set_biquad(&b);
                }
                self.active[s] = true;
                self.designs[s] = Some(b);
                self.fade[s].set_immediate(1.0);
            } else {
                self.fade[s].set_immediate(0.0);
            }
        }
        self.e_band = 0.0;
        self.e_full = 0.0;
        self.cut_db = 0.0;
        self.ag_tick = 7;
        self.clock = 0;
        self.gr_meter = 0.0;
        if self.values[p::AUTO_GAIN] >= 0.5 {
            let d = self.compute_designs();
            self.auto_db.set_immediate(-Self::curve_average_db(&d, self.sr as f64));
        } else {
            self.auto_db.set_immediate(0.0);
        }
    }

    fn set_param(&mut self, id: &str, v: f32) {
        if let Some(i) = PARAMS.iter().position(|q| q.id == id) {
            self.set_index(i, v);
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_block(buffers);
    }
}

#[cfg(test)]
mod tests;
