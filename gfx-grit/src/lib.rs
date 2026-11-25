// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-grit — golem's multiband distortion (design: README.md).
//!
//! Stands for golem's `grit` block (25 catalogue ids and names kept, today UnplugRed's Prisma: its
//! laws restated as formulas in the README, none of its code, tables or impulse responses used).
//!
//! - **One oversampling domain** (engine.rs): up 2× (Quality off) or 4× (on) once — 1× / 2× at host
//!   rates ≥ 88.2 kHz — the crossovers, every module and the band sum at the inner rate, down once.
//!   Every path carries exactly one inner sample of averaging (ADAA1 or a two-tap average), and one
//!   droop correction after the sum gives the highs back.
//! - **Crossovers**: gfx-dsp `crossover::Tree`, LR4 or LR8 with compensation all-passes: the clean
//!   sum is an all-pass, the band phases equal.
//! - **Bands**: two slots of 25 modules (modules.rs, gfx-dsp `shapers`), an always-on 5 Hz DC block on the
//!   wet, a per-band dry / wet against the aligned dry, a band gain on the whole band, solo / bypass.
//! - **Level**: the saturators carry a static make-up at a reference level (build-time tables).
//! - **Face**: Drive and Focus add weighted points to every nonlinear module's Amount.
//! - `quality` / `slope` pre-roll a second engine for 30 ms, then crossfade over 10 ms.
//!
//! Zero reported latency (the half-bands' minimum-phase group delay, one inner sample of averaging and
//! the crossover dispersion are not reported). Real-time safe, deterministic.

#![allow(clippy::needless_range_loop)]

pub mod convert;
/// The curves and laws, now gfx-dsp's `shapers` (moved verbatim, renders byte-identical).
pub use gfx_dsp::shapers as curves;
pub mod engine;
pub mod modules;

#[cfg(feature = "plugin")]
pub mod plugin;

#[cfg(test)]
mod tests;

include!(concat!(env!("OUT_DIR"), "/comp_tables.rs"));

use engine::{Config, Engine, Frame, BANDS};
use gfx_dsp::crossover::Order;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::{Effect, ParamInfo};
use modules::{raised_cos, Kind, ModParams};
use std::f64::consts::PI;

pub const NP: usize = 50;

pub static PARAMS: [ParamInfo; NP] = [
    // the catalogue's 25, in its order
    ParamInfo::enumeration("lo_m1", "Low Module 1 Type", 25, 0),
    ParamInfo::float("lo_v1", "Low Module 1 Value", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::enumeration("lo_m2", "Low Module 2 Type", 25, 0),
    ParamInfo::float("lo_v2", "Low Module 2 Value", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("lo_gain", "Low Gain", -60.0, 12.0, 0.0).with_exp(),
    ParamInfo::enumeration("lm_m1", "Low Mid Module 1 Type", 25, 0),
    ParamInfo::float("lm_v1", "Low Mid Module 1 Value", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::enumeration("lm_m2", "Low Mid Module 2 Type", 25, 0),
    ParamInfo::float("lm_v2", "Low Mid Module 2 Value", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("lm_gain", "Low Mid Gain", -60.0, 12.0, 0.0).with_exp(),
    ParamInfo::enumeration("hm_m1", "High Mid Module 1 Type", 25, 0),
    ParamInfo::float("hm_v1", "High Mid Module 1 Value", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::enumeration("hm_m2", "High Mid Module 2 Type", 25, 0),
    ParamInfo::float("hm_v2", "High Mid Module 2 Value", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("hm_gain", "High Mid Gain", -60.0, 12.0, 0.0).with_exp(),
    ParamInfo::enumeration("hi_m1", "High Module 1 Type", 25, 0),
    ParamInfo::float("hi_v1", "High Module 1 Value", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::enumeration("hi_m2", "High Module 2 Type", 25, 0),
    ParamInfo::float("hi_v2", "High Module 2 Value", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("hi_gain", "High Gain", -60.0, 12.0, 0.0).with_exp(),
    ParamInfo::float("x1", "Low Mid Crossover", 0.0, 100.0, 25.0).with_exp(),
    ParamInfo::float("x2", "High Mid Crossover", 0.0, 100.0, 50.0).with_exp(),
    ParamInfo::float("x3", "High Crossover", 0.0, 100.0, 75.0).with_exp(),
    ParamInfo::float("wet", "Wet", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::boolean("quality", "Quality", false),
    // the face
    ParamInfo::float("drive", "Drive", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("focus", "Focus", -100.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("level", "Level", -24.0, 12.0, 0.0).with_exp(),
    // expert, per band
    ParamInfo::float("lo_mix", "Low Mix", 0.0, 100.0, 100.0),
    ParamInfo::float("lo_y1", "Low Module 1 Peak Gain", -30.0, 30.0, 9.0),
    ParamInfo::float("lo_y2", "Low Module 2 Peak Gain", -30.0, 30.0, 9.0),
    ParamInfo::boolean("lo_solo", "Low Solo", false),
    ParamInfo::boolean("lo_bypass", "Low Bypass", false),
    ParamInfo::float("lm_mix", "Low Mid Mix", 0.0, 100.0, 100.0),
    ParamInfo::float("lm_y1", "Low Mid Module 1 Peak Gain", -30.0, 30.0, 9.0),
    ParamInfo::float("lm_y2", "Low Mid Module 2 Peak Gain", -30.0, 30.0, 9.0),
    ParamInfo::boolean("lm_solo", "Low Mid Solo", false),
    ParamInfo::boolean("lm_bypass", "Low Mid Bypass", false),
    ParamInfo::float("hm_mix", "High Mid Mix", 0.0, 100.0, 100.0),
    ParamInfo::float("hm_y1", "High Mid Module 1 Peak Gain", -30.0, 30.0, 9.0),
    ParamInfo::float("hm_y2", "High Mid Module 2 Peak Gain", -30.0, 30.0, 9.0),
    ParamInfo::boolean("hm_solo", "High Mid Solo", false),
    ParamInfo::boolean("hm_bypass", "High Mid Bypass", false),
    ParamInfo::float("hi_mix", "High Mix", 0.0, 100.0, 100.0),
    ParamInfo::float("hi_y1", "High Module 1 Peak Gain", -30.0, 30.0, 9.0),
    ParamInfo::float("hi_y2", "High Module 2 Peak Gain", -30.0, 30.0, 9.0),
    ParamInfo::boolean("hi_solo", "High Solo", false),
    ParamInfo::boolean("hi_bypass", "High Bypass", false),
    ParamInfo::enumeration("slope", "Crossover Slope", 2, 0),
    ParamInfo::boolean("bypass", "Bypass", false),
];

pub mod p {
    /// Band b's catalogue block starts at 5 b: m1, v1, m2, v2, gain.
    pub const fn m(b: usize, s: usize) -> usize {
        5 * b + 2 * s
    }
    pub const fn v(b: usize, s: usize) -> usize {
        5 * b + 2 * s + 1
    }
    pub const fn gain(b: usize) -> usize {
        5 * b + 4
    }
    pub const X1: usize = 20;
    pub const WET: usize = 23;
    pub const QUALITY: usize = 24;
    pub const DRIVE: usize = 25;
    pub const FOCUS: usize = 26;
    pub const LEVEL: usize = 27;
    /// Band b's expert block starts at 28 + 5 b: mix, y1, y2, solo, bypass.
    pub const fn mix(b: usize) -> usize {
        28 + 5 * b
    }
    pub const fn y(b: usize, s: usize) -> usize {
        29 + 5 * b + s
    }
    pub const fn solo(b: usize) -> usize {
        31 + 5 * b
    }
    pub const fn band_bypass(b: usize) -> usize {
        32 + 5 * b
    }
    pub const SLOPE: usize = 48;
    pub const BYPASS: usize = 49;
}

/// Slope option labels (enum order, final).
pub const SLOPES: [&str; 2] = ["24 dB/oct", "48 dB/oct"];
pub use modules::{CABINETS, MODULE_NAMES};

/// Focus weights of the bands (lo, lm, hm, hi).
const FOCUS_W: [f64; BANDS] = [-1.0, -1.0 / 3.0, 1.0 / 3.0, 1.0];
/// Pre-roll and crossfade of a `quality` / `slope` switch (ms).
const PREROLL_MS: f32 = 30.0;
const SWITCH_FADE_MS: f32 = 10.0;

/// Effective crossovers (%) from the three values: x2 ≥ x1 + 2, x3 ≥ x2 + 2 (the later one yields),
/// each in its window (2 – 94, 4 – 96, 6 – 98 %).
pub fn effective_crossovers(x: [f64; 3]) -> [f64; 3] {
    let e1 = x[0].clamp(2.0, 94.0);
    let e2 = x[1].max(e1 + 2.0).clamp(4.0, 96.0);
    let e3 = x[2].max(e2 + 2.0).clamp(6.0, 98.0);
    [e1, e2, e3]
}

/// Band gain (dB) → linear; −60 dB is a mute.
#[inline]
fn band_gain(db: f64) -> f64 {
    if db <= -60.0 {
        0.0
    } else {
        10f64.powf(db / 20.0)
    }
}

/// A 0 – 1 ramp's next value, raised-cosine shaped while it moves (exact 0 or 1 once settled).
#[inline]
fn ramp_shape(r: &mut LinearRamp) -> f64 {
    let v = r.next() as f64;
    if r.is_settled() {
        v
    } else {
        raised_cos(v)
    }
}

/// The effective Amount of a slot after Drive and Focus (README §2.5).
#[inline]
pub fn effective_amount(kind: Kind, v: f64, drive: f64, focus: f64, band: usize) -> f64 {
    let k = kind.weight();
    if k == 0.0 {
        v
    } else {
        (v + k * (drive + focus * FOCUS_W[band] * 0.5) / 100.0).clamp(0.0, 100.0)
    }
}

/// A slot's Amount in the module's own unit (what the plugin displays).
pub fn format_amount(kind: Kind, a: f64) -> String {
    use curves::*;
    let hz = |f: f64| if f >= 1000.0 { format!("{:.2} kHz", f / 1000.0) } else { format!("{f:.0} Hz") };
    match kind {
        Kind::Soft | Kind::Hard | Kind::Fold | Kind::SineFold | Kind::Tube | Kind::Fuzz => format!("{:.1} dB", 0.48 * a),
        Kind::Heavy | Kind::Asym => format!("e {:.3}", heavy_exp(a)),
        Kind::ZeroCross => format!("{:.1} dBFS", zc_threshold_db(a)),
        Kind::BitCrush => format!("{:.2} bits", crush_bits(a)),
        Kind::SampleDivide => hz(hold_rate(a)),
        Kind::Gain => format!("{:+.1} dB", gain_db(a)),
        Kind::Lowpass => hz(freq_down(a)),
        Kind::Highpass | Kind::Peak | Kind::RingMod => hz(freq_up(a)),
        Kind::Dc => format!("{:+.2}", a / 100.0 - 0.5),
        Kind::Cabinet => CABINETS[modules::cab_index(a)].to_string(),
        _ => format!("{a:.0} %"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Switch {
    Idle,
    Preroll(u32),
    Fade(u32),
}

pub struct Grit {
    v: [f32; NP],
    sr: f32,
    nch: usize,
    clock: u64,
    eng: [Engine; 2],
    active: usize,
    sw: Switch,
    /// the running switch crossfades at equal power (slope) rather than equal gain (quality)
    sw_power: bool,
    preroll_len: u32,
    fade_len: u32,
    frame: Frame,
    kinds: [[Kind; 2]; BANDS],
    /// the (kind, amount, peak dB) each slot's parameters were computed for
    cache: [[(Kind, f64, f64); 2]; BANDS],
    gain_cache: [(f64, f64); BANDS],
    level_cache: (f64, f64),
    x_cache: [f64; 3],
    drive: ExpSmoother,
    focus: ExpSmoother,
    wet: ExpSmoother,
    level: ExpSmoother,
    xs: [ExpSmoother; 3],
    amt: [[ExpSmoother; 2]; BANDS],
    gain: [ExpSmoother; BANDS],
    mix: [OnePole; BANDS],
    peak: [[OnePole; 2]; BANDS],
    solo: [LinearRamp; BANDS],
    byp: [LinearRamp; BANDS],
    fades: [[LinearRamp; 2]; BANDS],
    bypass: LinearRamp,
}

impl Grit {
    pub fn new() -> Self {
        let d = |i: usize| PARAMS[i].default;
        let cfg = Config { factor: 2, order: Order::Lr4 };
        let mut g = Self {
            v: core::array::from_fn(|i| PARAMS[i].default),
            sr: 48000.0,
            nch: 1,
            clock: 0,
            eng: [Engine::new(cfg), Engine::new(cfg)],
            active: 0,
            sw: Switch::Idle,
            sw_power: false,
            preroll_len: 1440,
            fade_len: 480,
            frame: Frame::new(),
            kinds: [[Kind::Off; 2]; BANDS],
            cache: [[(Kind::Off, f64::NAN, f64::NAN); 2]; BANDS],
            gain_cache: [(f64::NAN, 1.0); BANDS],
            level_cache: (f64::NAN, 1.0),
            x_cache: [f64::NAN; 3],
            drive: ExpSmoother::new(0.0, 100.0, d(p::DRIVE)),
            focus: ExpSmoother::new(-100.0, 100.0, d(p::FOCUS)),
            wet: ExpSmoother::new(0.0, 100.0, d(p::WET)),
            level: ExpSmoother::new(-24.0, 12.0, d(p::LEVEL)),
            xs: core::array::from_fn(|k| ExpSmoother::new(0.0, 100.0, d(p::X1 + k))),
            amt: core::array::from_fn(|b| core::array::from_fn(|s| ExpSmoother::new(0.0, 100.0, d(p::v(b, s))))),
            gain: core::array::from_fn(|b| ExpSmoother::new(-60.0, 12.0, d(p::gain(b)))),
            mix: core::array::from_fn(|b| OnePole::new(20.0, d(p::mix(b)), 1e-4)),
            peak: core::array::from_fn(|b| core::array::from_fn(|s| OnePole::new(20.0, d(p::y(b, s)), 1e-4))),
            solo: core::array::from_fn(|_| LinearRamp::new(5.0, 1.0)),
            byp: core::array::from_fn(|_| LinearRamp::new(5.0, 0.0)),
            fades: core::array::from_fn(|_| core::array::from_fn(|_| LinearRamp::new(10.0, 1.0))),
            bypass: LinearRamp::new(10.0, 0.0),
        };
        g.set_targets();
        g
    }

    fn want_cfg(&self) -> Config {
        let on = self.v[p::QUALITY] >= 0.5;
        let factor = match (self.sr >= 88200.0, on) {
            (false, false) => 2,
            (false, true) => 4,
            (true, false) => 1,
            (true, true) => 2,
        };
        let order = if self.v[p::SLOPE] >= 0.5 { Order::Lr8 } else { Order::Lr4 };
        Config { factor, order }
    }

    /// The configuration playing now (tests).
    pub fn config(&self) -> Config {
        self.eng[self.active].cfg
    }

    fn set_targets(&mut self) {
        let v = self.v;
        let now = self.clock;
        self.drive.set_target(v[p::DRIVE], now);
        self.focus.set_target(v[p::FOCUS], now);
        self.wet.set_target(v[p::WET], now);
        self.level.set_target(v[p::LEVEL], now);
        for k in 0..3 {
            self.xs[k].set_target(v[p::X1 + k], now);
        }
        let any_solo = (0..BANDS).any(|b| v[p::solo(b)] >= 0.5);
        for b in 0..BANDS {
            self.gain[b].set_target(v[p::gain(b)], now);
            self.mix[b].set_target(v[p::mix(b)]);
            self.solo[b].set_target(if any_solo && v[p::solo(b)] < 0.5 { 0.0 } else { 1.0 });
            self.byp[b].set_target(if v[p::band_bypass(b)] >= 0.5 { 1.0 } else { 0.0 });
            for s in 0..2 {
                self.amt[b][s].set_target(v[p::v(b, s)], now);
                self.peak[b][s].set_target(v[p::y(b, s)]);
                // the fade starts in update_frame(), one change at a time
                self.kinds[b][s] = Kind::from_index(v[p::m(b, s)].round().max(0.0) as usize);
            }
        }
        self.bypass.set_target(if v[p::BYPASS] >= 0.5 { 1.0 } else { 0.0 });
    }

    /// A slot's type (or cabinet voicing) changed: the playing module fades out over 10 ms while
    /// the new one fades in from rest. Changes queue: a new fade starts only once the last one ended
    /// (a module dropped mid-fade would click).
    fn start_fade(&mut self, b: usize, s: usize) {
        let f = &mut self.frame.bands[b].slots[s];
        f.old = f.cur;
        f.gen = f.gen.wrapping_add(1);
        f.fading = true;
        f.fade = 0.0;
        self.fades[b][s].set_immediate(0.0);
        self.fades[b][s].set_target(1.0);
    }

    /// Advances every smoother by one base sample and rebuilds the frame.
    fn update_frame(&mut self) {
        let drive = self.drive.next() as f64;
        let focus = self.focus.next() as f64;
        let wet = self.wet.next() as f64 * 0.01;
        for b in 0..BANDS {
            for s in 0..2 {
                let a = self.amt[b][s].next() as f64;
                let y = self.peak[b][s].next() as f64;
                let mut kind = self.frame.bands[b].slots[s].cur.kind;
                if !self.frame.bands[b].slots[s].fading && self.kinds[b][s] != kind {
                    kind = self.kinds[b][s];
                    self.start_fade(b, s);
                    self.cache[b][s].0 = Kind::Off;
                    self.cache[b][s].1 = f64::NAN;
                }
                let eff = effective_amount(kind, a, drive, focus, b);
                let y = if kind == Kind::Peak { y } else { 0.0 };
                if (kind, eff, y) != self.cache[b][s] {
                    let np = ModParams::new(kind, eff, y, s);
                    let f = &self.frame.bands[b].slots[s];
                    let new_voicing = kind == Kind::Cabinet && f.cur.kind == Kind::Cabinet && np.cab != f.cur.cab;
                    if !(new_voicing && f.fading) {
                        // a new cabinet voicing waits for the running fade to end
                        if new_voicing {
                            self.start_fade(b, s);
                        }
                        self.cache[b][s] = (kind, eff, y);
                        self.frame.bands[b].slots[s].cur = np;
                    }
                }
                let f = &mut self.frame.bands[b].slots[s];
                if f.fading {
                    let r = self.fades[b][s].next() as f64;
                    f.fade = raised_cos(r);
                    if self.fades[b][s].is_settled() {
                        f.fading = false;
                        f.fade = 1.0;
                    }
                }
            }
            let gdb = self.gain[b].next() as f64;
            if gdb != self.gain_cache[b].0 {
                self.gain_cache[b] = (gdb, band_gain(gdb));
            }
            let g = self.gain_cache[b].1;
            let m = wet * self.mix[b].next() as f64 * 0.01;
            let sm = ramp_shape(&mut self.solo[b]);
            let bp = ramp_shape(&mut self.byp[b]);
            let bf = &mut self.frame.bands[b];
            bf.a_wet = sm * (1.0 - bp) * g * m;
            bf.a_dry = sm * (bp + (1.0 - bp) * g * (1.0 - m));
        }
        let x = [self.xs[0].next() as f64, self.xs[1].next() as f64, self.xs[2].next() as f64];
        if x != self.x_cache {
            self.x_cache = x;
            let e = effective_crossovers(x);
            for k in 0..3 {
                self.frame.xf[k] = curves::freq_up(e[k]);
            }
        }
    }

    fn settle(&mut self) {
        self.drive.reset();
        self.focus.reset();
        self.wet.reset();
        self.level.reset();
        for s in self.xs.iter_mut().chain(self.gain.iter_mut()).chain(self.amt.iter_mut().flatten()) {
            s.reset();
        }
        for s in self.mix.iter_mut().chain(self.peak.iter_mut().flatten()) {
            s.settle();
        }
        for r in self.solo.iter_mut().chain(self.byp.iter_mut()).chain(self.fades.iter_mut().flatten()) {
            r.settle();
        }
        self.bypass.settle();
        for b in 0..BANDS {
            for s in 0..2 {
                let f = &mut self.frame.bands[b].slots[s];
                f.fading = false;
                f.fade = 1.0;
                self.cache[b][s].1 = f64::NAN;
            }
        }
        self.update_frame();
        // the first frame's fades were bumped by start_fade() before the reset: settled now
        for b in 0..BANDS {
            for s in 0..2 {
                let f = &mut self.frame.bands[b].slots[s];
                f.fading = false;
                f.fade = 1.0;
                f.old = f.cur;
            }
        }
    }

    /// Test switches on both engines: the droop correction, the crossover tree (README §5).
    #[cfg(test)]
    pub(crate) fn set_test(&mut self, droop_on: bool, tree_off: bool) {
        for e in self.eng.iter_mut() {
            e.droop_on = droop_on;
            e.tree_off = tree_off;
        }
    }
}

impl Default for Grit {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Grit {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate;
        self.nch = channels.clamp(1, 2);
        for s in [&mut self.drive, &mut self.focus, &mut self.wet, &mut self.level] {
            s.prepare(sample_rate);
        }
        for s in self.xs.iter_mut().chain(self.gain.iter_mut()).chain(self.amt.iter_mut().flatten()) {
            s.prepare(sample_rate);
        }
        for s in self.mix.iter_mut().chain(self.peak.iter_mut().flatten()) {
            s.prepare(sample_rate);
        }
        for r in self.solo.iter_mut().chain(self.byp.iter_mut()).chain(self.fades.iter_mut().flatten()) {
            r.prepare(sample_rate);
        }
        self.bypass.prepare(sample_rate);
        self.preroll_len = (PREROLL_MS * 0.001 * sample_rate).round() as u32;
        self.fade_len = (SWITCH_FADE_MS * 0.001 * sample_rate).round().max(1.0) as u32;
        self.reset();
    }

    fn reset(&mut self) {
        self.clock = 0;
        self.set_targets();
        self.settle();
        self.active = 0;
        self.sw = Switch::Idle;
        let cfg = self.want_cfg();
        self.eng[0].configure(cfg, self.sr, &self.frame);
        self.eng[1].configure(cfg, self.sr, &self.frame);
    }

    fn set_param(&mut self, id: &str, value: f32) {
        if let Some(i) = PARAMS.iter().position(|p| p.id == id) {
            let p = &PARAMS[i];
            self.v[i] = value.clamp(p.min, p.max);
            self.set_targets();
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(self.nch);
        if nch == 0 {
            return;
        }
        let n = buffers[0].len();
        for i in 0..n {
            self.clock += 1;
            self.update_frame();
            let x = [buffers[0][i], if nch > 1 { buffers[1][i] } else { 0.0 }];
            // quality / slope: a second engine pre-rolls beside the first, then takes over
            if self.sw == Switch::Idle {
                let want = self.want_cfg();
                if want != self.eng[self.active].cfg {
                    let o = 1 - self.active;
                    let (a, b) = self.eng.split_at_mut(1);
                    let (from, to) = if o == 1 { (&a[0], &mut b[0]) } else { (&b[0], &mut a[0]) };
                    self.sw_power = want.order != from.cfg.order;
                    to.configure(want, self.sr, &self.frame);
                    to.copy_phases(from);
                    self.sw = Switch::Preroll(self.preroll_len);
                }
            }
            let mut y = self.eng[self.active].process(x, nch, &self.frame);
            match self.sw {
                Switch::Idle => {}
                Switch::Preroll(k) => {
                    let _ = self.eng[1 - self.active].process(x, nch, &self.frame);
                    self.sw = if k <= 1 { Switch::Fade(self.fade_len) } else { Switch::Preroll(k - 1) };
                }
                Switch::Fade(k) => {
                    let z = self.eng[1 - self.active].process(x, nch, &self.frame);
                    let r = 1.0 - (k - 1) as f64 / self.fade_len as f64;
                    // a slope change crossfades two dispersions (up to ~100° apart near a
                    // crossover): equal power; a quality change two near-identical signals: equal gain
                    let (wo, wn) = if self.sw_power {
                        let a = 0.5 * PI * r;
                        (a.cos() as f32, a.sin() as f32)
                    } else {
                        let w = raised_cos(r) as f32;
                        (1.0 - w, w)
                    };
                    for ch in 0..nch {
                        y[ch] = wo * y[ch] + wn * z[ch];
                    }
                    if k <= 1 {
                        self.active = 1 - self.active;
                        self.sw = Switch::Idle;
                    } else {
                        self.sw = Switch::Fade(k - 1);
                    }
                }
            }
            let ldb = self.level.next() as f64;
            if ldb != self.level_cache.0 {
                self.level_cache = (ldb, 10f64.powf(ldb / 20.0));
            }
            let lv = self.level_cache.1 as f32;
            let bp = self.bypass.next();
            for ch in 0..nch {
                let out = y[ch] * lv;
                buffers[ch][i] = if bp == 0.0 { out } else { bp * x[ch] + (1.0 - bp) * out };
            }
        }
    }
}
