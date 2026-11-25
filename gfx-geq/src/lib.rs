// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-geq — golem's graphic equalizer (design: README.md).
//!
//! Stands for golem's `eq-graphic` block (ids `g1` … `g11` kept, dB, band order). The response is the
//! curve the faders draw: an **accurate cascade** (gfx-dsp `geq`: ACGE with fill-in sections, matched
//! bells realised in TPT state-variable filters) solved on a 1.33 ms control tick, one warm-started
//! correction per tick while a target moves, its coefficients ramped over the tick. Zero latency.
//!
//! - **Layouts**: Octave 11 (`g1` … `g11`), Guitar 7 (`gt_*`), Bass 7 (`gb_*`): one fader set each,
//!   never remapped; `layout` picks the set that drives the EQ. Two engine slots: a layout change
//!   starts the other slot from rest on the live input and crossfades over 20 ms.
//! - **Flat is flat**: all faders at 0 (or `amount` at 0) solve to exactly 0 dB; after 50 ms there the
//!   cascade fades out (5 ms) and is bypassed. Sections are never bypassed one by one.
//! - Around the bands: low cut ▶ [cascade] ▶ high cut (matched Butterworth, 12 / 24 dB / oct, bypassed
//!   at their end stops) ▶ static auto gain (from the curve, never the signal) ▶ level.
//!
//! README §3.4's Pedal mode is not in v1 (README "As coded"): its network, with the placeholder
//! constants, does not reach its own ±15 dB. `mode` will be appended when it is voiced.

#![allow(clippy::needless_range_loop)]

use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::geq::{Phi, Plan, MAX_BANDS, MAX_POINTS, MAX_SECTIONS, TOLERANCE_DB};
use gfx_dsp::matched::{design, Analog, Biquad};
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::Svf;
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
pub mod plugin;

#[cfg(test)]
mod tests;

const MAX_CH: usize = 2;
pub const NP: usize = 33;

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::float("g1", "G1", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("g2", "G2", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("g3", "G3", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("g4", "G4", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("g5", "G5", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("g6", "G6", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("g7", "G7", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("g8", "G8", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("g9", "G9", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("g10", "G10", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("g11", "G11", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::enumeration("layout", "Layout", 3, 0),
    ParamInfo::float("level", "Level", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("amount", "Amount", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::float("gt_100", "Guitar 100", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("gt_200", "Guitar 200", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("gt_400", "Guitar 400", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("gt_800", "Guitar 800", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("gt_1k6", "Guitar 1.6k", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("gt_3k2", "Guitar 3.2k", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("gt_6k4", "Guitar 6.4k", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("gb_50", "Bass 50", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("gb_120", "Bass 120", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("gb_400", "Bass 400", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("gb_500", "Bass 500", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("gb_800", "Bass 800", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("gb_4k5", "Bass 4.5k", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("gb_10k", "Bass 10k", -15.0, 15.0, 0.0).with_exp(),
    ParamInfo::float("lowcut", "Low Cut", 10.0, 500.0, 10.0),
    ParamInfo::enumeration("lowcut_slope", "Low Cut Slope", 2, 0),
    ParamInfo::float("highcut", "High Cut", 1000.0, 22000.0, 22000.0),
    ParamInfo::enumeration("highcut_slope", "High Cut Slope", 2, 0),
    ParamInfo::boolean("auto_gain", "Auto Gain", false),
];

/// Parameter indices.
pub mod p {
    pub const G1: usize = 0;
    pub const LAYOUT: usize = 11;
    pub const LEVEL: usize = 12;
    pub const AMOUNT: usize = 13;
    /// `gt_100`, the first Guitar 7 fader
    pub const GT: usize = 14;
    /// `gb_50`, the first Bass 7 fader
    pub const GB: usize = 21;
    pub const LOWCUT: usize = 28;
    pub const LOWCUT_SLOPE: usize = 29;
    pub const HIGHCUT: usize = 30;
    pub const HIGHCUT_SLOPE: usize = 31;
    pub const AUTO_GAIN: usize = 32;
}

/// Layout option labels (enum order, final; append only).
pub const LAYOUTS: [&str; 3] = ["Octave 11", "Guitar 7", "Bass 7"];
/// Fader centres of each layout (Hz).
pub const OCTAVE: [f64; 11] = [31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 12000.0, 16000.0];
pub const GUITAR: [f64; 7] = [100.0, 200.0, 400.0, 800.0, 1600.0, 3200.0, 6400.0];
pub const BASS: [f64; 7] = [50.0, 120.0, 400.0, 500.0, 800.0, 4500.0, 10000.0];

/// A layout's fader centres and the index of its first fader parameter.
pub fn layout(l: usize) -> (&'static [f64], usize) {
    match l {
        1 => (&GUITAR, p::GT),
        2 => (&BASS, p::GB),
        _ => (&OCTAVE, p::G1),
    }
}

/// The control tick: 1.33 ms (64 samples at 48 kHz), defined in time.
const TICK_S: f64 = 1.0 / 750.0;
/// Corrections go on this many ticks after the target stops.
const SETTLE_TICKS: u32 = 4;
/// Every gain below this (dB) for 50 ms: the cascade fades out and is bypassed.
const FLAT_DB: f64 = 1e-4;
const FLAT_HOLD_S: f64 = 0.05;
const BYPASS_FADE_MS: f32 = 5.0;
const SWITCH_FADE_MS: f32 = 20.0;
const NO_LAYOUT: usize = usize::MAX;

// cut sections, in order: two low-cut, two high-cut
const LC1: usize = 0;
const LC2: usize = 1;
const HC1: usize = 2;
const HC2: usize = 3;
const NC: usize = 4;
const BW2: [f64; 2] = [0.541_196_1, 1.306_563];

#[inline]
fn db_gain(db: f32) -> f32 {
    if db == 0.0 {
        1.0
    } else {
        10f32.powf(db * 0.05)
    }
}

const UNITY: Biquad = Biquad { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };

/// One engine: the accurate cascade of one layout, its solver state and its filters.
struct Slot {
    layout: usize,
    n: usize,
    g: [f64; MAX_SECTIONS],
    designs: [Biquad; MAX_SECTIONS],
    /// the fader targets (dB × amount) the target points were computed from
    faders: [f64; MAX_BANDS],
    t: [f64; MAX_POINTS],
    /// ticks since the target last moved
    still: u32,
    last_dg: f64,
    /// ticks every gain has stayed flat
    flat_ticks: u32,
    /// the cascade runs (else the slot passes its input through untouched)
    engaged: bool,
    wet: LinearRamp,
    m: f32,
    svf: [[Svf; MAX_SECTIONS]; MAX_CH],
    /// corrections made by the last cold start (tests, bench)
    cold_corrections: usize,
}

impl Slot {
    fn new() -> Self {
        Self {
            layout: NO_LAYOUT,
            n: 0,
            g: [0.0; MAX_SECTIONS],
            designs: [UNITY; MAX_SECTIONS],
            faders: [0.0; MAX_BANDS],
            t: [0.0; MAX_POINTS],
            still: 0,
            last_dg: 0.0,
            flat_ticks: 0,
            engaged: false,
            wet: LinearRamp::new(BYPASS_FADE_MS, 0.0),
            m: 0.0,
            svf: Default::default(),
            cold_corrections: 0,
        }
    }

    fn flat(&self) -> bool {
        self.g[..self.n].iter().all(|v| v.abs() < FLAT_DB)
    }

    /// Cold start on `layout` from rest: solved to convergence, coefficients set at once.
    fn start(&mut self, layout: usize, plan: &Plan, faders: &[f64]) {
        self.layout = layout;
        self.n = plan.len();
        self.faders[..faders.len()].copy_from_slice(faders);
        plan.targets(faders, &mut self.t);
        self.cold_corrections = plan.solve(&self.t, &mut self.g, &mut self.designs);
        self.still = 0;
        self.last_dg = 0.0;
        self.flat_ticks = 0;
        self.engaged = !self.flat();
        for ch in self.svf.iter_mut() {
            for (s, d) in ch.iter_mut().zip(self.designs.iter()).take(self.n) {
                s.reset();
                s.set_biquad(d);
            }
        }
        self.wet.set_immediate(if self.engaged { 1.0 } else { 0.0 });
        self.m = self.wet.value();
    }

    /// One control tick: a warm-started correction while the target moves (and a few ticks after),
    /// then the coefficients ramp over the tick; the fade to / from bypass.
    fn tick(&mut self, plan: &Plan, faders: &[f64], len: u32, flat_hold: u32) {
        let k = faders.len();
        let moved = self.faders[..k] != faders[..k];
        if moved {
            self.faders[..k].copy_from_slice(faders);
            self.still = 0;
        } else {
            self.still = self.still.saturating_add(1);
        }
        let mut changed = false;
        if moved || self.still <= SETTLE_TICKS || self.last_dg >= TOLERANCE_DB {
            if faders.iter().all(|&v| v == 0.0) {
                // flat is flat: exactly 0 dB, no iteration towards it
                if self.g[..self.n].iter().any(|&v| v != 0.0) {
                    self.g = [0.0; MAX_SECTIONS];
                    plan.designs(&self.g, &mut self.designs);
                    changed = true;
                }
                self.last_dg = 0.0;
            } else {
                if moved {
                    plan.targets(faders, &mut self.t);
                }
                self.last_dg = plan.correct(&self.t, &mut self.g, &mut self.designs);
                changed = true;
            }
        }
        let flat = self.flat();
        self.flat_ticks = if flat { self.flat_ticks.saturating_add(1) } else { 0 };
        if !flat && !self.engaged {
            // out of bypass: the cascade starts from rest (near the identity) and fades in
            for ch in self.svf.iter_mut() {
                for (s, d) in ch.iter_mut().zip(self.designs.iter()).take(self.n) {
                    s.reset();
                    s.set_biquad(d);
                }
            }
            self.engaged = true;
            self.wet.set_immediate(0.0);
            self.wet.set_target(1.0);
            return;
        }
        if self.engaged {
            if changed {
                for ch in self.svf.iter_mut() {
                    for (s, d) in ch.iter_mut().zip(self.designs.iter()).take(self.n) {
                        s.set_biquad_ramped(d, len);
                    }
                }
            }
            self.wet.set_target(if flat && self.flat_ticks >= flat_hold { 0.0 } else { 1.0 });
        }
    }

    /// Per sample, before `run`: the bypass fade.
    #[inline]
    fn advance(&mut self) {
        if self.engaged {
            self.m = self.wet.next();
            if self.m == 0.0 && self.wet.is_settled() {
                self.engaged = false;
            }
        }
    }

    #[inline]
    fn run(&mut self, c: usize, x: f32) -> f32 {
        if !self.engaged {
            return x;
        }
        let mut y = x;
        for s in self.svf[c][..self.n].iter_mut() {
            y = s.process(y);
        }
        let m = self.m;
        if m >= 1.0 {
            y
        } else {
            x + (y - x) * m
        }
    }
}

pub struct Geq {
    sr: f32,
    tick_len: u32,
    flat_hold: u32,
    values: [f32; NP],
    smooth: Vec<ExpSmoother>,
    log: [bool; NP],
    plans: Vec<Plan>,
    slots: [Slot; 2],
    cur: usize,
    switching: bool,
    xfade: LinearRamp,
    cuts: [[Svf; NC]; MAX_CH],
    cut_active: [bool; NC],
    cut_designs: [Option<Biquad>; NC],
    cut_fade: Vec<LinearRamp>,
    auto_db: OnePole,
    ag_tick: u32,
    clock: u64,
}

impl Geq {
    pub fn new() -> Self {
        let mut log = [false; NP];
        log[p::LOWCUT] = true;
        log[p::HIGHCUT] = true;
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
        let mut s = Self {
            sr: 48000.0,
            tick_len: 64,
            flat_hold: 38,
            values,
            smooth,
            log,
            plans: Vec::new(),
            slots: [Slot::new(), Slot::new()],
            cur: 0,
            switching: false,
            xfade: LinearRamp::new(SWITCH_FADE_MS, 1.0),
            cuts: Default::default(),
            cut_active: [false; NC],
            cut_designs: [None; NC],
            cut_fade: (0..NC).map(|_| LinearRamp::new(BYPASS_FADE_MS, 0.0)).collect(),
            auto_db: OnePole::new(50.0, 0.0, 1e-6),
            ag_tick: 0,
            clock: 0,
        };
        s.prepare(48000.0, 512, 2);
        s.reset();
        s
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
        if !Self::is_switch(i) {
            let t = if self.log[i] { v.ln() } else { v };
            self.smooth[i].set_target(t, self.clock);
        }
    }

    #[inline]
    fn is_switch(i: usize) -> bool {
        matches!(i, p::LAYOUT | p::LOWCUT_SLOPE | p::HIGHCUT_SLOPE | p::AUTO_GAIN)
    }

    #[inline]
    fn cur(&self, i: usize) -> f32 {
        if Self::is_switch(i) {
            return self.values[i];
        }
        let v = self.smooth[i].value();
        if self.log[i] {
            v.exp()
        } else {
            v
        }
    }

    /// The fader targets of layout `l` (smoothed dB × amount); returns their count.
    fn faders(&self, l: usize, out: &mut [f64; MAX_BANDS]) -> usize {
        let (freqs, first) = layout(l);
        let amount = self.cur(p::AMOUNT) as f64 * 0.01;
        for k in 0..freqs.len() {
            let v = self.cur(first + k) as f64;
            out[k] = if amount == 1.0 { v } else { v * amount };
        }
        freqs.len()
    }

    fn selected_layout(&self) -> usize {
        (self.values[p::LAYOUT].round() as usize).min(LAYOUTS.len() - 1)
    }

    /// The slot that is heard (or fading in).
    fn live(&self) -> &Slot {
        &self.slots[self.cur]
    }

    /// Current section gains of the heard slot (dB), for tests and the bench.
    pub fn section_gains(&self) -> &[f64] {
        let s = self.live();
        &s.g[..s.n]
    }

    /// Corrections of the heard slot's last cold start.
    pub fn cold_corrections(&self) -> usize {
        self.live().cold_corrections
    }

    /// The solver plan of layout `l` at the current sample rate.
    pub fn plan(&self, l: usize) -> &Plan {
        &self.plans[l]
    }

    /// Designs of the cut sections from the current values. `None` = bypassed.
    fn compute_cuts(&self) -> [Option<Biquad>; NC] {
        let sr = self.sr as f64;
        let mut d = [None; NC];
        let lc = self.cur(p::LOWCUT) as f64;
        if lc > 10.5 {
            if self.values[p::LOWCUT_SLOPE] >= 0.5 {
                d[LC1] = Some(design(Analog::high_pass(BW2[0]), lc, sr));
                d[LC2] = Some(design(Analog::high_pass(BW2[1]), lc, sr));
            } else {
                d[LC1] = Some(design(Analog::high_pass(std::f64::consts::FRAC_1_SQRT_2), lc, sr));
            }
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

    /// Average power gain (dB) of the realised curve over 40 Hz – 18 kHz, equal weight per octave
    /// (what pink noise hears): the heard slot's cascade and the cuts.
    fn curve_average_db(&self, cuts: &[Option<Biquad>; NC]) -> f32 {
        let sr = self.sr as f64;
        let s = self.live();
        let bands = if s.flat() { &s.designs[..0] } else { &s.designs[..s.n] };
        // 64 points (7 per octave): a zigzag on an octave grid is not aliased away
        let n = 64;
        let mut sum = 0.0;
        for k in 0..n {
            let f = 40.0 * ((0.42 * sr).min(18000.0) / 40.0).powf(k as f64 / (n - 1) as f64);
            let phi = Phi::new(f, sr);
            let mut m2 = 1.0;
            for b in bands.iter().chain(cuts.iter().flatten()) {
                m2 *= phi.mag2(b);
            }
            sum += m2;
        }
        (10.0 * (sum / n as f64).max(1e-20).log10()) as f32
    }

    fn control(&mut self) {
        // a layout change starts the other slot (the current crossfade finishes first)
        let want = self.selected_layout();
        if !self.switching && want != self.slots[self.cur].layout {
            let mut f = [0.0; MAX_BANDS];
            let k = self.faders(want, &mut f);
            let o = 1 - self.cur;
            self.slots[o].start(want, &self.plans[want], &f[..k]);
            self.cur = o;
            self.switching = true;
            self.xfade.set_immediate(0.0);
            self.xfade.set_target(1.0);
        }
        for i in 0..2 {
            if i != self.cur && !self.switching {
                continue;
            }
            let l = self.slots[i].layout;
            if l == NO_LAYOUT {
                continue;
            }
            let mut f = [0.0; MAX_BANDS];
            let k = self.faders(l, &mut f);
            self.slots[i].tick(&self.plans[l], &f[..k], self.tick_len, self.flat_hold);
        }

        let designs = self.compute_cuts();
        for s in 0..NC {
            match designs[s] {
                Some(b) => {
                    let first = !self.cut_active[s];
                    for ch in self.cuts.iter_mut() {
                        if first {
                            ch[s].reset();
                            ch[s].set_biquad(&b);
                        } else if self.cut_designs[s] != Some(b) {
                            ch[s].set_biquad_ramped(&b, self.tick_len);
                        }
                    }
                    self.cut_active[s] = true;
                    self.cut_designs[s] = Some(b);
                    self.cut_fade[s].set_target(1.0);
                }
                None => {
                    // keep the last design while it fades out
                    if self.cut_active[s] {
                        self.cut_fade[s].set_target(0.0);
                    }
                }
            }
        }
        if self.values[p::AUTO_GAIN] >= 0.5 {
            self.ag_tick += 1;
            if self.ag_tick >= 8 {
                self.ag_tick = 0;
                let a = self.curve_average_db(&designs);
                self.auto_db.set_target(-a);
            }
        } else {
            self.auto_db.set_target(0.0);
        }
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(MAX_CH);
        let n = buffers.first().map_or(0, |b| b.len());
        for i in 0..n {
            if self.clock.is_multiple_of(self.tick_len as u64) {
                // the curve's parameters advance in steps of the tick (their coefficients ramp in
                // between); the level advances per sample below
                for (k, s) in self.smooth.iter_mut().enumerate() {
                    if k != p::LEVEL && !s.is_settled() {
                        for _ in 0..self.tick_len {
                            s.next();
                        }
                    }
                }
                self.control();
            }
            self.clock += 1;

            let mut xm = 1.0f32;
            if self.switching {
                xm = self.xfade.next();
                if xm >= 1.0 && self.xfade.is_settled() {
                    self.switching = false;
                }
            }
            let old = 1 - self.cur;
            self.slots[self.cur].advance();
            if self.switching || xm < 1.0 {
                self.slots[old].advance();
            }
            let mut fades = [0.0f32; NC];
            for s in 0..NC {
                if self.cut_active[s] {
                    fades[s] = self.cut_fade[s].next();
                    if fades[s] == 0.0 && self.cut_fade[s].is_settled() {
                        self.cut_active[s] = false;
                        self.cut_designs[s] = None;
                    }
                }
            }
            let out = db_gain(self.smooth[p::LEVEL].next() + self.auto_db.next());
            for c in 0..nch {
                let mut x = buffers[c][i];
                for s in [LC1, LC2] {
                    if self.cut_active[s] {
                        let y = self.cuts[c][s].process(x);
                        let m = fades[s];
                        x = if m >= 1.0 { y } else { x + (y - x) * m };
                    }
                }
                let mut y = self.slots[self.cur].run(c, x);
                if xm < 1.0 {
                    let yo = self.slots[old].run(c, x);
                    y = yo + (y - yo) * xm;
                }
                for s in [HC1, HC2] {
                    if self.cut_active[s] {
                        let z = self.cuts[c][s].process(y);
                        let m = fades[s];
                        y = if m >= 1.0 { z } else { y + (z - y) * m };
                    }
                }
                buffers[c][i] = if out == 1.0 { y } else { y * out };
            }
        }
    }
}

impl Default for Geq {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Geq {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        let sr = sample_rate as f64;
        self.tick_len = ((sr * TICK_S).round() as u32).max(1);
        self.flat_hold = ((FLAT_HOLD_S * sr) / self.tick_len as f64).ceil() as u32;
        for s in self.smooth.iter_mut() {
            s.prepare(sample_rate);
        }
        // the solver's plans: fixed-size, a few tens of µs each
        self.plans = (0..LAYOUTS.len()).map(|l| Plan::new(layout(l).0, sr)).collect();
        for s in self.slots.iter_mut() {
            s.wet.prepare(sample_rate);
        }
        self.xfade.prepare(sample_rate);
        for f in self.cut_fade.iter_mut() {
            f.prepare(sample_rate);
        }
        self.auto_db.prepare(sample_rate);
    }

    fn reset(&mut self) {
        for s in self.smooth.iter_mut() {
            s.reset();
        }
        let l = self.selected_layout();
        let mut f = [0.0; MAX_BANDS];
        let k = self.faders(l, &mut f);
        self.cur = 0;
        self.switching = false;
        self.xfade.set_immediate(1.0);
        self.slots[1].layout = NO_LAYOUT;
        self.slots[1].engaged = false;
        self.slots[0].start(l, &self.plans[l], &f[..k]);
        // cuts present at reset start fully in, as if they had always been there
        let d = self.compute_cuts();
        for s in 0..NC {
            for ch in self.cuts.iter_mut() {
                ch[s].reset();
                if let Some(b) = d[s] {
                    ch[s].set_biquad(&b);
                }
            }
            self.cut_active[s] = d[s].is_some();
            self.cut_designs[s] = d[s];
            self.cut_fade[s].set_immediate(if d[s].is_some() { 1.0 } else { 0.0 });
        }
        self.ag_tick = 7;
        self.clock = 0;
        if self.values[p::AUTO_GAIN] >= 0.5 {
            let a = self.curve_average_db(&d);
            self.auto_db.set_immediate(-a);
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
