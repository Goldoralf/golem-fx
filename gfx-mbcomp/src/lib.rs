// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-mbcomp — golem's multiband compressor (design: README.md).
//!
//! A new golem block (`mb-comp`): 3 or 4 bands, each with downward compression and, below it, Lift
//! (upward compression or downward expansion) with a noise guard.
//!
//! - **Audio**: gfx-dsp `crossover::Tree`, LR4 or LR8 with compensation all-passes. Every band is a
//!   real weight times one all-pass, so any band gains change the magnitude only: the phase never
//!   moves with the gains (§2.2). `phase` Linear puts the tree's `Linearizer` ahead of it (one FIR
//!   for all bands, reported latency 2,048 / 4,096 / 8,192 samples per rate family).
//! - **Detection**: a separate power-complementary Butterworth-4 tree (`crossover::BwTree`), each
//!   band's (or the whole input, `<b>_sc` Wide) into gfx-comp's engine through its gain API
//!   (`next_gain_db`: no audio through it, so no colour), its detector sized for the band's lowest
//!   content (`detector_low_hz`: sliding maximum over 1.2 periods on both readings). In Linear the
//!   detection reads the input delayed by L, so the gains are Minimum's, L samples later.
//! - **Lift** (new here): a feed-forward slope below `<b>_lift_thresh` on the larger of the band's
//!   two readings, a boost capped by `lift_range` and by the signal's height above `guard`
//!   (gfx-transient's floor bound), or a cut capped by `range`. Its own ballistics, the band's times.
//! - **Face**: Amount scales every slope (ratio held at 30:1), Tilt shifts the thresholds of the outer
//!   bands by ±12 dB, Time scales the times, auto gain is a static make-up per band at a reference
//!   level (−24 dBFS, −18 for a Wide band).
//! - `bands` / `slope` pre-roll a second audio engine for 30 ms, then crossfade over 10 ms; `phase`
//!   fades out over 10 ms, changes the latency, and fades in (a set-up switch).
//!
//! Real-time safe (everything sized in `prepare`), deterministic, block-size independent.

#![allow(clippy::needless_range_loop)]

#[cfg(feature = "plugin")]
pub mod plugin;

#[cfg(test)]
mod tests;

use gfx_comp::{static_curve, Character, Comp, Settings as CompSettings};
use gfx_dsp::crossover::{linear_latency, BwTree, Linearizer, Order, Tree, MAX_BANDS};
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::util::one_pole_coef;
use gfx_dsp::{Effect, ParamInfo};
use std::f64::consts::PI;

pub const NP: usize = 54;
/// Band slots (lo, lm, hm, hi); a 3-band setting leaves `hm` out.
pub const BANDS: usize = 4;

pub static PARAMS: [ParamInfo; NP] = [
    // the face
    ParamInfo::float("amount", "Amount", 0.0, 200.0, 100.0).with_exp(),
    ParamInfo::float("tilt", "Tilt", -100.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("time", "Time", 10.0, 400.0, 100.0),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::float("level", "Level", -24.0, 12.0, 0.0).with_exp(),
    ParamInfo::enumeration("character", "Character", 5, 0),
    // the bands: thresh, ratio, lift_thresh, lift, attack, release, gain, sc
    ParamInfo::float("lo_thresh", "Low Threshold", -60.0, 0.0, -24.0),
    ParamInfo::float("lo_ratio", "Low Ratio", 1.0, 30.0, 2.5),
    ParamInfo::float("lo_lift_thresh", "Low Lift Threshold", -80.0, 0.0, -48.0),
    ParamInfo::float("lo_lift", "Low Lift", -100.0, 100.0, 0.0),
    ParamInfo::float("lo_attack", "Low Attack", 0.1, 300.0, 30.0),
    ParamInfo::float("lo_release", "Low Release", 10.0, 3000.0, 250.0),
    ParamInfo::float("lo_gain", "Low Gain", -24.0, 24.0, 0.0),
    ParamInfo::enumeration("lo_sc", "Low Sidechain", 2, 0),
    ParamInfo::float("lm_thresh", "Low Mid Threshold", -60.0, 0.0, -24.0),
    ParamInfo::float("lm_ratio", "Low Mid Ratio", 1.0, 30.0, 2.5),
    ParamInfo::float("lm_lift_thresh", "Low Mid Lift Threshold", -80.0, 0.0, -48.0),
    ParamInfo::float("lm_lift", "Low Mid Lift", -100.0, 100.0, 0.0),
    ParamInfo::float("lm_attack", "Low Mid Attack", 0.1, 300.0, 20.0),
    ParamInfo::float("lm_release", "Low Mid Release", 10.0, 3000.0, 180.0),
    ParamInfo::float("lm_gain", "Low Mid Gain", -24.0, 24.0, 0.0),
    ParamInfo::enumeration("lm_sc", "Low Mid Sidechain", 2, 0),
    ParamInfo::float("hm_thresh", "High Mid Threshold", -60.0, 0.0, -24.0),
    ParamInfo::float("hm_ratio", "High Mid Ratio", 1.0, 30.0, 2.5),
    ParamInfo::float("hm_lift_thresh", "High Mid Lift Threshold", -80.0, 0.0, -48.0),
    ParamInfo::float("hm_lift", "High Mid Lift", -100.0, 100.0, 0.0),
    ParamInfo::float("hm_attack", "High Mid Attack", 0.1, 300.0, 12.0),
    ParamInfo::float("hm_release", "High Mid Release", 10.0, 3000.0, 150.0),
    ParamInfo::float("hm_gain", "High Mid Gain", -24.0, 24.0, 0.0),
    ParamInfo::enumeration("hm_sc", "High Mid Sidechain", 2, 0),
    ParamInfo::float("hi_thresh", "High Threshold", -60.0, 0.0, -24.0),
    ParamInfo::float("hi_ratio", "High Ratio", 1.0, 30.0, 2.5),
    ParamInfo::float("hi_lift_thresh", "High Lift Threshold", -80.0, 0.0, -48.0),
    ParamInfo::float("hi_lift", "High Lift", -100.0, 100.0, 0.0),
    ParamInfo::float("hi_attack", "High Attack", 0.1, 300.0, 8.0),
    ParamInfo::float("hi_release", "High Release", 10.0, 3000.0, 120.0),
    ParamInfo::float("hi_gain", "High Gain", -24.0, 24.0, 0.0),
    ParamInfo::enumeration("hi_sc", "High Sidechain", 2, 0),
    // expert
    ParamInfo::enumeration("bands", "Bands", 2, 0),
    ParamInfo::float("x_lo", "Low Crossover", 40.0, 500.0, 120.0),
    ParamInfo::float("x_mid", "Mid Crossover", 150.0, 3000.0, 600.0),
    ParamInfo::float("x_hi", "High Crossover", 800.0, 12000.0, 2500.0),
    ParamInfo::enumeration("slope", "Crossover Slope", 2, 0),
    ParamInfo::enumeration("phase", "Phase", 2, 0),
    ParamInfo::boolean("auto_gain", "Auto Gain", true),
    ParamInfo::boolean("auto_release", "Auto Release", true),
    ParamInfo::float("knee", "Knee", 0.0, 24.0, 6.0),
    ParamInfo::float("range", "Range", 0.0, 60.0, 24.0),
    ParamInfo::float("lift_range", "Lift Range", 0.0, 24.0, 12.0),
    ParamInfo::float("guard", "Noise Guard", -96.0, -36.0, -72.0),
    ParamInfo::float("link", "Stereo Link", 0.0, 100.0, 100.0),
    ParamInfo::enumeration("solo", "Solo", 5, 0),
    ParamInfo::boolean("delta", "Delta", false),
    ParamInfo::boolean("bypass", "Bypass", false),
];

/// Parameter indices.
pub mod p {
    pub const AMOUNT: usize = 0;
    pub const TILT: usize = 1;
    pub const TIME: usize = 2;
    pub const MIX: usize = 3;
    pub const LEVEL: usize = 4;
    pub const CHARACTER: usize = 5;
    /// Band b's block starts at 6 + 8 b: thresh, ratio, lift_thresh, lift, attack, release, gain, sc.
    pub const fn thresh(b: usize) -> usize {
        6 + 8 * b
    }
    pub const fn ratio(b: usize) -> usize {
        7 + 8 * b
    }
    pub const fn lift_thresh(b: usize) -> usize {
        8 + 8 * b
    }
    pub const fn lift(b: usize) -> usize {
        9 + 8 * b
    }
    pub const fn attack(b: usize) -> usize {
        10 + 8 * b
    }
    pub const fn release(b: usize) -> usize {
        11 + 8 * b
    }
    pub const fn gain(b: usize) -> usize {
        12 + 8 * b
    }
    pub const fn sc(b: usize) -> usize {
        13 + 8 * b
    }
    pub const BANDS: usize = 38;
    pub const X_LO: usize = 39;
    pub const X_MID: usize = 40;
    pub const X_HI: usize = 41;
    pub const SLOPE: usize = 42;
    pub const PHASE: usize = 43;
    pub const AUTO_GAIN: usize = 44;
    pub const AUTO_RELEASE: usize = 45;
    pub const KNEE: usize = 46;
    pub const RANGE: usize = 47;
    pub const LIFT_RANGE: usize = 48;
    pub const GUARD: usize = 49;
    pub const LINK: usize = 50;
    pub const SOLO: usize = 51;
    pub const DELTA: usize = 52;
    pub const BYPASS: usize = 53;
}

/// Enum option labels (enum order, final: options only grow at the end).
pub const CHARACTERS: [&str; 5] = ["Clean", "Bus", "FET", "Opto", "Pedal"];
pub const SIDECHAINS: [&str; 2] = ["Band", "Wide"];
pub const BAND_COUNTS: [&str; 2] = ["3", "4"];
pub const SLOPES: [&str; 2] = ["24 dB/oct", "48 dB/oct"];
pub const PHASES: [&str; 2] = ["Minimum", "Linear"];
pub const SOLOS: [&str; 5] = ["Off", "Low", "Low Mid", "High Mid", "High"];

/// Auto gain's reference level (dBFS RMS) for a band's own detector and for a Wide one (§2.6).
pub const REF_BAND_DB: f32 = -24.0;
pub const REF_WIDE_DB: f32 = -18.0;
/// Tilt's threshold shift on the outer bands at ±100 % (dB).
pub const TILT_DB: f32 = 12.0;
/// The steepest ratio Amount gives the engine (gfx-comp's maximum).
pub const MAX_RATIO: f32 = 30.0;
/// Auto gain's clamp (dB).
pub const MAKEUP_MAX_DB: f32 = 24.0;
/// The low band's lowest content (its detector window: 1.2 / 30 Hz = 40 ms).
pub const LOW_BAND_HZ: f32 = 30.0;
/// Each crossover is at least this factor above the one below (the upper one yields).
pub const SPACING: f32 = 1.5;
/// Tilt's weights per band slot, 4 and 3 bands (lo, lm = Mid, hm, hi).
const TILT_W4: [f32; BANDS] = [-1.0, -1.0 / 3.0, 1.0 / 3.0, 1.0];
const TILT_W3: [f32; BANDS] = [-1.0, 0.0, 0.0, 1.0];
/// Band slots of each layout, low to high.
const MAP4: [usize; 4] = [0, 1, 2, 3];
const MAP3: [usize; 3] = [0, 1, 3];
const PREROLL_MS: f32 = 30.0;
const SWITCH_FADE_MS: f32 = 10.0;
/// `phase`: fade out, switch, fade in (ms each).
const PHASE_FADE_MS: f32 = 10.0;
const GLIDE_MS: f32 = 20.0;

/// dB → gain, exactly 1.0 at 0 dB.
#[inline]
fn gain(db: f64) -> f64 {
    if db == 0.0 {
        1.0
    } else {
        (db * (std::f64::consts::LN_10 / 20.0)).exp()
    }
}

/// The quadratic soft knee of a depth below a threshold: 0 above the knee, the depth below it.
#[inline]
fn soft_depth(d: f32, w: f32) -> f32 {
    if 2.0 * d < -w {
        0.0
    } else if w > 0.0 && 2.0 * d.abs() <= w {
        (d + 0.5 * w).powi(2) / (2.0 * w)
    } else {
        d
    }
}

/// Lift's static gain (dB) at level `x` (§2.5): slope `k` (dB per dB) below threshold `tl` (with
/// gfx-comp's knee). k > 0 boosts: min(k · depth, `lift_range`, x − `guard`), never below 0, so
/// nothing at or under the guard is lifted and a boost never exceeds the level's height above it.
/// k < 0 cuts: k · depth, held at −`range`.
#[inline]
pub fn lift_gain(x: f32, tl: f32, k: f32, knee: f32, lift_range: f32, guard: f32, range: f32) -> f32 {
    if k == 0.0 {
        return 0.0;
    }
    let u = soft_depth(tl - x, knee);
    if k > 0.0 {
        (k * u).min(lift_range).min(x - guard).max(0.0)
    } else {
        (k * u).max(-range)
    }
}

/// The ratio Amount gives the engine (§2.6): 1 − 1/r′ = a (1 − 1/r), held at 30:1; exactly r at
/// 100 %.
#[inline]
pub fn effective_ratio(r: f32, amount: f32) -> f32 {
    let a = amount / 100.0;
    if a == 1.0 {
        return r.min(MAX_RATIO);
    }
    let s = (a * (1.0 - 1.0 / r.max(1.0))).clamp(0.0, 1.0 - 1.0 / MAX_RATIO);
    1.0 / (1.0 - s)
}

/// Effective crossovers (Hz) from the three values: each at least ×1.5 above the one below it in
/// use (`x_mid` in 4 bands, `x_lo` in 3); the upper one yields. Held under 0.45 fs.
pub fn effective_crossovers(x: [f32; 3], four: bool, sample_rate: f32) -> [f32; 3] {
    let top = 0.45 * sample_rate;
    let lo = x[0].clamp(40.0, 500.0);
    let mid = x[1].clamp(150.0, 3000.0).max(SPACING * lo);
    let below = if four { mid } else { lo };
    let hi = x[2].clamp(800.0, 12000.0).max(SPACING * below).min(top);
    [lo, mid, hi]
}

/// One band's values after Amount, Tilt and Time (what the engine and Lift receive).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BandEff {
    pub thresh: f32,
    pub ratio: f32,
    pub lift_thresh: f32,
    /// Lift's slope, dB per dB (−2 … +1).
    pub lift: f32,
    pub attack: f32,
    pub release: f32,
    /// Auto gain's make-up (dB), 0 with auto gain off.
    pub makeup: f32,
}

/// Every band's effective values from the parameter values (§2.6).
pub fn effective_bands(v: &[f32; NP]) -> [BandEff; BANDS] {
    let a = v[p::AMOUNT] / 100.0;
    let tilt = v[p::TILT] / 100.0;
    let time = v[p::TIME] / 100.0;
    let four = v[p::BANDS] >= 0.5;
    let w = if four { TILT_W4 } else { TILT_W3 };
    let (knee, range, lift_range, guard) = (v[p::KNEE], v[p::RANGE], v[p::LIFT_RANGE], v[p::GUARD]);
    core::array::from_fn(|b| {
        let shift = TILT_DB * tilt * w[b];
        let shift = if shift == 0.0 { 0.0 } else { shift };
        let raw_t = v[p::thresh(b)];
        let thresh = (raw_t + shift).clamp(-60.0, 0.0);
        let lift_thresh = (v[p::lift_thresh(b)].min(raw_t) + shift).clamp(-80.0, 0.0).min(thresh);
        let ratio = effective_ratio(v[p::ratio(b)], v[p::AMOUNT]);
        let k = v[p::lift(b)] / 100.0;
        let lift = if a == 1.0 { k } else { (a * k).clamp(-2.0, 1.0) };
        let (attack, release) = if time == 1.0 {
            (v[p::attack(b)], v[p::release(b)])
        } else {
            ((v[p::attack(b)] * time).clamp(0.02, 1200.0), (v[p::release(b)] * time).clamp(5.0, 12000.0))
        };
        let makeup = if v[p::AUTO_GAIN] >= 0.5 {
            let r = if v[p::sc(b)] >= 0.5 { REF_WIDE_DB } else { REF_BAND_DB };
            let dn = (static_curve(r, thresh, ratio, knee) - r).max(-range);
            let up = lift_gain(r, lift_thresh, lift, knee, lift_range, guard, range);
            let m = -(dn + up).max(-range);
            if m == 0.0 {
                0.0
            } else {
                m.clamp(-MAKEUP_MAX_DB, MAKEUP_MAX_DB)
            }
        } else {
            0.0
        };
        BandEff { thresh, ratio, lift_thresh, lift, attack, release, makeup }
    })
}

/// The audio engine's structure (what `bands` and `slope` choose).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub order: Order,
    pub four: bool,
}

impl Config {
    fn map(&self) -> &'static [usize] {
        if self.four {
            &MAP4
        } else {
            &MAP3
        }
    }

    /// The crossovers this layout uses, from the effective three.
    fn freqs(&self, e: [f32; 3]) -> ([f32; 3], usize) {
        if self.four {
            (e, 3)
        } else {
            ([e[0], e[2], 0.0], 2)
        }
    }
}

/// One audio path: the tree per channel and, in Linear, its linearizer ahead of it.
struct Engine {
    cfg: Config,
    tree: [Tree; 2],
    lin: [Linearizer; 2],
}

impl Engine {
    fn new(cfg: Config) -> Self {
        let nb = cfg.map().len();
        Self { cfg, tree: [Tree::new(cfg.order, nb), Tree::new(cfg.order, nb)], lin: [Linearizer::new(), Linearizer::new()] }
    }

    /// A new structure: trees rebuilt (silent), kernels rendered at once.
    fn configure(&mut self, cfg: Config, sr: f32, e: [f32; 3]) {
        self.cfg = cfg;
        let nb = cfg.map().len();
        let (xs, n) = cfg.freqs(e);
        for c in 0..2 {
            self.tree[c] = Tree::new(cfg.order, nb);
            self.tree[c].prepare(sr, GLIDE_MS);
            for k in 0..n {
                self.tree[c].set_freq_now(k, xs[k]);
            }
            self.lin[c].set_sections_now(cfg.order, &xs[..n]);
        }
    }

    fn set_freqs(&mut self, e: [f32; 3], linear: bool) {
        let (xs, n) = self.cfg.freqs(e);
        for c in 0..2 {
            for k in 0..n {
                self.tree[c].set_freq(k, xs[k]);
            }
            if linear {
                self.lin[c].set_sections(self.cfg.order, &xs[..n]);
            }
        }
    }

    /// Silence in the trees and the linearizers' history (a `phase` switch, at zero output).
    fn clear(&mut self, e: [f32; 3]) {
        let (xs, n) = self.cfg.freqs(e);
        for c in 0..2 {
            self.tree[c].reset();
            self.lin[c].set_sections_now(self.cfg.order, &xs[..n]);
            self.lin[c].reset();
        }
    }

    /// One channel's sample: the bands, weighted by `g` (wet) and by `s` alone (dry).
    #[inline]
    fn process(&mut self, c: usize, x: f64, linear: bool, g: &[[f64; 2]; BANDS], s: &[f64; BANDS]) -> (f64, f64) {
        let u = if linear { self.lin[c].process(x) } else { x };
        let mut out = [0.0f64; MAX_BANDS];
        self.tree[c].process(u, &mut out);
        let (mut wet, mut dry) = (0.0, 0.0);
        for (pos, &b) in self.cfg.map().iter().enumerate() {
            let v = out[pos] * s[b];
            dry += v;
            wet += v * g[b][c];
        }
        (wet, dry)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Switch {
    Idle,
    Preroll(u32),
    Fade(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PhaseSwitch {
    Idle,
    Out,
    In,
}

/// What the meters read over the last block (§2.7).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Meters {
    /// The largest band reduction (dB, ≥ 0).
    pub gr: f32,
    /// Each band's dynamic gain g_dn + g_up (dB, signed), the value farthest from 0.
    pub dyn_db: [f32; BANDS],
}

pub struct MbComp {
    v: [f32; NP],
    sr: f32,
    nch: usize,
    clock: u64,
    dirty: bool,
    eng: [Engine; 2],
    active: usize,
    sw: Switch,
    preroll_len: u32,
    fade_len: u32,
    linear: bool,
    lat: usize,
    phase_sw: PhaseSwitch,
    phase_gain: LinearRamp,
    xs: [f32; 3],
    det: [BwTree; 2],
    det_four: bool,
    comp: [Comp; BANDS],
    eff: [BandEff; BANDS],
    wide: [bool; BANDS],
    lift: [[f32; 2]; BANDS],
    lift_att: [f32; BANDS],
    lift_rel: [f32; BANDS],
    lift_k: [OnePole; BANDS],
    lift_t: [OnePole; BANDS],
    makeup: [OnePole; BANDS],
    band_gain: [OnePole; BANDS],
    /// (makeup dB, gain dB) the static factor was computed for, and the factor.
    static_cache: [(f32, f32, f64); BANDS],
    knee: OnePole,
    range: OnePole,
    lift_range: OnePole,
    guard: OnePole,
    link: OnePole,
    mix: ExpSmoother,
    level: ExpSmoother,
    level_cache: (f32, f64),
    solo: [LinearRamp; BANDS],
    delta: LinearRamp,
    bypass: LinearRamp,
    /// The input delayed by L (Linear's detection, dry and bypass), per channel.
    delay: [Vec<f32>; 2],
    dpos: usize,
    meters: Meters,
    /// Each slot's last band gain per channel (linear, make-up and `<b>_gain` included).
    g_hold: [[f64; 2]; BANDS],
}

impl MbComp {
    pub fn new() -> Self {
        let d = |i: usize| PARAMS[i].default;
        let v: [f32; NP] = core::array::from_fn(|i| PARAMS[i].default);
        let cfg = Config { order: Order::Lr4, four: false };
        let mut m = Self {
            v,
            sr: 48000.0,
            nch: 1,
            clock: 0,
            dirty: true,
            eng: [Engine::new(cfg), Engine::new(cfg)],
            active: 0,
            sw: Switch::Idle,
            preroll_len: 1440,
            fade_len: 480,
            linear: false,
            lat: 2048,
            phase_sw: PhaseSwitch::Idle,
            phase_gain: LinearRamp::new(PHASE_FADE_MS, 1.0),
            xs: [120.0, 600.0, 2500.0],
            det: [BwTree::new(3), BwTree::new(3)],
            det_four: false,
            comp: core::array::from_fn(|_| Comp::new()),
            eff: effective_bands(&v),
            wide: [false; BANDS],
            lift: [[0.0; 2]; BANDS],
            lift_att: [0.0; BANDS],
            lift_rel: [0.0; BANDS],
            lift_k: core::array::from_fn(|_| OnePole::new(20.0, 0.0, 1e-6)),
            lift_t: core::array::from_fn(|b| OnePole::new(20.0, d(p::lift_thresh(b)), 1e-4)),
            makeup: core::array::from_fn(|_| OnePole::new(50.0, 0.0, 1e-4)),
            band_gain: core::array::from_fn(|_| OnePole::new(20.0, 0.0, 1e-4)),
            static_cache: [(0.0, 0.0, 1.0); BANDS],
            knee: OnePole::new(20.0, d(p::KNEE), 1e-4),
            range: OnePole::new(20.0, d(p::RANGE), 1e-4),
            lift_range: OnePole::new(20.0, d(p::LIFT_RANGE), 1e-4),
            guard: OnePole::new(20.0, d(p::GUARD), 1e-4),
            link: OnePole::new(20.0, d(p::LINK), 1e-4),
            mix: ExpSmoother::new(0.0, 100.0, d(p::MIX)),
            level: ExpSmoother::new(-24.0, 12.0, d(p::LEVEL)),
            level_cache: (0.0, 1.0),
            solo: core::array::from_fn(|_| LinearRamp::new(5.0, 1.0)),
            delta: LinearRamp::new(10.0, 0.0),
            bypass: LinearRamp::new(10.0, 0.0),
            delay: [Vec::new(), Vec::new()],
            dpos: 0,
            meters: Meters::default(),
            g_hold: [[1.0; 2]; BANDS],
        };
        m.prepare(48000.0, 512, 1);
        m
    }

    fn want_cfg(&self) -> Config {
        Config { order: if self.v[p::SLOPE] >= 0.5 { Order::Lr8 } else { Order::Lr4 }, four: self.v[p::BANDS] >= 0.5 }
    }

    /// The structure playing now (tests).
    pub fn config(&self) -> Config {
        self.eng[self.active].cfg
    }

    /// Whether the Linear path is playing.
    pub fn is_linear(&self) -> bool {
        self.linear
    }

    /// The meters over the last processed block.
    pub fn meters(&self) -> Meters {
        self.meters
    }

    /// Effective values of every band (after Amount, Tilt, Time; tests and the bench).
    pub fn effective(&self) -> [BandEff; BANDS] {
        self.eff
    }

    /// The effective crossovers in use (Hz).
    pub fn crossovers(&self) -> [f32; 3] {
        self.xs
    }

    /// Recomputes everything a parameter change touches (block rate, only after a change).
    fn update(&mut self) {
        self.dirty = false;
        let v = self.v;
        let now = self.clock;
        let want = self.want_cfg();
        let four = want.four;
        self.xs = effective_crossovers([v[p::X_LO], v[p::X_MID], v[p::X_HI]], four, self.sr);
        let e = self.xs;
        let linear = self.linear;
        for eng in self.eng.iter_mut() {
            eng.set_freqs(e, linear);
        }
        // detection follows the wanted layout at once (the audio crossfades to it)
        let (xs, n) = want.freqs(e);
        if self.det_four != four {
            self.det_four = four;
            for d in self.det.iter_mut() {
                *d = BwTree::new(n + 1);
                d.prepare(self.sr, GLIDE_MS);
                for k in 0..n {
                    d.set_freq_now(k, xs[k]);
                }
            }
            if four {
                // the High Mid engine starts from rest
                self.comp[2].reset();
                self.lift[2] = [0.0; 2];
            }
        } else {
            for d in self.det.iter_mut() {
                for k in 0..n {
                    d.set_freq(k, xs[k]);
                }
            }
        }
        // bands
        self.eff = effective_bands(&v);
        let character = Character::from_index(v[p::CHARACTER].round().max(0.0) as u32);
        let low_hz = [LOW_BAND_HZ, e[0], e[1], e[2]];
        for b in 0..BANDS {
            let f = &self.eff[b];
            self.comp[b].set(CompSettings {
                thresh: f.thresh,
                ratio: f.ratio,
                makeup: 0.0,
                attack: f.attack,
                release: f.release,
                character,
                knee: v[p::KNEE],
                auto_release: v[p::AUTO_RELEASE] >= 0.5,
                sc_hpf: 0.0,
                range: v[p::RANGE],
                drive: 0.0,
                mix: 100.0,
                link: v[p::LINK],
                sc_listen: false,
            });
            self.comp[b].set_detector_low_hz(Some(low_hz[b]));
            self.wide[b] = v[p::sc(b)] >= 0.5;
            self.lift_k[b].set_target(f.lift);
            self.lift_t[b].set_target(f.lift_thresh);
            self.makeup[b].set_target(f.makeup);
            self.band_gain[b].set_target(v[p::gain(b)]);
            let sr = self.sr as f64;
            self.lift_att[b] = one_pole_coef(f.attack as f64 * 0.001 * sr) as f32;
            self.lift_rel[b] = one_pole_coef(f.release as f64 * 0.001 * sr) as f32;
        }
        self.knee.set_target(v[p::KNEE]);
        self.range.set_target(v[p::RANGE]);
        self.lift_range.set_target(v[p::LIFT_RANGE]);
        self.guard.set_target(v[p::GUARD]);
        self.link.set_target(v[p::LINK]);
        if self.mix.target() != v[p::MIX] {
            self.mix.set_target(v[p::MIX], now);
        }
        if self.level.target() != v[p::LEVEL] {
            self.level.set_target(v[p::LEVEL], now);
        }
        // solo: Low Mid and High Mid both solo the Mid band in 3 bands
        let solo = v[p::SOLO].round().max(0.0) as usize;
        let target = match (solo, four) {
            (0, _) => None,
            (3, false) => Some(1),
            (s, _) => Some((s - 1).min(BANDS - 1)),
        };
        for b in 0..BANDS {
            self.solo[b].set_target(if target.is_none_or(|t| t == b) { 1.0 } else { 0.0 });
        }
        self.delta.set_target(if v[p::DELTA] >= 0.5 { 1.0 } else { 0.0 });
        self.bypass.set_target(if v[p::BYPASS] >= 0.5 { 1.0 } else { 0.0 });
    }

    fn settle(&mut self) {
        for s in self
            .lift_k
            .iter_mut()
            .chain(self.lift_t.iter_mut())
            .chain(self.makeup.iter_mut())
            .chain(self.band_gain.iter_mut())
            .chain([&mut self.knee, &mut self.range, &mut self.lift_range, &mut self.guard, &mut self.link])
        {
            s.settle();
        }
        self.mix.reset();
        self.level.reset();
        for r in self.solo.iter_mut().chain([&mut self.delta, &mut self.bypass]) {
            r.settle();
        }
    }

    /// Starts a `bands` / `slope` switch or a `phase` switch when one is wanted and none is running.
    fn start_switches(&mut self) {
        if self.sw != Switch::Idle || self.phase_sw != PhaseSwitch::Idle {
            return;
        }
        let want = self.want_cfg();
        if want != self.eng[self.active].cfg {
            let o = 1 - self.active;
            let e = self.xs;
            let (a, b) = self.eng.split_at_mut(1);
            let (from, to) = if o == 1 { (&a[0], &mut b[0]) } else { (&b[0], &mut a[0]) };
            to.configure(want, self.sr, e);
            for c in 0..2 {
                to.lin[c].copy_history_from(&from.lin[c]);
            }
            self.sw = Switch::Preroll(self.preroll_len);
        } else if (self.v[p::PHASE] >= 0.5) != self.linear {
            self.phase_sw = PhaseSwitch::Out;
            self.phase_gain.set_target(0.0);
        }
    }

    /// Advances a running `phase` switch by one sample: (output gain, the audio path's input gain).
    /// The output fades out; at silence the path changes and starts from rest; then its **input**
    /// fades in, so the Linear path's FIR never sees an abrupt onset (it would play it L later).
    #[inline]
    fn phase_step(&mut self) -> (f64, f64) {
        let g = ramp_shape(&mut self.phase_gain);
        match self.phase_sw {
            PhaseSwitch::Idle => (1.0, 1.0),
            PhaseSwitch::Out => {
                if self.phase_gain.is_settled() {
                    // silent: change the path, its latency, and start from rest
                    self.linear = self.v[p::PHASE] >= 0.5;
                    let e = self.xs;
                    let a = self.active;
                    self.eng[a].clear(e);
                    self.phase_sw = PhaseSwitch::In;
                    self.phase_gain.set_target(1.0);
                    (1.0, 0.0)
                } else {
                    (g, 1.0)
                }
            }
            PhaseSwitch::In => {
                if self.phase_gain.is_settled() {
                    self.phase_sw = PhaseSwitch::Idle;
                }
                (1.0, g)
            }
        }
    }
}

impl Default for MbComp {
    fn default() -> Self {
        Self::new()
    }
}

/// A 0 – 1 ramp's next value, raised-cosine shaped while it moves (exact 0 or 1 once settled).
#[inline]
fn ramp_shape(r: &mut LinearRamp) -> f64 {
    let v = r.next() as f64;
    if r.is_settled() {
        v
    } else {
        0.5 - 0.5 * (PI * v).cos()
    }
}

impl Effect for MbComp {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, max_block: usize, channels: usize) {
        self.sr = sample_rate;
        self.nch = channels.clamp(1, 2);
        self.lat = linear_latency(sample_rate);
        for eng in self.eng.iter_mut() {
            for l in eng.lin.iter_mut() {
                l.prepare(sample_rate);
            }
        }
        for c in self.comp.iter_mut() {
            c.prepare(sample_rate, max_block, 2);
        }
        for s in self
            .lift_k
            .iter_mut()
            .chain(self.lift_t.iter_mut())
            .chain(self.makeup.iter_mut())
            .chain(self.band_gain.iter_mut())
            .chain([&mut self.knee, &mut self.range, &mut self.lift_range, &mut self.guard, &mut self.link])
        {
            s.prepare(sample_rate);
        }
        self.mix.prepare(sample_rate);
        self.level.prepare(sample_rate);
        for r in self.solo.iter_mut().chain([&mut self.delta, &mut self.bypass, &mut self.phase_gain]) {
            r.prepare(sample_rate);
        }
        self.delay = [vec![0.0; self.lat + 1], vec![0.0; self.lat + 1]];
        self.preroll_len = (PREROLL_MS * 0.001 * sample_rate).round() as u32;
        self.fade_len = (SWITCH_FADE_MS * 0.001 * sample_rate).round().max(1.0) as u32;
        self.det_four = !self.want_cfg().four;
        self.reset();
    }

    fn reset(&mut self) {
        self.clock = 0;
        // detection rebuilt for the layout (update() sees the change)
        self.det_four = !self.want_cfg().four;
        self.update();
        for d in self.det.iter_mut() {
            d.reset();
        }
        for c in self.comp.iter_mut() {
            c.reset();
        }
        self.settle();
        self.lift = [[0.0; 2]; BANDS];
        self.linear = self.v[p::PHASE] >= 0.5;
        self.active = 0;
        self.sw = Switch::Idle;
        self.phase_sw = PhaseSwitch::Idle;
        self.phase_gain.set_immediate(1.0);
        let cfg = self.want_cfg();
        let e = self.xs;
        for eng in self.eng.iter_mut() {
            eng.configure(cfg, self.sr, e);
            for l in eng.lin.iter_mut() {
                l.reset();
            }
        }
        for d in self.delay.iter_mut() {
            d.iter_mut().for_each(|v| *v = 0.0);
        }
        self.dpos = 0;
        self.static_cache = [(f32::NAN, f32::NAN, 1.0); BANDS];
        self.level_cache = (f32::NAN, 1.0);
        self.meters = Meters::default();
        self.g_hold = [[1.0; 2]; BANDS];
    }

    fn set_param(&mut self, id: &str, value: f32) {
        if let Some(i) = PARAMS.iter().position(|p| p.id == id) {
            let pi = &PARAMS[i];
            let v = value.clamp(pi.min, pi.max);
            if v != self.v[i] {
                self.v[i] = v;
                self.dirty = true;
            }
        }
    }

    fn latency(&self) -> usize {
        if self.linear {
            self.lat
        } else {
            0
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(self.nch);
        if nch == 0 {
            return;
        }
        if self.dirty {
            self.update();
        }
        let n = buffers[0].len();
        let mut meters = Meters::default();
        let dlen = self.delay[0].len();
        for i in 0..n {
            self.clock += 1;
            self.start_switches();
            let (pg, pin) = self.phase_step();
            let linear = self.linear;
            let mix = self.mix.next() as f64 * 0.01;
            let ldb = self.level.next();
            if ldb != self.level_cache.0 {
                self.level_cache = (ldb, gain(ldb as f64));
            }
            let level = self.level_cache.1;
            let knee = self.knee.next();
            let range = self.range.next();
            let lift_range = self.lift_range.next();
            let guard = self.guard.next();
            let link = self.link.next() * 0.01;
            let dl = ramp_shape(&mut self.delta);
            let bp = ramp_shape(&mut self.bypass);
            let mut s = [0.0f64; BANDS];
            for b in 0..BANDS {
                s[b] = ramp_shape(&mut self.solo[b]);
            }

            // input, and the input delayed by L
            let mut x = [0.0f32; 2];
            let mut d = [0.0f32; 2];
            let rpos = (self.dpos + 1) % dlen;
            for c in 0..nch {
                x[c] = buffers[c][i];
                self.delay[c][self.dpos] = x[c];
                d[c] = self.delay[c][rpos];
            }
            self.dpos = rpos;
            let det_in = if linear { d } else { x };

            // detection and the band gains
            let mut split = [[0.0f64; MAX_BANDS]; 2];
            for c in 0..nch {
                self.det[c].process(det_in[c] as f64, &mut split[c]);
            }
            let map: &[usize] = if self.det_four { &MAP4 } else { &MAP3 };
            // a slot the layout no longer computes keeps its last gain (the old engine fades out with it)
            let mut g = self.g_hold;
            for (pos, &b) in map.iter().enumerate() {
                let mut sc = [0.0f32; 2];
                for c in 0..nch {
                    sc[c] = if self.wide[b] { det_in[c] } else { split[c][pos] as f32 };
                }
                let gdn = self.comp[b].next_gain_db(&sc[..nch]);
                let k = self.lift_k[b].next();
                let tl = self.lift_t[b].next();
                let mut gup = [0.0f32; 2];
                for c in 0..nch {
                    let target = lift_gain(self.comp[b].level_db(c), tl, k, knee, lift_range, guard, range);
                    let st = &mut self.lift[b][c];
                    let a = if target < *st { self.lift_att[b] } else { self.lift_rel[b] };
                    *st = target + (*st - target) * a;
                    gup[c] = *st;
                }
                if nch == 2 && link > 0.0 {
                    let m = gup[0].min(gup[1]);
                    for c in 0..2 {
                        gup[c] = link * m + (1.0 - link) * gup[c];
                    }
                }
                let mk = self.makeup[b].next();
                let bg = self.band_gain[b].next();
                if (mk, bg) != (self.static_cache[b].0, self.static_cache[b].1) {
                    self.static_cache[b] = (mk, bg, gain((mk + bg) as f64));
                }
                let st = self.static_cache[b].2;
                for c in 0..nch {
                    let dy = (gdn[c] + gup[c]).max(-range);
                    g[b][c] = gain(dy as f64) * st;
                    if dy.abs() > meters.dyn_db[b].abs() {
                        meters.dyn_db[b] = dy;
                    }
                    meters.gr = meters.gr.max(-dy);
                }
            }
            self.g_hold = g;

            // audio: the playing engine, and the next one during a switch
            let a = self.active;
            let mut wd = [(0.0f64, 0.0f64); 2];
            for c in 0..nch {
                wd[c] = self.eng[a].process(c, x[c] as f64 * pin, linear, &g, &s);
            }
            match self.sw {
                Switch::Idle => {}
                Switch::Preroll(k) => {
                    for c in 0..nch {
                        let _ = self.eng[1 - a].process(c, x[c] as f64 * pin, linear, &g, &s);
                    }
                    self.sw = if k <= 1 { Switch::Fade(self.fade_len) } else { Switch::Preroll(k - 1) };
                }
                Switch::Fade(k) => {
                    let r = 1.0 - (k - 1) as f64 / self.fade_len as f64;
                    // Minimum: two dispersions (equal power); Linear: two aligned paths (equal gain)
                    let (wo, wn) = if linear {
                        let w = 0.5 - 0.5 * (PI * r).cos();
                        (1.0 - w, w)
                    } else {
                        let t = 0.5 * PI * r;
                        (t.cos(), t.sin())
                    };
                    for c in 0..nch {
                        let z = self.eng[1 - a].process(c, x[c] as f64 * pin, linear, &g, &s);
                        wd[c] = (wo * wd[c].0 + wn * z.0, wo * wd[c].1 + wn * z.1);
                    }
                    if k <= 1 {
                        self.active = 1 - a;
                        self.sw = Switch::Idle;
                    } else {
                        self.sw = Switch::Fade(k - 1);
                    }
                }
            }
            for c in 0..nch {
                let (wet, dry) = wd[c];
                // mix (crossfade with the aligned dry), delta (wet − dry), level, the phase switch
                let y = (dry * (1.0 - dl) + (wet - dry) * mix) * level * pg;
                let thru = if linear { d[c] } else { x[c] } as f64;
                buffers[c][i] = if bp == 0.0 { y as f32 } else { (bp * thru + (1.0 - bp) * y) as f32 };
            }
        }
        self.meters = meters;
    }
}
