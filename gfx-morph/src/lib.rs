// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-morph — golem's live vocal mangler (design: README.md), golem block type `morph`.
//!
//! The chain, all at zero latency (README §2):
//!
//! in → **guard** (a downward gate whose depth is the pedal's whole small-signal gain at its worst
//! frequency, plus 3 dB) → **pre** (low cut, high cut; the de-fizz detector reads here) → **tilt** →
//! **dirt** (dirt.rs: up 2× / 4× once, Single · Two · Three LR4 bands, two parallel types per band
//! blended by `morph` with a live ρ gain, vocal make-up tables, aligned dry and `mix`, down once) →
//! **de-fizz shelf** → **box** (gfx-dsp `modal`: an object of ≤ 48 physical modes and a skirt, A ⇄ B) →
//! **comp** (one knob) → **post** (low cut, resonant low-pass under the envelope) → **slap echo** →
//! `level` → out. `order` Box → Dirt moves the box before the dirt.
//!
//! Control rate: every [`CTRL`] base samples the smoothers advance, the detectors' dB laws run (guard,
//! de-fizz, envelope), the bands' drives and laws are rebuilt and the guard's cap recomputed; the
//! gains that multiply audio ramp per sample in between. The object's coefficients are recomputed every
//! second tick (32 samples) and ramped in gfx-dsp `modal`.
//!
//! Zero reported latency (minimum-phase half-bands and crossovers, ADAA, IIR banks). Real-time safe,
//! deterministic.

#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

pub mod dirt;
pub mod objects;
pub mod types;
pub mod voice;

#[cfg(feature = "plugin")]
mod plugin;

#[cfg(test)]
mod tests;

include!(concat!(env!("OUT_DIR"), "/tables.rs"));

use dirt::{rho_gain, BandFrame, Config, Engine, Frame, AIR, BANDS, BODY, LOW};
use gfx_dsp::delay::{DelayLine, Interp};
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::modal::{self, ModalBank, Mode, ModeList, MAX_MODES};
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{OnePoleTpt, Svf, SvfMode};
use gfx_dsp::{Effect, ParamInfo};
use objects::Object;
use std::f64::consts::PI;
use types::{SlotParams, Type, TABLE_POINTS};

pub use objects::OBJECT_NAMES;
pub use types::TYPE_NAMES;

pub const NP: usize = 45;

/// Option labels of `object_b`: Same, then the objects (both grow together, at the end).
pub const OBJECT_B_NAMES: [&str; 11] = ["Same", "None", "Tin Can", "Bucket", "Pipe", "Bottle", "Cardboard Box", "Oil Drum", "Glass Jar", "Cone", "Hands"];
pub const SPLIT_NAMES: [&str; 3] = ["Single", "Two", "Three"];
pub const ORDER_NAMES: [&str; 2] = ["Dirt → Box", "Box → Dirt"];

pub static PARAMS: [ParamInfo; NP] = [
    // the face
    ParamInfo::float("drive", "Drive", 0.0, 100.0, 40.0).with_exp(),
    ParamInfo::float("morph", "Morph", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("box", "Box", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::enumeration("object", "Object", 10, 1),
    // input
    ParamInfo::float("guard", "Guard", -90.0, -20.0, -55.0),
    ParamInfo::float("defizz", "De-fizz", 0.0, 100.0, 50.0),
    ParamInfo::float("env_drive", "Env → Drive", -24.0, 24.0, 0.0),
    ParamInfo::float("env_morph", "Env → Morph", -100.0, 100.0, 0.0),
    // pre
    ParamInfo::float("pre_hp", "Pre Low Cut", 20.0, 1000.0, 100.0),
    ParamInfo::float("pre_lp", "Pre High Cut", 1000.0, 20000.0, 20000.0),
    ParamInfo::float("tilt", "Tilt", -12.0, 12.0, 0.0),
    ParamInfo::float("tilt_freq", "Tilt Frequency", 200.0, 5000.0, 1000.0),
    // dirt
    ParamInfo::enumeration("split", "Bands", 3, 0),
    ParamInfo::float("x_low", "Low Crossover", 60.0, 1000.0, 250.0),
    ParamInfo::float("x_air", "Air Crossover", 1000.0, 10000.0, 3500.0),
    ParamInfo::enumeration("low_a", "Low Type A", 15, 0),
    ParamInfo::enumeration("body_a", "Body Type A", 15, 1),
    ParamInfo::enumeration("air_a", "Air Type A", 15, 0),
    ParamInfo::enumeration("low_b", "Low Type B", 15, 0),
    ParamInfo::enumeration("body_b", "Body Type B", 15, 2),
    ParamInfo::enumeration("air_b", "Air Type B", 15, 0),
    ParamInfo::float("low_trim", "Low Drive Trim", -24.0, 24.0, 0.0),
    ParamInfo::float("body_trim", "Body Drive Trim", -24.0, 24.0, 0.0),
    ParamInfo::float("air_trim", "Air Drive Trim", -24.0, 24.0, 0.0),
    ParamInfo::float("low_level", "Low Level", -60.0, 12.0, 0.0),
    ParamInfo::float("body_level", "Body Level", -60.0, 12.0, 0.0),
    ParamInfo::float("air_level", "Air Level", -60.0, 12.0, 0.0),
    ParamInfo::float("ring_hz", "Ring Frequency", 20.0, 2000.0, 60.0),
    ParamInfo::boolean("quality", "Quality", false),
    // box
    ParamInfo::enumeration("object_b", "Object B", 11, 0),
    ParamInfo::float("size", "Size", 25.0, 400.0, 100.0).with_exp(),
    ParamInfo::float("decay", "Decay", 25.0, 400.0, 100.0),
    ParamInfo::float("position", "Position", 0.0, 100.0, 50.0).with_exp(),
    ParamInfo::enumeration("order", "Order", 2, 0),
    // dynamics and post
    ParamInfo::float("comp", "Compress", 0.0, 100.0, 0.0),
    ParamInfo::float("post_hp", "Post Low Cut", 20.0, 1000.0, 20.0),
    ParamInfo::float("post_lp", "Post High Cut", 1000.0, 20000.0, 20000.0).with_exp(),
    ParamInfo::float("reso", "Resonance", 0.0, 100.0, 0.0),
    ParamInfo::float("env_filter", "Env → Filter", -4.0, 4.0, 0.0),
    // echo
    ParamInfo::float("echo_time", "Echo Time", 20.0, 500.0, 110.0),
    ParamInfo::float("echo_fb", "Echo Feedback", 0.0, 90.0, 30.0),
    ParamInfo::float("echo_mix", "Echo Mix", 0.0, 100.0, 0.0),
    // output
    ParamInfo::float("level", "Level", -24.0, 12.0, 0.0).with_exp(),
    ParamInfo::boolean("bypass", "Bypass", false),
];

pub mod p {
    pub const DRIVE: usize = 0;
    pub const MORPH: usize = 1;
    pub const BOX: usize = 2;
    pub const MIX: usize = 3;
    pub const OBJECT: usize = 4;
    pub const GUARD: usize = 5;
    pub const DEFIZZ: usize = 6;
    pub const ENV_DRIVE: usize = 7;
    pub const ENV_MORPH: usize = 8;
    pub const PRE_HP: usize = 9;
    pub const PRE_LP: usize = 10;
    pub const TILT: usize = 11;
    pub const TILT_FREQ: usize = 12;
    pub const SPLIT: usize = 13;
    pub const X_LOW: usize = 14;
    pub const X_AIR: usize = 15;
    /// type A of band b (low, body, air)
    pub const fn ta(b: usize) -> usize {
        16 + b
    }
    pub const fn tb(b: usize) -> usize {
        19 + b
    }
    pub const fn trim(b: usize) -> usize {
        22 + b
    }
    pub const fn level(b: usize) -> usize {
        25 + b
    }
    pub const RING_HZ: usize = 28;
    pub const QUALITY: usize = 29;
    pub const OBJECT_B: usize = 30;
    pub const SIZE: usize = 31;
    pub const DECAY: usize = 32;
    pub const POSITION: usize = 33;
    pub const ORDER: usize = 34;
    pub const COMP: usize = 35;
    pub const POST_HP: usize = 36;
    pub const POST_LP: usize = 37;
    pub const RESO: usize = 38;
    pub const ENV_FILTER: usize = 39;
    pub const ECHO_TIME: usize = 40;
    pub const ECHO_FB: usize = 41;
    pub const ECHO_MIX: usize = 42;
    pub const LEVEL: usize = 43;
    pub const BYPASS: usize = 44;
}

/// The control period (base samples).
pub const CTRL: usize = 16;
/// The object's coefficients are recomputed every this many control ticks (32 samples).
const BOX_TICKS: u64 = 2;
/// `drive` 0 – 100 % → 0 – 48 dB.
pub const DRIVE_DB_PER_PCT: f64 = 0.48;
/// Pre-roll and crossfade of a `split` / `quality` switch (ms); type crossfade; `order` fade.
const PREROLL_MS: f32 = 30.0;
const SWITCH_FADE_MS: f32 = 10.0;
const TYPE_FADE_MS: f32 = 10.0;
const ORDER_FADE_MS: f32 = 10.0;
const OBJECT_FADE_MS: f32 = 20.0;
/// The filters' engage ramp at their stops (20 Hz, 20 kHz) (ms).
const CUT_RAMP_MS: f32 = 30.0;

// guard (README §2.2)
const GUARD_OFF: f64 = -90.0;
const GUARD_RMS_MS: f64 = 5.0;
const GUARD_KNEE_DB: f64 = 6.0;
const GUARD_HYST_DB: f64 = 3.0;
const GUARD_MARGIN_DB: f64 = 3.0;
const GUARD_ATTACK_MS: f64 = 1.0;
const GUARD_RELEASE_MS: f64 = 80.0;
// de-fizz (README §2.3)
pub const DEFIZZ_HZ: f64 = 4500.0;
const DEFIZZ_RMS_MS: f64 = 5.0;
const DEFIZZ_ATTACK_MS: f64 = 0.5;
const DEFIZZ_RELEASE_MS: f64 = 30.0;
const DEFIZZ_MAX_DB: f64 = 18.0;
const DEFIZZ_SHARE_DB: f64 = -14.0;
const DEFIZZ_SHARE_SPAN: f64 = 8.0;
const DEFIZZ_FLOOR: (f64, f64) = (-66.0, -60.0);
// envelope (README §2.8)
const ENV_RMS_MS: f64 = 10.0;
const ENV_ATTACK_MS: f64 = 5.0;
const ENV_RELEASE_MS: f64 = 120.0;
const ENV_FLOOR_DB: f64 = -48.0;
const ENV_SPAN_DB: f64 = 36.0;
// comp (README §2.8)
const COMP_ATTACK_MS: f64 = 3.0;
const COMP_RELEASE_MS: f64 = 120.0;
const COMP_KNEE_DB: f64 = 6.0;
pub const COMP_REF_DB: f64 = -18.0;
// echo (README §2.9)
const ECHO_LP_HZ: f64 = 4000.0;
const ECHO_GLIDE_MS: f32 = 50.0;
const ECHO_MAX_MS: f64 = 500.0;

/// The comp's threshold (dBFS) and ratio at `comp` (0 – 100).
#[inline]
pub fn comp_law(c: f64) -> (f64, f64) {
    (-6.0 - 0.24 * c, 1.0 + 0.07 * c)
}

/// The comp's static curve: gain reduction (dB, ≥ 0) at level `l` (dB), soft knee 6 dB.
#[inline]
pub fn comp_gr(l: f64, t: f64, r: f64) -> f64 {
    let o = l - t;
    let w = COMP_KNEE_DB;
    if 2.0 * o < -w {
        0.0
    } else if 2.0 * o <= w {
        (1.0 - 1.0 / r) * (o + w / 2.0).powi(2) / (2.0 * w)
    } else {
        o * (1.0 - 1.0 / r)
    }
}

/// The comp's static make-up (dB): half its gain reduction at the reference level.
#[inline]
pub fn comp_makeup(c: f64) -> f64 {
    if c <= 0.0 {
        return 0.0;
    }
    let (t, r) = comp_law(c);
    0.5 * comp_gr(COMP_REF_DB, t, r)
}

/// The post low-pass Q at `reso` (0 – 100): 0.707 → 6, linear; no self-oscillation.
#[inline]
pub fn reso_q(reso: f64) -> f64 {
    std::f64::consts::FRAC_1_SQRT_2 + (6.0 - std::f64::consts::FRAC_1_SQRT_2) * reso / 100.0
}

/// A resonant low-pass's peak (dB) at quality q.
#[inline]
pub fn reso_peak_db(q: f64) -> f64 {
    if q <= std::f64::consts::FRAC_1_SQRT_2 {
        0.0
    } else {
        20.0 * (q / (1.0 - 1.0 / (4.0 * q * q)).sqrt()).log10()
    }
}

/// A table read at drive `d` (dB), linear between the 1 dB points.
#[inline]
pub fn table_at(t: &[f32; TABLE_POINTS], d: f64) -> f64 {
    let x = d.clamp(0.0, (TABLE_POINTS - 1) as f64);
    let i = (x as usize).min(TABLE_POINTS - 2);
    let fr = x - i as f64;
    t[i] as f64 * (1.0 - fr) + t[i + 1] as f64 * fr
}

/// The vocal make-up (dB) of `ty` at drive `d`.
#[inline]
pub fn makeup_db(ty: Type, d: f64) -> f64 {
    table_at(&MAKEUP_DB[ty as usize], d)
}

/// The small-signal gain S(type, d) (dB) the guard reads.
#[inline]
pub fn small_db(ty: Type, d: f64) -> f64 {
    table_at(&SMALL_DB[ty as usize], d)
}

#[inline]
fn db_lin(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// One-pole coefficient for a time constant at the control rate.
#[inline]
fn ctrl_coef(ms: f64, sr: f64) -> f64 {
    gfx_dsp::util::one_pole_coef(ms * 0.001 * sr / CTRL as f64)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Switch {
    Idle,
    Preroll(u32),
    Fade(u32),
}

/// A 12 dB / oct cut that is the exact identity at its stop (20 Hz or 20 kHz), with a 30 ms ramp in
/// and out (gfx-dsp `cuts`' rule).
#[derive(Clone, Debug)]
struct Cut12 {
    mode: SvfMode,
    svf: [Svf; 2],
    on: LinearRamp,
    f: f64,
    live: bool,
}

impl Cut12 {
    fn new(high_pass: bool) -> Self {
        Self { mode: if high_pass { SvfMode::HighPass } else { SvfMode::LowPass }, svf: Default::default(), on: LinearRamp::new(CUT_RAMP_MS, 0.0), f: f64::NAN, live: false }
    }

    fn prepare(&mut self, sr: f32) {
        self.on.prepare(sr);
    }

    /// The cut-off (Hz) and whether it is at its stop.
    fn set(&mut self, f: f64, stop: bool, sr: f32) {
        self.on.set_target(if stop { 0.0 } else { 1.0 });
        if f != self.f {
            self.f = f;
            for s in self.svf.iter_mut() {
                s.set(self.mode, f as f32, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            }
        }
    }

    fn reset(&mut self) {
        self.on.settle();
        self.svf.iter_mut().for_each(|s| s.reset());
        self.live = false;
    }

    #[inline]
    fn process(&mut self, x: &mut [f32; 2], nch: usize) {
        let on = self.on.next();
        if on == 0.0 && self.on.is_settled() {
            if self.live {
                self.live = false;
                self.svf.iter_mut().for_each(|s| s.reset());
            }
            return;
        }
        self.live = true;
        for ch in 0..nch {
            let y = self.svf[ch].process(x[ch]);
            x[ch] = if on == 1.0 { y } else { x[ch] + on * (y - x[ch]) };
        }
    }
}

/// A value at the control rate with its per-sample linear ramp toward the next tick's.
#[derive(Clone, Copy, Debug, Default)]
struct Ramp {
    v: f64,
    step: f64,
}

impl Ramp {
    #[inline]
    fn to(&mut self, target: f64) {
        self.step = (target - self.v) / CTRL as f64;
    }
    #[inline]
    fn jump(&mut self, v: f64) {
        self.v = v;
        self.step = 0.0;
    }
    #[inline]
    fn next(&mut self) -> f64 {
        self.v += self.step;
        self.v
    }
    #[inline]
    fn moving(&self) -> bool {
        self.step != 0.0
    }
}

/// The object stage (README §2.7): the bank in f64 lanes per channel, its skirt, a 20 ms crossfade on
/// an object change, `box` between the direct path and the object.
struct BoxStage {
    defs: Vec<Object>,
    /// [playing, fading out][channel]
    bank: [[ModalBank; 2]; 2],
    skirt: [[[Svf; 2]; 2]; 2],
    skirt_gain: [f64; 2],
    skirt_f: [(f64, f64); 2],
    fade: LinearRamp,
    fading: bool,
    amount: Ramp,
    awake: bool,
    /// (object A, object B, μ, position, size, decay) the bank was set for
    key: (usize, usize, f64, f64, f64, f64),
    list: ModeList,
    peak_db: f64,
    fs: f64,
}

impl BoxStage {
    fn new() -> Self {
        Self {
            defs: (0..objects::OBJECTS).map(Object::new).collect(),
            bank: Default::default(),
            skirt: Default::default(),
            skirt_gain: [0.0; 2],
            skirt_f: [(0.0, 0.0); 2],
            fade: LinearRamp::new(OBJECT_FADE_MS, 1.0),
            fading: false,
            amount: Ramp::default(),
            awake: false,
            key: (usize::MAX, 0, 0.0, 0.0, 0.0, 0.0),
            list: ModeList::default(),
            peak_db: 0.0,
            fs: 48000.0,
        }
    }

    fn prepare(&mut self, sr: f32) {
        self.fs = sr as f64;
        self.fade.prepare(sr);
    }

    fn reset(&mut self) {
        for b in self.bank.iter_mut().flatten() {
            b.reset();
        }
        for s in self.skirt.iter_mut().flatten().flatten() {
            s.reset();
        }
        self.fade.settle();
        self.fading = false;
        self.awake = false;
        self.key.0 = usize::MAX;
    }

    /// The level (dB) that normalises object `o`: its pink table at the setting and its voice constant.
    fn level_db(o: usize, pos: f64, size: f64, decay: f64) -> f64 {
        objects::level_at(&BOX_LEVEL_DB[o], pos, decay, size) + BOX_VOICE_DB[o] as f64
    }

    /// The largest peak re bypass of object `o` (dB).
    fn object_peak(o: usize, size: f64, decay: f64) -> f64 {
        if o == 0 {
            0.0
        } else {
            objects::peak_at(&BOX_PEAK_DB[o], decay, size) + BOX_VOICE_DB[o] as f64
        }
    }

    /// The skirt of object `o` at `size`: (high-pass Hz, low-pass Hz, linear gain incl. its level).
    fn skirt_of(&self, o: usize, size: f64, scale: f64) -> (f64, f64, f64) {
        if o == 0 {
            return (10.0, 0.45 * self.fs, 1.0);
        }
        let d = &self.defs[o];
        let (hp, lp) = d.skirt(size);
        (hp, lp.min(0.45 * self.fs), scale * db_lin(d.skirt_db))
    }

    /// Rebuilds the bank (every 32 samples while anything moves): object A ⇄ B by rank at μ, each
    /// pre-scaled by its level, edges faded; the skirt interpolated.
    fn update(&mut self, a: usize, b: usize, mu: f64, pos: f64, size: f64, decay: f64, ramp: u32) {
        let key = (a, b, mu, pos, size, decay);
        // the guard's peak term: the larger end while the morph is between them
        let (pa, pb) = (Self::object_peak(a, size, decay), Self::object_peak(b, size, decay));
        self.peak_db = if a == b || mu == 0.0 {
            pa
        } else if mu == 1.0 {
            pb
        } else {
            pa.max(pb)
        };
        if key == self.key && !self.bank[0][0].is_ramping() {
            return;
        }
        let first = self.key.0 == usize::MAX;
        self.key = key;
        let mut ma = [Mode::default(); MAX_MODES];
        let mut mb = [Mode::default(); MAX_MODES];
        let ka = db_lin(Self::level_db(a, pos, size, decay));
        let na = self.defs[a].modes(pos, size, decay, ka, &mut ma);
        let (nb, kb) = if b != a && mu > 0.0 {
            let kb = db_lin(Self::level_db(b, pos, size, decay));
            (self.defs[b].modes(pos, size, decay, kb, &mut mb), kb)
        } else {
            (0, 1.0)
        };
        if b != a && mu > 0.0 {
            modal::morph(&ma[..na], &mb[..nb], mu, &mut self.list);
        } else {
            modal::morph(&ma[..na], &[], 0.0, &mut self.list);
        }
        for m in self.list.modes[..self.list.n].iter_mut() {
            *m = modal::edge_fade(*m, self.fs);
        }
        let n = if first { 0 } else { ramp };
        for bank in self.bank[0].iter_mut() {
            bank.set_ramped(self.list.as_slice(), self.fs, n);
        }
        let sa = self.skirt_of(a, size, ka);
        let s = if b != a && mu > 0.0 {
            let sb = self.skirt_of(b, size, kb);
            let l = |x: f64, y: f64| ((1.0 - mu) * x.ln() + mu * y.ln()).exp();
            (l(sa.0, sb.0), l(sa.1, sb.1), (1.0 - mu) * sa.2 + mu * sb.2)
        } else {
            sa
        };
        self.skirt_gain[0] = s.2;
        if (s.0, s.1) != self.skirt_f[0] {
            self.skirt_f[0] = (s.0, s.1);
            for ch in self.skirt[0].iter_mut() {
                ch[0].set(SvfMode::HighPass, s.0 as f32, std::f32::consts::FRAC_1_SQRT_2, 0.0, self.fs as f32);
                ch[1].set(SvfMode::LowPass, s.1 as f32, std::f32::consts::FRAC_1_SQRT_2, 0.0, self.fs as f32);
            }
        }
    }

    /// A new object (or object B): the playing bank fades out over 20 ms while the new one starts from
    /// rest (only while the box is heard).
    fn change_object(&mut self) {
        if self.awake && !self.fading {
            let [a, b] = &mut self.bank;
            b.clone_from(a);
            let [sa, sb] = &mut self.skirt;
            sb.clone_from(sa);
            self.skirt_gain[1] = self.skirt_gain[0];
            self.skirt_f[1] = self.skirt_f[0];
            for bank in self.bank[0].iter_mut() {
                bank.reset();
            }
            for s in self.skirt[0].iter_mut().flatten() {
                s.reset();
            }
            self.fade.set_immediate(0.0);
            self.fade.set_target(1.0);
            self.fading = true;
        }
        self.key.0 = usize::MAX;
    }

    #[inline]
    fn wet(&mut self, k: usize, ch: usize, x: f32) -> f64 {
        let m = self.bank[k][ch].process(x as f64);
        let s = &mut self.skirt[k][ch];
        let h = s[0].process(x);
        let v = s[1].process(h);
        m + self.skirt_gain[k] * v as f64
    }

    #[inline]
    fn process(&mut self, x: &mut [f32; 2], nch: usize) {
        let b = self.amount.next();
        if b <= 0.0 && !self.amount.moving() && !self.fading {
            if self.awake {
                self.awake = false;
                for bank in self.bank.iter_mut().flatten() {
                    bank.reset();
                }
                for s in self.skirt.iter_mut().flatten().flatten() {
                    s.reset();
                }
            }
            return;
        }
        self.awake = true;
        let f = if self.fading {
            let r = self.fade.next() as f64;
            if self.fade.is_settled() {
                self.fading = false;
            }
            0.5 - 0.5 * (PI * r).cos()
        } else {
            1.0
        };
        for ch in 0..nch {
            let mut w = self.wet(0, ch, x[ch]);
            if f < 1.0 {
                let o = self.wet(1, ch, x[ch]);
                w = f * w + (1.0 - f) * o;
            }
            x[ch] = ((1.0 - b) * x[ch] as f64 + b * w) as f32;
        }
    }
}

/// The one-knob compressor (README §2.8): gfx-comp's feed-forward law (soft knee 6 dB), linked,
/// static make-up. The Giannoulis smooth decoupled peak detector runs per sample on the linked level
/// (|x|, max of the channels); the dB gain computer runs at the control rate, its gain ramped per
/// sample (two transcendental functions per tick instead of per sample).
#[derive(Clone, Debug, Default)]
struct Comp {
    on: bool,
    t: f64,
    r: f64,
    makeup: f64,
    att: f64,
    rel: f64,
    y1: f64,
    yl: f64,
    gr: f64,
    g: Ramp,
}

impl Comp {
    /// The gain computer at a control tick: GR (dB) from the detector, the gain's ramp target.
    fn tick(&mut self) {
        let l = if self.yl > 1e-9 { 20.0 * self.yl.log10() } else { -180.0 };
        self.gr = comp_gr(l, self.t, self.r);
        self.g.to(db_lin(self.makeup - self.gr));
    }

    #[inline]
    fn process(&mut self, x: &mut [f32; 2], nch: usize) {
        let mut l = 0.0f32;
        for ch in 0..nch {
            l = l.max(x[ch].abs());
        }
        let l = l as f64;
        self.y1 = l.max(self.rel * self.y1 + (1.0 - self.rel) * l);
        self.yl = self.att * self.yl + (1.0 - self.att) * self.y1;
        let g = self.g.next() as f32;
        for ch in 0..nch {
            x[ch] *= g;
        }
    }
}

/// The slap echo (README §2.9): one tap, feedback through a 4 kHz one-pole, added to the dry.
struct Echo {
    line: [DelayLine; 2],
    lp: [OnePoleTpt; 2],
    lp_g: f64,
    time: OnePole,
    fb: Ramp,
    mix: Ramp,
    awake: bool,
}

impl Echo {
    fn new() -> Self {
        Self { line: Default::default(), lp: Default::default(), lp_g: 0.0, time: OnePole::new(ECHO_GLIDE_MS, 110.0, 1e-3), fb: Ramp::default(), mix: Ramp::default(), awake: false }
    }

    #[inline]
    fn process(&mut self, x: &mut [f32; 2], nch: usize, sr: f64) {
        let mix = self.mix.next();
        let fb = self.fb.next();
        let t = self.time.next() as f64;
        if mix <= 0.0 && !self.mix.moving() {
            if self.awake {
                self.awake = false;
                self.line.iter_mut().for_each(|l| l.reset());
                self.lp.iter_mut().for_each(|l| l.reset());
            }
            return;
        }
        self.awake = true;
        let d = (t * 0.001 * sr - 1.0).max(1.0);
        for ch in 0..nch {
            let wet = self.line[ch].read(d, Interp::Hermite) as f64;
            let back = self.lp[ch].low(wet, self.lp_g) * fb;
            self.line[ch].push((x[ch] as f64 + back) as f32);
            x[ch] = (x[ch] as f64 + mix * wet) as f32;
        }
    }
}

/// The smoothed controls (control rate).
struct Smooth {
    exp: [ExpSmoother; 8],
    one: Vec<(usize, OnePole)>,
}

/// The EXP parameters and their slot in `Smooth::exp`.
const EXP_IDS: [usize; 8] = [p::DRIVE, p::MORPH, p::BOX, p::MIX, p::SIZE, p::POSITION, p::POST_LP, p::LEVEL];
/// The other smoothed floats: 20 ms (the log ones smoothed on log2 of their value).
const ONE_IDS: [usize; 25] = [
    p::DEFIZZ,
    p::ENV_DRIVE,
    p::ENV_MORPH,
    p::PRE_HP,
    p::PRE_LP,
    p::TILT,
    p::TILT_FREQ,
    p::X_LOW,
    p::X_AIR,
    p::trim(0),
    p::trim(1),
    p::trim(2),
    p::level(0),
    p::level(1),
    p::level(2),
    p::RING_HZ,
    p::DECAY,
    p::COMP,
    p::POST_HP,
    p::RESO,
    p::ENV_FILTER,
    p::ECHO_FB,
    p::ECHO_MIX,
    p::GUARD,
    p::ECHO_TIME,
];
const LOG_IDS: [usize; 7] = [p::PRE_HP, p::PRE_LP, p::TILT_FREQ, p::X_LOW, p::X_AIR, p::RING_HZ, p::POST_HP];

pub struct Morph {
    v: [f32; NP],
    sr: f32,
    nch: usize,
    /// base samples since reset, and the position in the control period
    clock: u64,
    ticks: u64,
    phase: usize,
    sm: Smooth,
    /// current smoothed values (index = parameter)
    c: [f64; NP],
    // dirt
    eng: [Engine; 2],
    active: usize,
    sw: Switch,
    sw_power: bool,
    preroll_len: u32,
    fade_len: u32,
    frame: Frame,
    mu: Ramp,
    band_ramps: [[Ramp; 3]; BANDS],
    rho_g: [f64; BANDS],
    types: [[Type; 2]; BANDS],
    cache: [[(Type, f64, f64); 2]; BANDS],
    fades: [[LinearRamp; 2]; BANDS],
    drive_db: [f64; BANDS],
    // detectors (linked)
    p_guard: f64,
    guard_cut: f64,
    guard_open: bool,
    guard_g: Ramp,
    g_cap: f64,
    p_full: f64,
    p_hf: f64,
    hf: [Svf; 2],
    defizz_c: f64,
    p_env: f64,
    env: f64,
    coefs: Coefs,
    // filters
    pre_hp: Cut12,
    pre_lp: Cut12,
    tilt: [OnePoleTpt; 2],
    tilt_k: (f64, f64, f64),
    tilt_on: bool,
    shelf: [Svf; 2],
    shelf_db: f64,
    // box, comp, post, echo
    boxs: BoxStage,
    obj_key: (usize, usize),
    comp: Comp,
    gr: f64,
    post_hp: Cut12,
    post_lp: [Svf; 2],
    post_on: LinearRamp,
    post_key: (f64, f64),
    echo: Echo,
    level: Ramp,
    // order switch
    order_now: usize,
    order_g: LinearRamp,
    bypass: LinearRamp,
}

/// Rate-dependent coefficients (control rate unless noted).
#[derive(Clone, Copy, Debug, Default)]
struct Coefs {
    guard_rms: f64,
    guard_att: f64,
    guard_rel: f64,
    fizz_rms: f64,
    fizz_att: f64,
    fizz_rel: f64,
    env_rms: f64,
    env_att: f64,
    env_rel: f64,
    rho: f64,
}

impl Morph {
    pub fn new() -> Self {
        let d = |i: usize| PARAMS[i].default;
        let cfg = Config { factor: 2, bands: 1 };
        let mut m = Self {
            v: core::array::from_fn(|i| PARAMS[i].default),
            sr: 48000.0,
            nch: 1,
            clock: 0,
            ticks: 0,
            phase: 0,
            sm: Smooth {
                exp: core::array::from_fn(|k| {
                    let pi = PARAMS[EXP_IDS[k]];
                    ExpSmoother::new(pi.min, pi.max, pi.default)
                }),
                one: ONE_IDS
                    .iter()
                    .map(|&i| {
                        let ms = if i == p::ECHO_TIME { ECHO_GLIDE_MS } else if i == p::GUARD { 1.0 } else { 20.0 };
                        let v = if LOG_IDS.contains(&i) { d(i).log2() } else { d(i) };
                        (i, OnePole::new(ms, v, 1e-6))
                    })
                    .collect(),
            },
            c: core::array::from_fn(|i| PARAMS[i].default as f64),
            eng: [Engine::new(cfg), Engine::new(cfg)],
            active: 0,
            sw: Switch::Idle,
            sw_power: false,
            preroll_len: 1440,
            fade_len: 480,
            frame: Frame::default(),
            mu: Ramp::default(),
            band_ramps: Default::default(),
            rho_g: [1.0; BANDS],
            types: [[Type::Off; 2]; BANDS],
            cache: [[(Type::Off, f64::NAN, f64::NAN); 2]; BANDS],
            fades: core::array::from_fn(|_| core::array::from_fn(|_| LinearRamp::new(TYPE_FADE_MS, 1.0))),
            drive_db: [0.0; BANDS],
            p_guard: 0.0,
            guard_cut: 0.0,
            guard_open: true,
            guard_g: Ramp::default(),
            g_cap: 0.0,
            p_full: 0.0,
            p_hf: 0.0,
            hf: Default::default(),
            defizz_c: 0.0,
            p_env: 0.0,
            env: 0.0,
            coefs: Coefs::default(),
            pre_hp: Cut12::new(true),
            pre_lp: Cut12::new(false),
            tilt: Default::default(),
            tilt_k: (0.0, 1.0, 0.0),
            tilt_on: false,
            shelf: Default::default(),
            shelf_db: f64::NAN,
            boxs: BoxStage::new(),
            obj_key: (usize::MAX, usize::MAX),
            comp: Comp::default(),
            gr: 0.0,
            post_hp: Cut12::new(true),
            post_lp: Default::default(),
            post_on: LinearRamp::new(CUT_RAMP_MS, 0.0),
            post_key: (f64::NAN, f64::NAN),
            echo: Echo::new(),
            level: Ramp::default(),
            order_now: 0,
            order_g: LinearRamp::new(ORDER_FADE_MS, 1.0),
            bypass: LinearRamp::new(10.0, 0.0),
        };
        m.set_targets();
        m
    }

    fn want_cfg(&self) -> Config {
        let on = self.v[p::QUALITY] >= 0.5;
        let factor = match (self.sr >= 88200.0, on) {
            (false, false) => 2,
            (false, true) => 4,
            (true, false) => 1,
            (true, true) => 2,
        };
        Config { factor, bands: (self.v[p::SPLIT].round() as usize).min(2) + 1 }
    }

    /// The configuration playing now (tests).
    pub fn config(&self) -> Config {
        self.eng[self.active].cfg
    }

    fn set_targets(&mut self) {
        let now = self.ticks;
        for (k, &i) in EXP_IDS.iter().enumerate() {
            self.sm.exp[k].set_target(self.v[i], now);
        }
        for (i, s) in self.sm.one.iter_mut() {
            let v = self.v[*i];
            s.set_target(if LOG_IDS.contains(i) { v.log2() } else { v });
        }
        for b in 0..BANDS {
            self.types[b] = [Type::from_index(self.v[p::ta(b)].round().max(0.0) as usize), Type::from_index(self.v[p::tb(b)].round().max(0.0) as usize)];
        }
        self.bypass.set_target(if self.v[p::BYPASS] >= 0.5 { 1.0 } else { 0.0 });
    }

    /// Advances every smoother by one control tick into `c`.
    fn smooth_tick(&mut self) {
        for (k, &i) in EXP_IDS.iter().enumerate() {
            self.c[i] = self.sm.exp[k].next() as f64;
        }
        for (i, s) in self.sm.one.iter_mut() {
            let v = s.next() as f64;
            self.c[*i] = if LOG_IDS.contains(i) { v.exp2() } else { v };
        }
        for i in [p::OBJECT, p::SPLIT, p::QUALITY, p::OBJECT_B, p::ORDER, p::BYPASS] {
            self.c[i] = self.v[i] as f64;
        }
        for b in 0..BANDS {
            self.c[p::ta(b)] = self.v[p::ta(b)] as f64;
            self.c[p::tb(b)] = self.v[p::tb(b)] as f64;
        }
    }

    fn start_fade(&mut self, b: usize, s: usize) {
        let f = &mut self.frame.bands[b].slots[s];
        f.old = f.cur;
        f.gen = f.gen.wrapping_add(1);
        f.fading = true;
        f.fade = 0.0;
        self.fades[b][s].set_immediate(0.0);
        self.fades[b][s].set_target(1.0);
    }

    /// The effective object pair: (A, B, whether B morphs).
    fn objects(&self) -> (usize, usize) {
        let a = (self.v[p::OBJECT].round().max(0.0) as usize).min(objects::OBJECTS - 1);
        let ob = self.v[p::OBJECT_B].round().max(0.0) as usize;
        let b = if ob == 0 { a } else { (ob - 1).min(objects::OBJECTS - 1) };
        (a, b)
    }

    /// The bands of the playing configuration (and of the incoming one during a switch).
    fn band_live(&self, b: usize) -> bool {
        let want = self.want_cfg();
        self.eng[self.active].cfg.has(b) || want.has(b)
    }

    /// One control tick: smoothers, detectors' laws, the bands' laws, the guard's cap, the stages.
    fn control(&mut self) {
        self.smooth_tick();
        let sr = self.sr as f64;
        let c = self.c;
        let k = self.coefs;
        // ---- envelope e(t) ∈ [0, 1]: 10 ms RMS in dB, attack 5 ms, release 120 ms
        let l = 10.0 * self.p_env.max(1e-20).log10();
        let et = ((l - ENV_FLOOR_DB) / ENV_SPAN_DB).clamp(0.0, 1.0);
        let a = if et > self.env { k.env_att } else { k.env_rel };
        self.env = et + (self.env - et) * a;
        let e = self.env;
        // ---- de-fizz: the HF share, its floor, the cut c ≤ 0 (dB)
        let defizz = c[p::DEFIZZ];
        let target = if defizz > 0.0 {
            let s = 10.0 * (self.p_hf.max(1e-30) / self.p_full.max(1e-30)).log10();
            let lhf = 10.0 * self.p_hf.max(1e-30).log10();
            let w = ((lhf - DEFIZZ_FLOOR.0) / (DEFIZZ_FLOOR.1 - DEFIZZ_FLOOR.0)).clamp(0.0, 1.0);
            -DEFIZZ_MAX_DB * defizz / 100.0 * w * ((s - DEFIZZ_SHARE_DB) / DEFIZZ_SHARE_SPAN).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let a = if target < self.defizz_c { k.fizz_att } else { k.fizz_rel };
        self.defizz_c = target + (self.defizz_c - target) * a;
        if self.defizz_c > -1e-6 && target == 0.0 {
            self.defizz_c = 0.0;
        }
        let cut = self.defizz_c;
        if cut != self.shelf_db {
            if self.shelf_db == 0.0 {
                // the shelf leaves rest (where it is the exact identity and does not run)
                self.shelf.iter_mut().for_each(|s| s.reset());
            }
            self.shelf_db = cut;
            for s in self.shelf.iter_mut() {
                s.set(SvfMode::HighShelf, DEFIZZ_HZ.min(0.4 * sr) as f32, std::f32::consts::FRAC_1_SQRT_2, cut as f32, sr as f32);
            }
        }
        // ---- the morph, the bands' drives and laws
        let mu = (c[p::MORPH] / 100.0 + c[p::ENV_MORPH] / 100.0 * e).clamp(0.0, 1.0);
        let mu0 = self.mu.v;
        self.mu.to(mu);
        let ring = c[p::RING_HZ];
        for b in 0..BANDS {
            let d = (DRIVE_DB_PER_PCT * c[p::DRIVE] + c[p::trim(b)] + c[p::ENV_DRIVE] * e + cut).clamp(0.0, types::MAX_DRIVE_DB);
            self.drive_db[b] = d;
            for s in 0..2 {
                let mut ty = self.frame.bands[b].slots[s].cur.ty;
                if !self.frame.bands[b].slots[s].fading && self.types[b][s] != ty {
                    ty = self.types[b][s];
                    self.start_fade(b, s);
                    self.cache[b][s] = (Type::Off, f64::NAN, f64::NAN);
                }
                if (ty, d, ring) != self.cache[b][s] {
                    self.cache[b][s] = (ty, d, ring);
                    self.frame.bands[b].slots[s].cur = SlotParams::new(ty, d, db_lin(makeup_db(ty, d)), ring);
                }
            }
            // ρ: the slow gain of the blend (1 while one slot runs), smoothed 20 ms
            let both = mu0.max(mu) > 0.0 && mu0.min(mu) < 1.0;
            let g = if both {
                let r = self.eng[self.active].rho(b).rho();
                rho_gain(mu, r)
            } else {
                1.0
            };
            self.rho_g[b] = g + (self.rho_g[b] - g) * k.rho;
            let bf = &mut self.frame.bands[b];
            bf.run = [mu0.min(mu) < 1.0, mu0.max(mu) > 0.0];
            let lv = if c[p::level(b)] <= -60.0 { 0.0 } else { db_lin(c[p::level(b)]) };
            let m = c[p::MIX] / 100.0;
            let r = &mut self.band_ramps[b];
            r[0].to(lv * (1.0 - m));
            r[1].to(lv * m);
            r[2].to(if both { self.rho_g[b] } else { 1.0 });
        }
        // crossovers: x_air held ≥ 2 × x_low
        let xl = c[p::X_LOW];
        self.frame.xf = [xl, c[p::X_AIR].max(2.0 * xl)];
        // ---- pre filters and tilt
        self.pre_hp.set(c[p::PRE_HP], self.v[p::PRE_HP] <= 20.0, self.sr);
        self.pre_lp.set(c[p::PRE_LP].min(0.45 * sr), self.v[p::PRE_LP] >= 20000.0, self.sr);
        let tilt = c[p::TILT];
        if tilt != 0.0 {
            // H = A (x − (1 − 1 / A²) LP(x)), LP at f0 · A: ±tilt / 2 at the extremes, 0 dB at f0
            let aa = db_lin(tilt / 2.0);
            let fp = (c[p::TILT_FREQ] * aa).min(0.45 * sr);
            self.tilt_k = (aa, 1.0 - 1.0 / (aa * aa), OnePoleTpt::coef(fp, sr));
            if !self.tilt_on {
                self.tilt.iter_mut().for_each(|t| t.reset());
            }
            self.tilt_on = true;
        } else {
            self.tilt_on = false;
        }
        // ---- box
        let (oa, ob) = self.objects();
        if (oa, ob) != self.obj_key {
            if self.obj_key.0 != usize::MAX {
                self.boxs.change_object();
            }
            self.obj_key = (oa, ob);
        }
        let mu_box = if ob == oa { 0.0 } else { mu };
        let bx = c[p::BOX] / 100.0;
        self.boxs.amount.to(bx);
        if self.ticks.is_multiple_of(BOX_TICKS) || self.boxs.key.0 == usize::MAX {
            self.boxs.update(oa, ob, mu_box, c[p::POSITION] / 100.0, c[p::SIZE] / 100.0, c[p::DECAY] / 100.0, (CTRL as u64 * BOX_TICKS) as u32);
        }
        // ---- comp
        let cp = c[p::COMP];
        self.comp.on = cp > 0.0;
        let (t, r) = comp_law(cp);
        self.comp.t = t;
        self.comp.r = r;
        self.comp.makeup = comp_makeup(cp);
        if self.comp.on {
            self.comp.tick();
            self.gr = self.comp.gr;
        } else {
            self.comp.g.jump(1.0);
            self.comp.y1 = 0.0;
            self.comp.yl = 0.0;
            self.gr = 0.0;
        }
        // ---- post filters: the low-pass moves with the envelope (octaves)
        self.post_hp.set(c[p::POST_HP], self.v[p::POST_HP] <= 20.0, self.sr);
        let fc = (c[p::POST_LP] * (c[p::ENV_FILTER] * e).exp2()).clamp(20.0, 0.45 * sr);
        let q = reso_q(c[p::RESO]);
        let engaged = c[p::RESO] > 0.0 || self.v[p::POST_LP] < 20000.0 || c[p::ENV_FILTER] < 0.0;
        self.post_on.set_target(if engaged { 1.0 } else { 0.0 });
        if (fc, q) != self.post_key {
            self.post_key = (fc, q);
            for s in self.post_lp.iter_mut() {
                s.set(SvfMode::LowPass, fc as f32, q as f32, 0.0, sr as f32);
            }
        }
        // ---- echo
        self.echo.time.set_target(c[p::ECHO_TIME] as f32);
        self.echo.fb.to(c[p::ECHO_FB] / 100.0);
        self.echo.mix.to(c[p::ECHO_MIX] / 100.0);
        // ---- level
        self.level.to(db_lin(c[p::LEVEL]));
        // ---- the guard: the cap, then the law with its hysteresis
        self.g_cap = self.cap(mu, engaged.then_some(q));
        let tg = c[p::GUARD];
        let target = if tg <= GUARD_OFF {
            self.guard_open = true;
            0.0
        } else {
            let lg = 10.0 * self.p_guard.max(1e-20).log10();
            if self.guard_open && lg < tg - GUARD_HYST_DB {
                self.guard_open = false;
            } else if !self.guard_open && lg >= tg {
                self.guard_open = true;
            }
            if self.guard_open {
                0.0
            } else {
                (self.g_cap + GUARD_MARGIN_DB) * ((tg - lg) / GUARD_KNEE_DB).clamp(0.0, 1.0)
            }
        };
        let a = if target < self.guard_cut { k.guard_att } else { k.guard_rel };
        self.guard_cut = target + (self.guard_cut - target) * a;
        if self.guard_cut < 1e-6 && target == 0.0 {
            self.guard_cut = 0.0;
        }
        self.guard_g.to(db_lin(-self.guard_cut));
        self.ticks += 1;
    }

    /// G_cap (README §2.2): the largest gain the pedal gives a small signal, at any frequency (dB ≥ 0).
    fn cap(&self, mu: f64, lp_q: Option<f64>) -> f64 {
        let c = &self.c;
        let m = c[p::MIX] / 100.0;
        let mut g_dirt = f64::NEG_INFINITY;
        for b in 0..BANDS {
            if !self.band_live(b) || c[p::level(b)] <= -60.0 {
                continue;
            }
            let d = self.drive_db[b];
            let [ta, tb] = [self.frame.bands[b].slots[0].cur.ty, self.frame.bands[b].slots[1].cur.ty];
            let gb = ((1.0 - mu) * db_lin(small_db(ta, d)) + mu * db_lin(small_db(tb, d))) * self.rho_g[b];
            let g = 20.0 * (1.0 - m + m * gb).max(1e-6).log10() + c[p::level(b)];
            g_dirt = g_dirt.max(g);
        }
        if g_dirt == f64::NEG_INFINITY {
            g_dirt = 0.0;
        }
        let mut g_post = c[p::TILT].abs() / 2.0;
        let bx = c[p::BOX] / 100.0;
        if bx > 0.0 {
            g_post += 20.0 * (1.0 - bx + bx * db_lin(self.boxs.peak_db)).log10();
        }
        if let Some(q) = lp_q {
            g_post += reso_peak_db(q);
        }
        g_post += comp_makeup(c[p::COMP]);
        let em = c[p::ECHO_MIX] / 100.0;
        if em > 0.0 {
            g_post += 20.0 * (1.0 + em / (1.0 - c[p::ECHO_FB] / 100.0)).log10();
        }
        (g_dirt + g_post).max(0.0)
    }

    fn settle(&mut self) {
        for (k, s) in self.sm.exp.iter_mut().enumerate() {
            s.reset();
            self.c[EXP_IDS[k]] = s.value() as f64;
        }
        for (i, s) in self.sm.one.iter_mut() {
            s.settle();
            let v = s.value() as f64;
            self.c[*i] = if LOG_IDS.contains(i) { v.exp2() } else { v };
        }
        for r in self.fades.iter_mut().flatten() {
            r.settle();
        }
        self.bypass.settle();
        self.order_g.settle();
        self.order_now = self.v[p::ORDER].round() as usize;
        self.mu.jump((self.c[p::MORPH] / 100.0).clamp(0.0, 1.0));
        self.rho_g = [1.0; BANDS];
        self.p_guard = 0.0;
        self.guard_cut = 0.0;
        self.guard_open = true;
        self.p_full = 0.0;
        self.p_hf = 0.0;
        self.defizz_c = 0.0;
        self.p_env = 0.0;
        self.env = 0.0;
        self.shelf_db = f64::NAN;
        self.post_key = (f64::NAN, f64::NAN);
        self.comp.y1 = 0.0;
        self.comp.yl = 0.0;
        self.gr = 0.0;
        self.comp.g.jump(1.0);
        for b in 0..BANDS {
            for s in 0..2 {
                self.cache[b][s].1 = f64::NAN;
            }
        }
        self.obj_key = self.objects();
        self.boxs.reset();
        self.boxs.amount.jump(self.c[p::BOX] / 100.0);
        self.echo.time.set_immediate(self.v[p::ECHO_TIME]);
        self.echo.fb.jump(self.c[p::ECHO_FB] / 100.0);
        self.echo.mix.jump(self.c[p::ECHO_MIX] / 100.0);
        self.level.jump(db_lin(self.c[p::LEVEL]));
        self.guard_g.jump(1.0);
        self.ticks = 0;
        // the first tick builds everything; the type fades it starts are settled at once
        self.control();
        for b in 0..BANDS {
            for s in 0..2 {
                let f = &mut self.frame.bands[b].slots[s];
                f.fading = false;
                f.fade = 1.0;
                f.old = f.cur;
            }
            for r in self.band_ramps[b].iter_mut() {
                r.jump(r.v + r.step * CTRL as f64);
            }
            self.frame.bands[b].run = [self.mu.v < 1.0, self.mu.v > 0.0];
        }
        self.mu.step = 0.0;
        self.boxs.amount.step = 0.0;
        self.echo.fb.step = 0.0;
        self.echo.mix.step = 0.0;
        self.level.step = 0.0;
        self.guard_g.step = 0.0;
        self.comp.g.jump(self.comp.g.v + self.comp.g.step * CTRL as f64);
        self.post_on.set_target(self.post_on_target());
        self.post_on.settle();
        self.post_hp.reset();
        self.pre_hp.reset();
        self.pre_lp.reset();
        self.post_lp.iter_mut().for_each(|s| s.reset());
        self.tilt.iter_mut().for_each(|t| t.reset());
        self.shelf.iter_mut().for_each(|s| s.reset());
        self.hf.iter_mut().for_each(|s| s.reset());
        self.phase = 1;
    }

    fn post_on_target(&self) -> f32 {
        if self.v[p::RESO] > 0.0 || self.v[p::POST_LP] < 20000.0 || self.v[p::ENV_FILTER] < 0.0 {
            1.0
        } else {
            0.0
        }
    }

    /// The guard's state (true = open), its cut (dB) and cap (dB) (tests and meters).
    pub fn guard_state(&self) -> (bool, f64, f64) {
        (self.guard_open, self.guard_cut, self.g_cap)
    }

    /// The de-fizz cut (dB, ≤ 0) and the envelope (0 – 1).
    pub fn defizz_db(&self) -> f64 {
        self.defizz_c
    }

    pub fn envelope(&self) -> f64 {
        self.env
    }

    /// The bands' drives (dB) at the last tick.
    pub fn drives(&self) -> [f64; BANDS] {
        self.drive_db
    }

    /// The object's current peak term (dB) and the bank's mode count.
    pub fn box_peak_db(&self) -> f64 {
        self.boxs.peak_db
    }

    /// The modes the bank was last set to.
    pub fn box_modes(&self) -> &[Mode] {
        self.boxs.list.as_slice()
    }

    /// The comp's gain reduction (dB, ≥ 0).
    pub fn gr_db(&self) -> f64 {
        self.gr
    }

    /// The ρ gains of the bands.
    pub fn rho_gains(&self) -> [f64; BANDS] {
        self.rho_g
    }

    /// The dirt stage of one base sample (engine, switch, de-fizz shelf).
    #[inline]
    fn dirt(&mut self, x: [f32; 2], nch: usize) -> [f32; 2] {
        if self.sw == Switch::Idle {
            let want = self.want_cfg();
            if want != self.eng[self.active].cfg {
                let o = 1 - self.active;
                let (a, b) = self.eng.split_at_mut(1);
                let (from, to) = if o == 1 { (&a[0], &mut b[0]) } else { (&b[0], &mut a[0]) };
                self.sw_power = want.bands != from.cfg.bands;
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
                // a split change crossfades two dispersions: equal power; a quality change two
                // near-identical signals: equal gain
                let (wo, wn) = if self.sw_power {
                    let a = 0.5 * PI * r;
                    (a.cos() as f32, a.sin() as f32)
                } else {
                    let w = (0.5 - 0.5 * (PI * r).cos()) as f32;
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
        if self.shelf_db != 0.0 {
            for ch in 0..nch {
                y[ch] = self.shelf[ch].process(y[ch]);
            }
        }
        y
    }
}

impl Default for Morph {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Morph {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate;
        self.nch = channels.clamp(1, 2);
        let cr = sample_rate / CTRL as f32;
        for s in self.sm.exp.iter_mut() {
            s.prepare(cr);
        }
        for (_, s) in self.sm.one.iter_mut() {
            s.prepare(cr);
        }
        for r in self.fades.iter_mut().flatten() {
            r.prepare(sample_rate);
        }
        self.bypass.prepare(sample_rate);
        self.order_g.prepare(sample_rate);
        self.post_on.prepare(sample_rate);
        for c in [&mut self.pre_hp, &mut self.pre_lp, &mut self.post_hp] {
            c.prepare(sample_rate);
        }
        self.boxs.prepare(sample_rate);
        self.echo.time.prepare(sample_rate);
        let max = (ECHO_MAX_MS * 0.001 * sample_rate as f64).ceil() as usize + 8;
        for l in self.echo.line.iter_mut() {
            l.prepare(max);
        }
        let sr = sample_rate as f64;
        self.echo.lp_g = OnePoleTpt::coef(ECHO_LP_HZ, sr);
        self.comp.att = gfx_dsp::util::one_pole_coef(COMP_ATTACK_MS * 0.001 * sr);
        self.comp.rel = gfx_dsp::util::one_pole_coef(COMP_RELEASE_MS * 0.001 * sr);
        self.coefs = Coefs {
            guard_rms: gfx_dsp::util::one_pole_coef(GUARD_RMS_MS * 0.001 * sr),
            guard_att: ctrl_coef(GUARD_ATTACK_MS, sr),
            guard_rel: ctrl_coef(GUARD_RELEASE_MS, sr),
            fizz_rms: gfx_dsp::util::one_pole_coef(DEFIZZ_RMS_MS * 0.001 * sr),
            fizz_att: ctrl_coef(DEFIZZ_ATTACK_MS, sr),
            fizz_rel: ctrl_coef(DEFIZZ_RELEASE_MS, sr),
            env_rms: gfx_dsp::util::one_pole_coef(ENV_RMS_MS * 0.001 * sr),
            env_att: ctrl_coef(ENV_ATTACK_MS, sr),
            env_rel: ctrl_coef(ENV_RELEASE_MS, sr),
            rho: ctrl_coef(20.0, sr),
        };
        for s in self.hf.iter_mut() {
            s.set(SvfMode::HighPass, DEFIZZ_HZ.min(0.4 * sr) as f32, std::f32::consts::FRAC_1_SQRT_2, 0.0, sample_rate);
        }
        self.preroll_len = (PREROLL_MS * 0.001 * sample_rate).round() as u32;
        self.fade_len = (SWITCH_FADE_MS * 0.001 * sample_rate).round().max(1.0) as u32;
        self.reset();
    }

    fn reset(&mut self) {
        self.clock = 0;
        self.set_targets();
        self.active = 0;
        self.sw = Switch::Idle;
        self.settle();
        let cfg = self.want_cfg();
        self.eng[0].configure(cfg, self.sr, &self.frame);
        self.eng[1].configure(cfg, self.sr, &self.frame);
        self.echo.line.iter_mut().for_each(|l| l.reset());
        self.echo.lp.iter_mut().for_each(|l| l.reset());
        self.echo.awake = false;
    }

    fn set_param(&mut self, id: &str, value: f32) {
        if let Some(i) = PARAMS.iter().position(|p| p.id == id) {
            let p = &PARAMS[i];
            self.v[i] = value.clamp(p.min, p.max);
            self.set_targets();
        }
    }

    fn meter(&self, id: &str) -> Option<f32> {
        match id {
            "gr" => Some(self.gr as f32),
            "state" => Some(if self.guard_open { 1.0 } else { 0.0 }),
            "defizz_gr" => Some(-self.defizz_c as f32),
            _ => None,
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(self.nch);
        if nch == 0 {
            return;
        }
        let n = buffers[0].len();
        let sr = self.sr as f64;
        let k = self.coefs;
        let inv = 1.0 / nch as f64;
        for i in 0..n {
            self.phase -= 1;
            if self.phase == 0 {
                self.phase = CTRL;
                self.control();
            }
            self.clock += 1;
            let x = [buffers[0][i], if nch > 1 { buffers[1][i] } else { 0.0 }];
            // bypass: the input fades out of the chain, whose tails ring on
            let bp = self.bypass.next();
            let mut u = x;
            if bp > 0.0 {
                for ch in 0..nch {
                    u[ch] *= 1.0 - bp;
                }
            }
            // detectors on the input: guard (5 ms) and envelope (10 ms), linked
            let mut p2 = 0.0;
            for ch in 0..nch {
                p2 += (u[ch] as f64) * (u[ch] as f64);
            }
            p2 *= inv;
            self.p_guard = p2 + (self.p_guard - p2) * k.guard_rms;
            self.p_env = p2 + (self.p_env - p2) * k.env_rms;
            // guard
            let g = self.guard_g.next() as f32;
            for ch in 0..nch {
                u[ch] *= g;
            }
            // pre filters
            self.pre_hp.process(&mut u, nch);
            self.pre_lp.process(&mut u, nch);
            // de-fizz detector: HF share after the pre filters, before the tilt (mono sum)
            if self.c[p::DEFIZZ] > 0.0 || self.defizz_c != 0.0 {
                let m = (0..nch).map(|ch| u[ch] as f64).sum::<f64>() * inv;
                let h0 = self.hf[0].process(m as f32);
                let h = self.hf[1].process(h0) as f64;
                self.p_full = m * m + (self.p_full - m * m) * k.fizz_rms;
                self.p_hf = h * h + (self.p_hf - h * h) * k.fizz_rms;
            }
            // tilt
            if self.tilt_on {
                let (aa, kk, gg) = self.tilt_k;
                for ch in 0..nch {
                    let v = u[ch] as f64;
                    let lp = self.tilt[ch].low(v, gg);
                    u[ch] = (aa * (v - kk * lp)) as f32;
                }
            }
            // the per-sample ramps of the dirt's frame
            let mu = self.mu.next();
            for b in 0..BANDS {
                let r = &mut self.band_ramps[b];
                let bf: &mut BandFrame = &mut self.frame.bands[b];
                bf.mu = mu;
                bf.a_dry = r[0].next();
                bf.a_wet = r[1].next();
                bf.g_rho = r[2].next();
            }
            for b in 0..BANDS {
                for s in 0..2 {
                    let f = &mut self.frame.bands[b].slots[s];
                    if f.fading {
                        let r = self.fades[b][s].next() as f64;
                        f.fade = 0.5 - 0.5 * (PI * r).cos();
                        if self.fades[b][s].is_settled() {
                            f.fading = false;
                            f.fade = 1.0;
                        }
                    }
                }
            }
            // order: Dirt → Box or Box → Dirt (a 10 ms fade out, the swap, a 10 ms fade in)
            let want = self.v[p::ORDER].round() as usize;
            if want != self.order_now && self.order_g.is_settled() {
                if self.boxs.awake {
                    if self.order_g.value() == 1.0 {
                        self.order_g.set_target(0.0);
                    } else {
                        self.order_now = want;
                        self.order_g.set_target(1.0);
                    }
                } else {
                    self.order_now = want;
                }
            }
            let og = self.order_g.next();
            let mut y = if self.order_now == 0 {
                let mut y = self.dirt(u, nch);
                self.boxs.process(&mut y, nch);
                y
            } else {
                self.boxs.process(&mut u, nch);
                self.dirt(u, nch)
            };
            if og != 1.0 {
                for ch in 0..nch {
                    y[ch] *= og;
                }
            }
            // comp, post filters, echo, level
            if self.comp.on {
                self.comp.process(&mut y, nch);
            }
            self.post_hp.process(&mut y, nch);
            let on = self.post_on.next();
            if on > 0.0 || !self.post_on.is_settled() {
                for ch in 0..nch {
                    let l = self.post_lp[ch].process(y[ch]);
                    y[ch] = if on == 1.0 { l } else { y[ch] + on * (l - y[ch]) };
                }
            }
            self.echo.process(&mut y, nch, sr);
            let lv = self.level.next() as f32;
            for ch in 0..nch {
                let out = y[ch] * lv;
                buffers[ch][i] = if bp == 0.0 { out } else { bp * x[ch] + out };
            }
        }
    }
}

/// The bands' names (low, body, air), for the tests and the plugin.
pub const BAND_NAMES: [&str; BANDS] = ["low", "body", "air"];
pub const BAND_INDEX: [usize; BANDS] = [LOW, BODY, AIR];
