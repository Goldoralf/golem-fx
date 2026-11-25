// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-ducker — golem's ducker (design: README.md): it lowers a signal while another one plays.
//!
//! - **Key detector** (gfx-dsp `sidechain`): Butterworth `hpf` / `lpf` on the key bus, linked
//!   max(|kL|, |kR|), a sliding maximum over W = clamp(1.2 / max(`hpf` / 2, 30 Hz), 8 ms, 40 ms)
//!   (gfx-transient's halved rule, the review's decision), read in dB. No key = −∞ = no duck.
//! - **Drivers** (gfx-dsp `duck`): Follow (a* from the level with `knee` and `hyst`, linear
//!   activation with attack / hold / release, duck = depth · S(a), S the raised cosine); Trigger
//!   (gfx-dsp `onset::Pick` fed with the sliding-maximum level fires a fixed dip: raised-cosine fall,
//!   hold, `tension`-shaped recovery in linear gain, its length in ms or a share of a note value);
//!   Pump (the same dip read from the host's position, or free-running). `force` ducks fully through
//!   the same attack / hold / release.
//! - **Gain stage** (`shape`): Wide (x · g per sample), Bell / Low / High (matched designs at the
//!   control tick, 1 / 1,500 s, coefficients ramped), Spectral (16 half-octave matched bells from
//!   56.6 Hz, Q 2, cuts from the solver, coefficients from tables). A filter whose cut is 0 is out of
//!   the path, so a released ducker is bit-transparent. `stereo` Mid / Side ducks M or S only, written
//!   as a difference (L' = L − (M − M')).
//!
//! Zero latency by default; `lookahead` 2 / 5 / 10 ms delays the main and is reported. `mode`,
//! `shape` and `stereo` crossfade over 20 ms. Real-time safe, deterministic, block-size independent.

#![allow(clippy::needless_range_loop)]

#[cfg(feature = "plugin")]
pub mod plugin;

#[cfg(test)]
mod tests;

use gfx_dsp::delay::DelayLine;
use gfx_dsp::duck::{self, dip_db, s_curve, Activation, Dip, DipShape, KeyGate, SpectralAnalysis, SpectralPlan, BANDS};
use gfx_dsp::envelope::SlidingMax;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::matched::{design, Analog};
use gfx_dsp::onset::{Pick, PickSettings};
use gfx_dsp::sidechain::{window_secs, ScFilter, MAX_WINDOW_S};
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{Svf, SvfCoefs};
use gfx_dsp::transport::{Sync, TransportInfo};
use gfx_dsp::{Effect, ParamInfo};

pub const NP: usize = 24;

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::enumeration("mode", "Mode", 3, 0),
    ParamInfo::float("thresh", "Threshold", -90.0, 0.0, -40.0),
    ParamInfo::float("depth", "Depth", 0.0, 60.0, 12.0).with_exp(),
    ParamInfo::float("attack", "Attack", 0.5, 500.0, 10.0),
    ParamInfo::float("hold", "Hold", 0.0, 2000.0, 150.0),
    ParamInfo::float("release", "Release", 10.0, 5000.0, 400.0),
    ParamInfo::enumeration("shape", "Band", 5, 0),
    ParamInfo::float("hpf", "Key Low Cut", 0.0, 2000.0, 0.0),
    ParamInfo::float("lpf", "Key High Cut", 100.0, 22000.0, 22000.0),
    ParamInfo::float("knee", "Knee", 0.0, 24.0, 6.0),
    ParamInfo::float("hyst", "Hysteresis", -24.0, 0.0, -3.0),
    ParamInfo::float("freq", "Band Frequency", 40.0, 12000.0, 1000.0).with_exp(),
    ParamInfo::float("width", "Band Width", 0.3, 4.0, 2.0),
    ParamInfo::float("spread", "Spectral Spread", 3.0, 30.0, 12.0),
    ParamInfo::enumeration("sync", "Sync", 17, 7),
    ParamInfo::float("length", "Dip Length", 10.0, 100.0, 75.0),
    ParamInfo::float("tension", "Recovery Tension", -100.0, 100.0, 0.0),
    ParamInfo::float("offset", "Beat Offset", 0.0, 100.0, 0.0),
    ParamInfo::float("sens", "Trigger Sensitivity", 0.0, 100.0, 50.0),
    ParamInfo::enumeration("stereo", "Stereo", 3, 0),
    ParamInfo::enumeration("lookahead", "Lookahead", 4, 0),
    ParamInfo::boolean("force", "Duck Now", false),
    ParamInfo::boolean("listen", "Listen", false),
    ParamInfo::boolean("bypass", "Bypass", false),
];

pub mod p {
    pub const MODE: usize = 0;
    pub const THRESH: usize = 1;
    pub const DEPTH: usize = 2;
    pub const ATTACK: usize = 3;
    pub const HOLD: usize = 4;
    pub const RELEASE: usize = 5;
    pub const SHAPE: usize = 6;
    pub const HPF: usize = 7;
    pub const LPF: usize = 8;
    pub const KNEE: usize = 9;
    pub const HYST: usize = 10;
    pub const FREQ: usize = 11;
    pub const WIDTH: usize = 12;
    pub const SPREAD: usize = 13;
    pub const SYNC: usize = 14;
    pub const LENGTH: usize = 15;
    pub const TENSION: usize = 16;
    pub const OFFSET: usize = 17;
    pub const SENS: usize = 18;
    pub const STEREO: usize = 19;
    pub const LOOKAHEAD: usize = 20;
    pub const FORCE: usize = 21;
    pub const LISTEN: usize = 22;
    pub const BYPASS: usize = 23;
}

/// `mode` option labels (enum order, final: append only).
pub const MODES: [&str; 3] = ["Follow", "Trigger", "Pump"];
/// `shape` option labels (enum order, final).
pub const SHAPES: [&str; 5] = ["Wide", "Bell", "Low", "High", "Spectral"];
/// `stereo` option labels (enum order, final).
pub const STEREO: [&str; 3] = ["Both", "Mid", "Side"];
/// `lookahead` options (enum order, final), in ms.
pub const LOOKAHEAD_MS: [f32; 4] = [0.0, 2.0, 5.0, 10.0];

const FOLLOW: usize = 0;
const TRIGGER: usize = 1;
const PUMP: usize = 2;
const WIDE: usize = 0;
const SPECTRAL: usize = 4;
const MAX_CH: usize = 2;
/// Lines of the gain stage: 0 / 1 = L / R (Both, or mono), 2 = Mid, 3 = Side.
const LINES: usize = 4;
/// Control ticks per second (§2.7): 32 samples at 48 kHz, 128 at 192 kHz.
const TICK_HZ: f32 = 1500.0;
/// Solver sweeps per tick (warm-started).
const SWEEPS: usize = 2;
/// Trigger's refractory time (ms) and its slope window's minimum (ms).
const REFRACTORY_MS: f32 = 40.0;
const SLOPE_MIN_MS: f32 = 3.0;
const DB_FLOOR: f64 = -120.0;

#[inline]
fn idx(v: f32, n: usize) -> usize {
    (v.round().max(0.0) as usize).min(n - 1)
}

/// The Bell's Q for a bandwidth of `oct` octaves: √(2^N) / (2^N − 1).
pub fn bell_q(oct: f64) -> f64 {
    let n = 2f64.powf(oct.clamp(0.3, 4.0));
    n.sqrt() / (n - 1.0)
}

/// The analog prototype of a band shape for a duck of `db` (positive).
pub fn band_proto(shape: usize, db: f64, width_oct: f64) -> Analog {
    match shape {
        1 => Analog::bell(bell_q(width_oct), -db),
        2 => Analog::low_shelf(0.7, -db),
        _ => Analog::high_shelf(0.7, -db),
    }
}

/// One dynamic filter of the gain stage: out of the path while its cut is 0.
#[derive(Clone, Debug, Default)]
struct BandFilt {
    svf: Svf,
    on: bool,
    cur: f64,
}

impl BandFilt {
    /// A control tick: ramp toward `ct` (the cut `target`); enter from reset state at `c0` (the
    /// 0 dB coefficients), leave once the ramp back to `c0` is over.
    #[inline]
    fn update(&mut self, target: f64, c0: &SvfCoefs, ct: &SvfCoefs, n: u32) {
        if target > 0.0 {
            if !self.on {
                self.svf.reset();
                self.svf.set_coefs(c0);
                self.on = true;
            }
            self.svf.set_coefs_ramped(ct, n);
            self.cur = target;
        } else if self.on {
            if self.cur > 0.0 {
                self.svf.set_coefs_ramped(c0, n);
                self.cur = 0.0;
            } else {
                self.on = false;
            }
        }
    }

    fn off(&mut self) {
        self.on = false;
        self.cur = 0.0;
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        if self.on {
            self.svf.process(x)
        } else {
            x
        }
    }
}

/// The last designs of a band shape (Bell, Low, High) and what they were made for.
#[derive(Clone, Copy, Debug)]
struct BandCache {
    f0: f64,
    width: f64,
    duck: f64,
    c0: SvfCoefs,
    ct: SvfCoefs,
}

impl Default for BandCache {
    fn default() -> Self {
        Self { f0: f64::NAN, width: f64::NAN, duck: f64::NAN, c0: SvfCoefs::default(), ct: SvfCoefs::default() }
    }
}

#[derive(Clone, Debug, Default)]
struct Line {
    /// Bell, Low, High
    band: [BandFilt; 3],
    spec: [BandFilt; BANDS],
}

pub struct Ducker {
    v: [f32; NP],
    sr: f32,
    ctl: u32,
    clock: u64,
    // control-rate parameters (20 ms)
    thresh: OnePole,
    knee: OnePole,
    hyst: OnePole,
    width: OnePole,
    spread: OnePole,
    length: OnePole,
    tension: OnePole,
    hpf: OnePole,
    lpf: OnePole,
    // EXP parameters (the contract's smoother); `freq` in ln Hz
    depth: ExpSmoother,
    freq: ExpSmoother,
    // per-tick values
    att_s: f64,
    hold_s: f64,
    rel_s: f64,
    // key detection
    sc: [ScFilter; MAX_CH],
    sc_set: (f32, f32),
    smax: SlidingMax,
    level_db: f64,
    gate: KeyGate,
    follow: Activation,
    force_act: Activation,
    pick: Pick,
    pick_set: PickSettings,
    dip: Dip,
    // Pump
    transport: TransportInfo,
    free_beats: f64,
    free_samples: f64,
    // Spectral
    analysis: SpectralAnalysis,
    plan: SpectralPlan,
    spec_live: bool,
    w_target: [f64; BANDS],
    w: [f64; BANDS],
    g: [f64; BANDS],
    // gain stage
    lines: [Line; LINES],
    band_cache: [BandCache; 3],
    delay: [DelayLine; MAX_CH],
    nch: usize,
    mode_cur: usize,
    mode_prev: usize,
    mode_fade: LinearRamp,
    shape_cur: usize,
    shape_prev: usize,
    shape_fade: LinearRamp,
    st_cur: usize,
    st_prev: usize,
    st_fade: LinearRamp,
    la_cur: usize,
    la_prev: usize,
    la_fade: LinearRamp,
    listen_g: LinearRamp,
    bypass_g: LinearRamp,
    // the duck now (dB, ≥ 0), meters
    duck: f64,
    gr: f32,
    /// Trigger's onsets since reset (test witness)
    fired: u64,
}

/// `OnePole` advanced by a whole tick.
fn tick(s: &mut OnePole, n: u32) -> f32 {
    if !s.is_settled() {
        for _ in 0..n {
            s.next();
        }
    }
    s.value()
}

impl Ducker {
    pub fn new() -> Self {
        let v: [f32; NP] = core::array::from_fn(|i| PARAMS[i].default);
        Self {
            v,
            sr: 48000.0,
            ctl: 32,
            clock: 0,
            thresh: OnePole::new(20.0, v[p::THRESH], 1e-4),
            knee: OnePole::new(20.0, v[p::KNEE], 1e-5),
            hyst: OnePole::new(20.0, v[p::HYST], 1e-5),
            width: OnePole::new(20.0, v[p::WIDTH], 1e-6),
            spread: OnePole::new(20.0, v[p::SPREAD], 1e-5),
            length: OnePole::new(20.0, v[p::LENGTH], 1e-5),
            tension: OnePole::new(20.0, v[p::TENSION], 1e-5),
            hpf: OnePole::new(20.0, v[p::HPF], 1e-3),
            lpf: OnePole::new(20.0, v[p::LPF], 1e-2),
            depth: ExpSmoother::new(0.0, 60.0, v[p::DEPTH]),
            freq: ExpSmoother::new(40f32.ln(), 12000f32.ln(), v[p::FREQ].ln()),
            att_s: 0.01,
            hold_s: 0.15,
            rel_s: 0.4,
            sc: Default::default(),
            sc_set: (f32::NAN, f32::NAN),
            smax: SlidingMax::new(),
            level_db: DB_FLOOR,
            gate: KeyGate::default(),
            follow: Activation::default(),
            force_act: Activation::default(),
            pick: Pick::default(),
            pick_set: PickSettings::default(),
            dip: Dip::default(),
            transport: TransportInfo::default(),
            free_beats: 0.0,
            free_samples: 0.0,
            analysis: SpectralAnalysis::default(),
            plan: SpectralPlan::default(),
            spec_live: false,
            w_target: [1.0; BANDS],
            w: [1.0; BANDS],
            g: [0.0; BANDS],
            lines: Default::default(),
            band_cache: Default::default(),
            delay: Default::default(),
            nch: 1,
            mode_cur: 0,
            mode_prev: 0,
            mode_fade: LinearRamp::new(20.0, 1.0),
            shape_cur: 0,
            shape_prev: 0,
            shape_fade: LinearRamp::new(20.0, 1.0),
            st_cur: 0,
            st_prev: 0,
            st_fade: LinearRamp::new(20.0, 1.0),
            la_cur: 0,
            la_prev: 0,
            la_fade: LinearRamp::new(20.0, 1.0),
            listen_g: LinearRamp::new(5.0, 0.0),
            bypass_g: LinearRamp::new(10.0, 1.0),
            duck: 0.0,
            gr: 0.0,
            fired: 0,
        }
    }

    /// Trigger's onsets since reset (test witness).
    pub fn onsets(&self) -> u64 {
        self.fired
    }

    /// `gr` meter: the current duck (dB, ≥ 0; in band shapes the deepest target).
    pub fn gr_db(&self) -> f32 {
        self.gr
    }

    /// `state` meter: ducking (`gr` > 1 dB).
    pub fn is_ducking(&self) -> bool {
        self.gr > 1.0
    }

    /// The duck applied at the last sample (dB, test witness).
    pub fn duck_db(&self) -> f64 {
        self.duck
    }

    /// The key level at the last sample (dB, test witness).
    pub fn key_level_db(&self) -> f64 {
        self.level_db
    }

    /// The Spectral cuts the solver gave at the last tick (dB, test witness).
    pub fn spectral_cuts(&self) -> [f64; BANDS] {
        self.g
    }

    fn la_samples(&self, i: usize) -> usize {
        (LOOKAHEAD_MS[i] * 0.001 * self.sr).round() as usize
    }

    fn fading(r: &LinearRamp) -> bool {
        !r.is_settled() || r.value() < 1.0
    }

    fn shape_active(&self, s: usize) -> bool {
        self.shape_cur == s || (Self::fading(&self.shape_fade) && self.shape_prev == s)
    }

    fn mode_active(&self, m: usize) -> bool {
        self.mode_cur == m || (Self::fading(&self.mode_fade) && self.mode_prev == m)
    }

    /// The lines the stage reads now.
    fn line_active(&self, line: usize) -> bool {
        if self.nch < 2 {
            return line == 0;
        }
        let of = |st: usize| match st {
            1 => line == 2,
            2 => line == 3,
            _ => line < 2,
        };
        of(self.st_cur) || (Self::fading(&self.st_fade) && of(self.st_prev))
    }

    /// The dip's segments in samples for Trigger (and Pump's shape inside one period).
    fn dip_shape(&self, length: f64) -> DipShape {
        let sr = self.sr as f64;
        DipShape::new(self.att_s * sr, self.hold_s * sr, length)
    }

    /// The dip's length in samples: `length` % of the note value with `sync` on, else
    /// attack + hold + release.
    fn dip_length(&self) -> f64 {
        let sr = self.sr as f64;
        match Sync::from_index(idx(self.v[p::SYNC], 17)).beats() {
            Some(b) => self.transport.note_samples(b, self.sr) * (self.length.value() as f64 * 0.01).clamp(0.1, 1.0),
            None => (self.att_s + self.hold_s + self.rel_s) * sr,
        }
    }

    fn control(&mut self) {
        let n = self.ctl;
        let sr = self.sr as f64;
        for s in [&mut self.thresh, &mut self.knee, &mut self.hyst, &mut self.width, &mut self.spread, &mut self.length, &mut self.tension] {
            tick(s, n);
        }
        let hpf = tick(&mut self.hpf, n).clamp(0.0, 2000.0);
        let lpf = tick(&mut self.lpf, n).clamp(100.0, 22000.0);
        if !self.freq.is_settled() {
            for _ in 0..n {
                self.freq.next();
            }
        }
        // times: band shapes move at the tick, so their fastest edge is two ticks
        let band = (1..=SPECTRAL).any(|s| self.shape_active(s));
        let min_att = if band { 2.0 * n as f64 / sr } else { 0.0005 };
        self.att_s = (self.v[p::ATTACK].clamp(0.5, 500.0) as f64 * 0.001).max(min_att);
        self.hold_s = self.v[p::HOLD].clamp(0.0, 2000.0) as f64 * 0.001;
        self.rel_s = self.v[p::RELEASE].clamp(10.0, 5000.0) as f64 * 0.001;
        self.follow.set_times(self.att_s, self.hold_s, self.rel_s, sr);
        self.force_act.set_times(self.att_s, self.hold_s, self.rel_s, sr);
        // key detector
        if (hpf, lpf) != self.sc_set {
            self.sc_set = (hpf, lpf);
            for s in self.sc.iter_mut() {
                s.set(hpf, lpf, self.sr);
            }
            self.smax.set_window((window_secs(hpf, 2.0) * self.sr) as usize);
        }
        let ps = PickSettings {
            rise_db: Pick::rise_for(self.v[p::SENS].clamp(0.0, 100.0) * 0.01),
            slope_ms: (500.0 / lpf).max(SLOPE_MIN_MS),
            hold_ms: 0.0,
            refractory_ms: REFRACTORY_MS,
        };
        if ps != self.pick_set {
            self.pick_set = ps;
            self.pick.set(ps);
        }
        self.control_bands();
        self.control_spectral();
    }

    /// Bell, Low, High: one live matched design per active shape and tick, shared by the lines.
    fn control_bands(&mut self) {
        let n = self.ctl;
        let sr = self.sr as f64;
        let duck = self.duck;
        let f0 = (self.freq.value().exp() as f64).clamp(40.0, 12000.0).min(0.45 * sr);
        let width = self.width.value() as f64;
        let active_lines: [bool; LINES] = core::array::from_fn(|l| self.line_active(l));
        for s in 1..=3 {
            let live = self.shape_active(s);
            let busy = self.lines.iter().any(|l| l.band[s - 1].on);
            if !live {
                if busy {
                    for l in self.lines.iter_mut() {
                        l.band[s - 1].off();
                    }
                }
                continue;
            }
            if duck <= 0.0 && !busy {
                continue;
            }
            // designs are cached: the 0 dB one changes with `freq` and `width` only, the cut also
            // stays put while the duck holds at its depth
            let cache = &mut self.band_cache[s - 1];
            if cache.f0 != f0 || cache.width != width {
                cache.f0 = f0;
                cache.width = width;
                cache.c0 = SvfCoefs::from_biquad(&design(band_proto(s, 0.0, width), f0, sr)).unwrap_or_default();
                cache.duck = f64::NAN;
            }
            if cache.duck != duck {
                cache.duck = duck;
                cache.ct = if duck > 0.0 { SvfCoefs::from_biquad(&design(band_proto(s, duck, width), f0, sr)).unwrap_or(cache.c0) } else { cache.c0 };
            }
            let (c0, ct) = (cache.c0, cache.ct);
            for (l, line) in self.lines.iter_mut().enumerate() {
                if active_lines[l] {
                    line.band[s - 1].update(duck, &c0, &ct, n);
                } else {
                    line.band[s - 1].off();
                }
            }
        }
    }

    /// Spectral: weights from the key's bands (moved at the activation's rates), targets, the
    /// solver, then the bells from the tables.
    fn control_spectral(&mut self) {
        let live = self.shape_active(SPECTRAL);
        if !live {
            if self.spec_live {
                self.spec_live = false;
                for l in self.lines.iter_mut() {
                    l.spec.iter_mut().for_each(BandFilt::off);
                }
                self.g = [0.0; BANDS];
            }
            return;
        }
        if !self.spec_live {
            self.spec_live = true;
            self.analysis.reset();
        }
        let n = self.ctl as f64;
        let sr = self.sr as f64;
        // the spectrum decides where only while the key is there: between words the weights hold
        if self.level_db > (self.thresh.value() + self.hyst.value().min(0.0)) as f64 {
            let mut lv = [0.0; BANDS];
            self.analysis.levels_db(&mut lv);
            duck::weights(&lv, self.spread.value() as f64, &mut self.w_target);
        }
        let up = n / (self.att_s * sr).max(1.0);
        let down = n / (self.rel_s * sr).max(1.0);
        let duck = self.duck;
        let mut t = [0.0; BANDS];
        for k in 0..BANDS {
            let (w, wt) = (self.w[k], self.w_target[k]);
            self.w[k] = if wt > w { (w + up).min(wt) } else { (w - down).max(wt) };
            t[k] = duck * self.w[k];
        }
        self.plan.solve(&t, &mut self.g, SWEEPS);
        let active_lines: [bool; LINES] = core::array::from_fn(|l| self.line_active(l));
        let ctl = self.ctl;
        for k in 0..BANDS {
            let gk = self.g[k];
            let busy = self.lines.iter().any(|l| l.spec[k].on);
            if gk <= 0.0 && !busy {
                continue;
            }
            let c0 = self.plan.coefs(k, 0.0);
            let ct = if gk > 0.0 { self.plan.coefs(k, gk) } else { c0 };
            for (l, line) in self.lines.iter_mut().enumerate() {
                if active_lines[l] {
                    line.spec[k].update(gk, &c0, &ct, ctl);
                } else {
                    line.spec[k].off();
                }
            }
        }
    }

    /// One line through one shape.
    #[inline]
    fn line_out(&mut self, line: usize, shape: usize, x: f32, wide_g: f32) -> f32 {
        match shape {
            WIDE => {
                if wide_g == 1.0 {
                    x
                } else {
                    x * wide_g
                }
            }
            1..=3 => self.lines[line].band[shape - 1].process(x),
            _ => {
                let mut y = x;
                for f in self.lines[line].spec.iter_mut() {
                    y = f.process(y);
                }
                y
            }
        }
    }

    /// The stage for one stereo setting and one shape: the output frame.
    #[inline]
    fn stage(&mut self, st: usize, shape: usize, x: &[f32; MAX_CH], wide_g: f32) -> [f32; MAX_CH] {
        if self.nch < 2 {
            return [self.line_out(0, shape, x[0], wide_g), 0.0];
        }
        match st {
            1 => {
                let m = 0.5 * (x[0] + x[1]);
                let d = m - self.line_out(2, shape, m, wide_g);
                [x[0] - d, x[1] - d]
            }
            2 => {
                let s = 0.5 * (x[0] - x[1]);
                let d = s - self.line_out(3, shape, s, wide_g);
                [x[0] - d, x[1] + d]
            }
            _ => [self.line_out(0, shape, x[0], wide_g), self.line_out(1, shape, x[1], wide_g)],
        }
    }

    #[inline]
    fn stage_shapes(&mut self, st: usize, x: &[f32; MAX_CH], wide_g: f32, sf: f32) -> [f32; MAX_CH] {
        let a = self.stage(st, self.shape_cur, x, wide_g);
        if sf >= 1.0 {
            return a;
        }
        let b = self.stage(st, self.shape_prev, x, wide_g);
        [b[0] + (a[0] - b[0]) * sf, b[1] + (a[1] - b[1]) * sf]
    }

    /// Pump's duck at sample `i` of the block (dB).
    fn pump_db(&self, i: usize, depth: f64, tension: f64) -> f64 {
        let sr = self.sr as f64;
        let offset = (self.v[p::OFFSET].clamp(0.0, 100.0) as f64) * 0.01;
        match Sync::from_index(idx(self.v[p::SYNC], 17)).beats() {
            Some(pb) => {
                let spb = self.transport.samples_per_beat(self.sr);
                let beats = match (self.transport.playing, self.transport.pos_beats) {
                    // the gain of an output sample belongs to the input `lookahead` samples earlier
                    (true, Some(pos)) => pos + (i as f64 - self.la_cur as f64) / spb,
                    _ => self.free_beats,
                };
                let period = pb * spb;
                let phase = (beats / pb - offset).rem_euclid(1.0);
                let shape = self.dip_shape(period * (self.length.value() as f64 * 0.01).clamp(0.1, 1.0));
                dip_db(phase * period, 0.0, &shape, depth, tension).unwrap_or(0.0)
            }
            None => {
                let period = ((self.att_s + self.hold_s + self.rel_s) * sr).max(2.0);
                let t = (self.free_samples - offset * period).rem_euclid(period);
                let shape = self.dip_shape(period);
                dip_db(t, 0.0, &shape, depth, tension).unwrap_or(0.0)
            }
        }
    }

    pub fn process_with_key(&mut self, buffers: &mut [&mut [f32]], key: Option<&[&[f32]]>) {
        let nch = buffers.len().min(MAX_CH);
        let n = buffers.first().map_or(0, |b| b.len());
        if nch == 0 {
            return;
        }
        self.nch = nch;
        let v = self.v;
        let key = key.filter(|k| !k.is_empty());
        let nk = key.map_or(0, |k| k.len().min(MAX_CH));
        self.listen_g.set_target(if v[p::LISTEN] >= 0.5 { 1.0 } else { 0.0 });
        self.bypass_g.set_target(if v[p::BYPASS] >= 0.5 { 0.0 } else { 1.0 });
        let force = if v[p::FORCE] >= 0.5 { 1.0 } else { 0.0 };
        let spb = self.transport.samples_per_beat(self.sr);
        let mut gr = 0.0f64;

        for i in 0..n {
            if self.clock.is_multiple_of(self.ctl as u64) {
                self.control();
            }
            self.clock += 1;
            let depth = (self.depth.next() as f64).clamp(0.0, 60.0);
            let tension = self.tension.value() as f64;

            // the key: filtered, linked, sliding maximum, dB
            let mut kf = [0.0f32; MAX_CH];
            let mut a = 0.0f32;
            for c in 0..nk {
                let s = key.map_or(0.0, |k| k[c].get(i).copied().unwrap_or(0.0));
                let s = if s.is_finite() { s } else { 0.0 };
                kf[c] = self.sc[c].process(s);
                a = a.max(kf[c].abs());
            }
            if nk == 1 {
                kf[1] = kf[0];
            }
            let l = self.smax.next(a) as f64;
            self.level_db = if l > 1e-6 { 20.0 * l.log10() } else { DB_FLOOR };
            let thresh = self.thresh.value() as f64;

            // the drivers
            let a_star = self.gate.target(self.level_db, thresh, self.knee.value() as f64, self.hyst.value() as f64);
            let follow_db = depth * s_curve(self.follow.next(a_star));
            // Trigger's detector runs while Trigger is in use
            if self.mode_active(TRIGGER) && self.pick.detect(self.level_db as f32, self.level_db > thresh) {
                let shape = self.dip_shape(self.dip_length());
                self.dip.fire(shape, depth);
                self.fired += 1;
            }
            let trig_db = self.dip.next(depth, tension);
            let pump = self.mode_active(PUMP);
            let pump_db = if pump { self.pump_db(i, depth, tension) } else { 0.0 };
            self.free_beats += 1.0 / spb;
            self.free_samples += 1.0;
            let drv = |m: usize| match m {
                FOLLOW => follow_db,
                TRIGGER => trig_db,
                _ => pump_db,
            };
            let mf = self.mode_fade.next() as f64;
            let mode_db = if mf >= 1.0 { drv(self.mode_cur) } else { drv(self.mode_prev) + (drv(self.mode_cur) - drv(self.mode_prev)) * mf };
            let force_db = depth * s_curve(self.force_act.next(force));
            let duck = mode_db.max(force_db);
            self.duck = duck;
            gr = gr.max(duck);
            if self.spec_live {
                let m = if nk == 2 { 0.5 * (kf[0] + kf[1]) } else { kf[0] };
                self.analysis.process(m);
            }

            // the main, delayed by the lookahead
            let xf = self.la_fade.next();
            let mut x = [0.0f32; MAX_CH];
            for c in 0..nch {
                let s = buffers[c][i];
                let d = &mut self.delay[c];
                d.push(if s.is_finite() { s } else { 0.0 });
                x[c] = if xf >= 1.0 { d.tap(self.la_cur) } else { d.tap(self.la_prev) + (d.tap(self.la_cur) - d.tap(self.la_prev)) * xf };
            }
            let wide_g = if duck <= 0.0 { 1.0 } else { (-duck as f32 * (std::f32::consts::LN_10 / 20.0)).exp() };
            let sf = self.shape_fade.next();
            let stf = self.st_fade.next();
            let mut y = self.stage_shapes(self.st_cur, &x, wide_g, sf);
            if stf < 1.0 {
                let yp = self.stage_shapes(self.st_prev, &x, wide_g, sf);
                y = [yp[0] + (y[0] - yp[0]) * stf, yp[1] + (y[1] - yp[1]) * stf];
            }
            let lg = self.listen_g.next();
            let b = self.bypass_g.next();
            for c in 0..nch {
                let mut out = y[c];
                if lg > 0.0 {
                    out = if lg >= 1.0 { kf[c] } else { out + (kf[c] - out) * lg };
                }
                if b < 1.0 {
                    // bypass: the delayed main (the latency stays)
                    out = x[c] + (out - x[c]) * b;
                }
                buffers[c][i] = out;
            }
        }
        self.gr = gr.clamp(0.0, 60.0) as f32;
    }
}

impl Default for Ducker {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Ducker {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate;
        self.ctl = (sample_rate / TICK_HZ).round().max(1.0) as u32;
        self.nch = channels.clamp(1, MAX_CH);
        for s in [
            &mut self.thresh,
            &mut self.knee,
            &mut self.hyst,
            &mut self.width,
            &mut self.spread,
            &mut self.length,
            &mut self.tension,
            &mut self.hpf,
            &mut self.lpf,
        ] {
            s.prepare(sample_rate);
        }
        self.depth.prepare(sample_rate);
        self.freq.prepare(sample_rate);
        self.smax.prepare((MAX_WINDOW_S * sample_rate) as usize + 2);
        self.pick.prepare(sample_rate);
        self.pick.set(self.pick_set);
        self.analysis.prepare(sample_rate);
        self.plan.prepare(sample_rate);
        self.band_cache = Default::default();
        let max_la = (LOOKAHEAD_MS[3] * 0.001 * sample_rate).ceil() as usize + 2;
        for d in self.delay.iter_mut() {
            d.prepare(max_la);
        }
        for r in [&mut self.mode_fade, &mut self.shape_fade, &mut self.st_fade, &mut self.la_fade, &mut self.listen_g, &mut self.bypass_g] {
            r.prepare(sample_rate);
        }
        self.la_cur = self.la_samples(idx(self.v[p::LOOKAHEAD], 4));
        self.la_prev = self.la_cur;
    }

    fn reset(&mut self) {
        let v = self.v;
        for (s, i) in [
            (&mut self.thresh, p::THRESH),
            (&mut self.knee, p::KNEE),
            (&mut self.hyst, p::HYST),
            (&mut self.width, p::WIDTH),
            (&mut self.spread, p::SPREAD),
            (&mut self.length, p::LENGTH),
            (&mut self.tension, p::TENSION),
            (&mut self.hpf, p::HPF),
            (&mut self.lpf, p::LPF),
        ] {
            s.set_immediate(v[i].clamp(PARAMS[i].min, PARAMS[i].max));
        }
        self.depth.settle(v[p::DEPTH].clamp(0.0, 60.0));
        self.depth.reset();
        self.freq.settle(v[p::FREQ].clamp(40.0, 12000.0).ln());
        self.freq.reset();
        for s in self.sc.iter_mut() {
            s.reset();
        }
        self.sc_set = (f32::NAN, f32::NAN);
        self.smax.reset();
        self.level_db = DB_FLOOR;
        self.gate.reset();
        self.follow.reset();
        self.force_act.reset();
        self.pick.reset();
        self.dip.reset();
        self.free_beats = 0.0;
        self.free_samples = 0.0;
        self.analysis.reset();
        self.spec_live = false;
        self.w_target = [1.0; BANDS];
        self.w = [1.0; BANDS];
        self.g = [0.0; BANDS];
        for l in self.lines.iter_mut() {
            for f in l.band.iter_mut().chain(l.spec.iter_mut()) {
                f.off();
                f.svf.reset();
            }
        }
        for d in self.delay.iter_mut() {
            d.reset();
        }
        self.mode_cur = idx(v[p::MODE], 3);
        self.mode_prev = self.mode_cur;
        self.mode_fade.set_immediate(1.0);
        self.shape_cur = idx(v[p::SHAPE], 5);
        self.shape_prev = self.shape_cur;
        self.shape_fade.set_immediate(1.0);
        self.st_cur = idx(v[p::STEREO], 3);
        self.st_prev = self.st_cur;
        self.st_fade.set_immediate(1.0);
        self.la_cur = self.la_samples(idx(v[p::LOOKAHEAD], 4));
        self.la_prev = self.la_cur;
        self.la_fade.set_immediate(1.0);
        self.listen_g.set_immediate(if v[p::LISTEN] >= 0.5 { 1.0 } else { 0.0 });
        self.bypass_g.set_immediate(if v[p::BYPASS] >= 0.5 { 0.0 } else { 1.0 });
        self.clock = 0;
        self.duck = 0.0;
        self.gr = 0.0;
        self.fired = 0;
    }

    fn set_param(&mut self, id: &str, value: f32) {
        let Some(i) = PARAMS.iter().position(|pi| pi.id == id) else {
            return;
        };
        let old = self.v[i];
        self.v[i] = value;
        let pi = &PARAMS[i];
        let x = value.clamp(pi.min, pi.max);
        // a structural change crossfades from the structure in use (the nearer end of a running fade)
        let swap = |cur: &mut usize, prev: &mut usize, fade: &mut LinearRamp, new: usize| {
            if new != *cur {
                *prev = if fade.is_settled() || fade.value() >= 0.5 { *cur } else { *prev };
                *cur = new;
                fade.set_immediate(0.0);
                fade.set_target(1.0);
            }
        };
        match i {
            p::DEPTH if value != old => self.depth.set_target(x, self.clock),
            p::FREQ if value != old => self.freq.set_target(x.ln(), self.clock),
            p::THRESH => self.thresh.set_target(x),
            p::KNEE => self.knee.set_target(x),
            p::HYST => self.hyst.set_target(x),
            p::WIDTH => self.width.set_target(x),
            p::SPREAD => self.spread.set_target(x),
            p::LENGTH => self.length.set_target(x),
            p::TENSION => self.tension.set_target(x),
            p::HPF => self.hpf.set_target(x),
            p::LPF => self.lpf.set_target(x),
            p::MODE => swap(&mut self.mode_cur, &mut self.mode_prev, &mut self.mode_fade, idx(value, 3)),
            p::SHAPE => swap(&mut self.shape_cur, &mut self.shape_prev, &mut self.shape_fade, idx(value, 5)),
            p::STEREO => swap(&mut self.st_cur, &mut self.st_prev, &mut self.st_fade, idx(value, 3)),
            p::LOOKAHEAD => {
                let la = self.la_samples(idx(value, 4));
                if la != self.la_cur {
                    // a setup change: the host re-aligns; the audio crossfades over 20 ms
                    self.la_prev = if self.la_fade.is_settled() || self.la_fade.value() >= 0.5 { self.la_cur } else { self.la_prev };
                    self.la_cur = la;
                    self.la_fade.set_immediate(0.0);
                    self.la_fade.set_target(1.0);
                }
            }
            _ => {}
        }
    }

    fn set_transport(&mut self, transport: TransportInfo) {
        self.transport = transport;
    }

    fn latency(&self) -> usize {
        self.la_cur
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_with_key(buffers, None);
    }

    fn process_with_key(&mut self, buffers: &mut [&mut [f32]], key: Option<&[&[f32]]>) {
        Ducker::process_with_key(self, buffers, key);
    }
}
