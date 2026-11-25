// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-ringmod — golem's ring modulator (design: README.md).
//!
//! Stands for two golem blocks: `ringmod` (mda RingMod: ids `freq`, `fine`, `feedback`) and
//! `surge-treemonster` (Surge XT Treemonster: `thr`, `speed`, `lowcut`, `highcut`, `pitch`, `ring`,
//! `mix`), every catalogue id on its 0 – 1 scale with the original plugin's law (README §3).
//!
//! - **Carrier**: Free (f = 16,000 · `freq` + 100 · `fine` Hz, the mda law) or Track (the played note),
//!   times the interval `pitch` (−60 … +60 st, Equal or Just), times the LFO and the envelope in
//!   semitones. An f64 phase advanced by the instantaneous frequency; band-limited Sine / Triangle /
//!   Square (osc.rs).
//! - **Track**: YIN (gfx-dsp `detect`) on the input through `lowcut` / `highcut` names the note (the
//!   lowest note follows `lowcut`), an onset pre-estimate (two agreeing zero-crossing periods) answers
//!   first above 110 Hz, the glide follows Surge's `speed` law. With `lock`, the carrier then follows the
//!   gfx-dsp `track` loop on the unfiltered input: its frequency is the string's, and where 2 r is an
//!   integer its phase is set by `phase` against the fundamental's.
//! - **Modulator** at 2× (4× for Diode): Clean multiply (`drive` = ADAA1 on the tanh's residual), Parker's
//!   diode ring with ADAA1 per branch (`carrier_level`, `balance`), or one sideband through the gfx-dsp
//!   `ssb` pair; mda's feedback (loop delay fixed at 1 / 48,000 s, soft-limited); Treemonster's `ring`
//!   blend with a sine voice.
//! - **Output**: squelch under `thr`, DC blocker, wet low / high cut, linear `mix` against a dry through
//!   the same oversampler path, `bass_keep`, `level`. Zero latency reported.
//!
//! Real-time safe (tables, YIN and loops sized in `prepare`), deterministic.

#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

pub mod osc;

#[cfg(feature = "plugin")]
pub mod plugin;

#[cfg(test)]
mod tests;

use gfx_dsp::adaa::{Adaa1Fn, DiodePair, ParkerDiode, TanhResidual};
use gfx_dsp::detect::PitchDetector;
use gfx_dsp::envelope::{PeakFollower, SlidingMax};
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::lfo::Rng;
use gfx_dsp::oversample::{halfband_coefs, Oversampler2x, DEFAULT_COEFS, DEFAULT_TRANSITION};
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::ssb::{phase_dist, HilbertDesign, SsbShifter};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::track::{Engage, Tracker};
use gfx_dsp::transport::{Sync, TransportInfo};
use gfx_dsp::{Effect, ParamInfo};
use osc::Tables;
use std::f64::consts::PI;

pub const NP: usize = 34;

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::enumeration("carrier", "Carrier", 2, 0),
    ParamInfo::float("freq", "Freq", 0.0, 1.0, 0.025).with_exp(),
    ParamInfo::float("fine", "Fine", 0.0, 1.0, 0.5).with_exp(),
    ParamInfo::float("pitch", "Pitch", 0.0, 1.0, 0.5).with_exp(),
    ParamInfo::float("ring", "Ring Mod", 0.0, 1.0, 1.0).with_exp(),
    ParamInfo::float("feedback", "Feedback", 0.0, 1.0, 0.0).with_exp(),
    ParamInfo::float("mix", "Mix", 0.0, 1.0, 1.0).with_exp(),
    ParamInfo::float("thr", "Threshold", 0.0, 1.0, 0.3),
    ParamInfo::float("speed", "Speed", 0.0, 1.0, 0.5),
    ParamInfo::float("lowcut", "Low Cut", 0.0, 1.0, 0.3),
    ParamInfo::float("highcut", "High Cut", 0.0, 1.0, 0.7),
    ParamInfo::enumeration("mode", "Mode", 3, 0),
    ParamInfo::enumeration("model", "Model", 2, 0),
    ParamInfo::enumeration("wave", "Carrier Wave", 3, 0),
    ParamInfo::enumeration("tuning", "Tuning", 2, 1),
    ParamInfo::boolean("lock", "Phase Lock", true),
    ParamInfo::boolean("snap", "Snap", false),
    ParamInfo::float("phase", "Lock Phase", 0.0, 360.0, 90.0),
    ParamInfo::float("drive", "Drive", 0.0, 100.0, 0.0),
    ParamInfo::float("carrier_level", "Carrier Level", -12.0, 12.0, 0.0),
    ParamInfo::float("balance", "Balance", 0.0, 100.0, 0.0),
    ParamInfo::float("rate", "LFO Rate", 0.1, 25.0, 1.0).with_exp(),
    ParamInfo::float("lfo_depth", "LFO Depth", 0.0, 24.0, 0.0).with_exp(),
    ParamInfo::enumeration("shape", "LFO Shape", 4, 0),
    ParamInfo::enumeration("sync", "LFO Sync", 17, 0),
    ParamInfo::float("stereo_phase", "Stereo Phase", 0.0, 180.0, 0.0),
    ParamInfo::float("env_amount", "Env → Carrier", -48.0, 48.0, 0.0),
    ParamInfo::float("env_sens", "Env Sensitivity", 0.0, 100.0, 50.0),
    ParamInfo::float("env_att", "Env Attack", 1.0, 100.0, 5.0),
    ParamInfo::float("env_rel", "Env Release", 20.0, 2000.0, 200.0),
    ParamInfo::float("hpf", "Wet Low Cut", 0.0, 1000.0, 0.0),
    ParamInfo::float("lpf", "Wet High Cut", 1000.0, 20000.0, 20000.0),
    ParamInfo::float("bass_keep", "Bass Keep", 0.0, 500.0, 0.0),
    ParamInfo::float("level", "Level", -24.0, 12.0, 0.0),
];

pub mod p {
    pub const CARRIER: usize = 0;
    pub const FREQ: usize = 1;
    pub const FINE: usize = 2;
    pub const PITCH: usize = 3;
    pub const RING: usize = 4;
    pub const FEEDBACK: usize = 5;
    pub const MIX: usize = 6;
    pub const THR: usize = 7;
    pub const SPEED: usize = 8;
    pub const LOWCUT: usize = 9;
    pub const HIGHCUT: usize = 10;
    pub const MODE: usize = 11;
    pub const MODEL: usize = 12;
    pub const WAVE: usize = 13;
    pub const TUNING: usize = 14;
    pub const LOCK: usize = 15;
    pub const SNAP: usize = 16;
    pub const PHASE: usize = 17;
    pub const DRIVE: usize = 18;
    pub const CARRIER_LEVEL: usize = 19;
    pub const BALANCE: usize = 20;
    pub const RATE: usize = 21;
    pub const LFO_DEPTH: usize = 22;
    pub const SHAPE: usize = 23;
    pub const SYNC: usize = 24;
    pub const STEREO_PHASE: usize = 25;
    pub const ENV_AMOUNT: usize = 26;
    pub const ENV_SENS: usize = 27;
    pub const ENV_ATT: usize = 28;
    pub const ENV_REL: usize = 29;
    pub const HPF: usize = 30;
    pub const LPF: usize = 31;
    pub const BASS_KEEP: usize = 32;
    pub const LEVEL: usize = 33;
}

/// Enum options (orders final; append only).
pub const CARRIERS: [&str; 2] = ["Free", "Track"];
pub const MODES: [&str; 3] = ["Ring", "Upper", "Lower"];
pub const MODELS: [&str; 2] = ["Clean", "Diode"];
pub const TUNINGS: [&str; 2] = ["Equal", "Just"];
pub const SHAPES: [&str; 4] = ["Sine", "Triangle", "Square", "Random"];

// ---------------------------------------------------------------- the catalogue laws (README §3)

/// mda RingMod: the free carrier (Hz).
#[inline]
pub fn free_hz(freq: f64, fine: f64) -> f64 {
    16000.0 * freq + 100.0 * fine
}

/// Surge `pitch`: semitones.
#[inline]
pub fn pitch_st(v: f64) -> f64 {
    -60.0 + 120.0 * v
}

/// Surge `thr`: dBFS.
#[inline]
pub fn thr_db(v: f64) -> f64 {
    -96.0 + 96.0 * v
}

/// Surge `speed`: the glide's time constant (s), from its per-48 kHz-sample coefficient.
#[inline]
pub fn speed_tau(v: f64) -> f64 {
    1.0 / ((1e-4 + v.powi(4) * 0.0999 / 128.0) * 48000.0)
}

/// Surge `lowcut` / `highcut`: Hz (−60 … +70 st around 440 Hz).
#[inline]
pub fn cut_hz(v: f64) -> f64 {
    440.0 * 2f64.powf((-60.0 + 130.0 * v) / 12.0)
}

/// The just intervals of one octave: small denominators (products are harmonics of f0 / q).
pub const JUST: [f64; 12] = [1.0, 16.0 / 15.0, 9.0 / 8.0, 6.0 / 5.0, 5.0 / 4.0, 4.0 / 3.0, 7.0 / 5.0, 3.0 / 2.0, 8.0 / 5.0, 5.0 / 3.0, 9.0 / 5.0, 15.0 / 8.0];

/// The just ratio of an integer interval (semitones, any sign).
#[inline]
fn just_int(st: i64) -> f64 {
    JUST[st.rem_euclid(12) as usize] * 2f64.powi(st.div_euclid(12) as i32)
}

/// Just tuning, continuous: exact at integers, linear in cents between neighbours (cents).
#[inline]
pub fn just_cents(st: f64) -> f64 {
    let lo = st.floor();
    let t = st - lo;
    let a = 1200.0 * just_int(lo as i64).log2();
    if t == 0.0 {
        return a;
    }
    let b = 1200.0 * just_int(lo as i64 + 1).log2();
    a + (b - a) * t
}

// ---------------------------------------------------------------- constants

/// The analysis floor and ceiling of the tracker (Hz); `lowcut` / 1.75 sets the floor in 30 – 150.
const YIN_MIN: f32 = 30.0;
const YIN_MAX: f32 = 1500.0;
/// A YIN frame names the note when its clarity (1 − CMNDF) is at least this.
const CLARITY: f32 = 0.8;
/// The gate's hysteresis (dB) and its level window (s).
const GATE_HYST: f64 = 6.0;
const GATE_WINDOW_S: f64 = 0.035;
/// Squelch: linear ramps, open in 5 ms, close in 100 ms (to exactly 0).
const SQ_OPEN_MS: f64 = 5.0;
const SQ_CLOSE_MS: f64 = 100.0;
/// A target jump above this is a new note (cents); the glide has arrived within this (cents).
const NEW_NOTE_C: f64 = 50.0;
const ARRIVED_C: f64 = 30.0;
/// The handover from the glide to the locked note, and `snap`'s jump (ms).
const HAND_MS: f32 = 20.0;
const SNAP_MS: f64 = 10.0;
/// Onset pre-estimate: only above this (Hz); periods must agree within 3 %; window after the onset.
const PRE_MIN_HZ: f64 = 110.0;
const PRE_AGREE: f64 = 0.03;
const PRE_WINDOW_S: f64 = 0.04;
const PRE_HYST: f64 = 0.3;
/// The pick detector: +9 dB fast over slow, re-armed under +4.5 dB.
const PICK_RISE: f64 = 2.818_382_931_264_454;
const PICK_REARM: f64 = 1.678_804_018_122_56;
/// `phase`: the carrier is pulled onto its target at most this far in frequency (cents), with a
/// 50 ms time constant below that.
const PHASE_SLEW_C: f64 = 8.0;
const PHASE_TAU_S: f64 = 0.05;
/// Diode: Parker's law (vb, vl, h) and the `balance` mismatch at 100 %.
const VB: f64 = 0.2;
const VL: f64 = 0.4;
const MISMATCH: f64 = 0.25;
/// The SSB's gain: one sideband with the power of the ring's two.
const SSB_GAIN: f64 = std::f64::consts::FRAC_1_SQRT_2;
/// mda's feedback gain law and the loop's ceiling (transparent below −1 dBFS).
const FB_LAW: f64 = 0.95;
const LIM_KNEE: f64 = 0.891;
/// Control period (samples) of the filters' coefficients.
const CTRL: u64 = 16;
/// The wet's DC blocker (2-pole).
const DC_HZ: f32 = 20.0;

#[inline]
fn idx(v: f32, n: usize) -> usize {
    (v.round().max(0.0) as usize).min(n - 1)
}

#[inline]
fn cents(a: f64, b: f64) -> f64 {
    1200.0 * (a / b).log2()
}

/// The loop's soft ceiling: linear below −1 dBFS, ceiling 0 dBFS.
#[inline]
fn lim(v: f64) -> f64 {
    let a = v.abs();
    if a <= LIM_KNEE {
        v
    } else {
        (LIM_KNEE + (1.0 - LIM_KNEE) * ((a - LIM_KNEE) / (1.0 - LIM_KNEE)).tanh()).copysign(v)
    }
}

/// The up-sampler's phase at `hz` (turns, negative = late) for the first output of each pair: the
/// even-ranked all-passes at the input rate. The 4× path adds the second stage at twice the rate.
pub fn up_phase(hz: f64, sample_rate: f64) -> f64 {
    let c = halfband_coefs(DEFAULT_COEFS, DEFAULT_TRANSITION);
    let w = 2.0 * PI * hz / sample_rate;
    let (cw, sw) = (w.cos(), -w.sin());
    let mut ph = 0.0;
    for a in c.iter().step_by(2) {
        // arg (a + e^{−jω}) − arg (1 + a e^{−jω})
        let n = sw.atan2(a + cw);
        let d = (a * sw).atan2(1.0 + a * cw);
        ph += n - d;
    }
    ph / (2.0 * PI)
}

/// A value derived from two inputs, recomputed only when they change (transcendentals per sample
/// only while a parameter moves).
#[derive(Clone, Copy, Debug)]
struct Memo {
    k: [f64; 2],
    v: f64,
}

impl Memo {
    fn new() -> Self {
        Self { k: [f64::NAN; 2], v: 0.0 }
    }
    #[inline]
    fn get(&mut self, a: f64, b: f64, f: impl FnOnce(f64, f64) -> f64) -> f64 {
        if a != self.k[0] || b != self.k[1] {
            self.k = [a, b];
            self.v = f(a, b);
        }
        self.v
    }
}

/// The interval ratio from `pitch`, Equal ↔ Just crossfaded in cents (cents).
#[inline]
pub fn interval_cents(pitch: f64, just_x: f64) -> f64 {
    let st = pitch_st(pitch);
    100.0 * st * (1.0 - just_x) + just_cents(st) * just_x
}

/// The feedback loop's fixed delay of 1 / 48,000 s at the oversampled rate: a 4-point Lagrange read
/// on nodes ≥ 1 sample (the loop needs the previous output at least).
#[derive(Clone, Copy, Debug, Default)]
struct FbLoop {
    buf: [f64; 16],
    at: usize,
    nodes: [usize; 4],
    w: [f64; 4],
}

impl FbLoop {
    fn prepare(&mut self, os_rate: f64) {
        let d = os_rate / 48000.0;
        let n0 = ((d.floor() as usize).saturating_sub(1)).max(1);
        for j in 0..4 {
            self.nodes[j] = n0 + j;
        }
        for j in 0..4 {
            let mut w = 1.0;
            for m in 0..4 {
                if m != j {
                    w *= (d - self.nodes[m] as f64) / (self.nodes[j] as f64 - self.nodes[m] as f64);
                }
            }
            self.w[j] = w;
        }
        self.reset();
    }

    fn reset(&mut self) {
        self.buf = [0.0; 16];
        self.at = 0;
    }

    #[inline]
    fn read(&self) -> f64 {
        let mut y = 0.0;
        for j in 0..4 {
            y += self.w[j] * self.buf[(self.at + 17 - self.nodes[j]) & 15];
        }
        y
    }

    #[inline]
    fn push(&mut self, y: f64) {
        self.at = (self.at + 1) & 15;
        self.buf[self.at] = y;
    }
}

/// A crossfade between the options of an enum (20 ms).
#[derive(Clone, Debug)]
struct Xfade {
    from: usize,
    to: usize,
    x: LinearRamp,
}

impl Xfade {
    fn new(i: usize) -> Self {
        Self { from: i, to: i, x: LinearRamp::new(20.0, 1.0) }
    }
    fn prepare(&mut self, sr: f32) {
        self.x.prepare(sr);
    }
    fn set(&mut self, i: usize) {
        if i == self.to {
            return;
        }
        self.from = if self.x.value() >= 0.5 { self.to } else { self.from };
        self.to = i;
        self.x.set_immediate(0.0);
        self.x.set_target(1.0);
    }
    fn immediate(&mut self, i: usize) {
        self.from = i;
        self.to = i;
        self.x.set_immediate(1.0);
    }
    #[inline]
    fn next(&mut self) -> f32 {
        self.x.next()
    }
    /// The weight of option `k` (after `next`).
    #[inline]
    fn w(&self, k: usize) -> f32 {
        let x = self.x.value();
        let mut w = 0.0;
        if self.from == k {
            w += 1.0 - x;
        }
        if self.to == k {
            w += x;
        }
        w
    }
}

/// The LFO: phase in f64; Random = seeded sample-and-hold, slewed 2 ms by the caller.
#[derive(Clone, Debug)]
struct ModLfo {
    phase: f64,
    rng: Rng,
    held: f32,
    out: f32,
}

impl ModLfo {
    fn new() -> Self {
        Self { phase: 0.0, rng: Rng::new(0x41_4E_47_52), held: 0.0, out: 0.0 }
    }
    fn reset(&mut self) {
        *self = Self::new();
    }
    fn set_phase(&mut self, phase: f64) {
        if phase < self.phase {
            self.held = self.rng.bipolar();
        }
        self.phase = phase;
    }
    fn raw(&self, shape: usize) -> f32 {
        let p = self.phase as f32;
        match shape {
            0 => (2.0 * std::f32::consts::PI * p).sin(),
            1 => {
                // in phase with the sine: 0 at 0, +1 at ¼
                let q = (p + 0.25).rem_euclid(1.0);
                1.0 - 4.0 * (q - 0.5).abs()
            }
            2 => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            _ => self.held,
        }
    }
}

/// One channel's audio path.
#[derive(Clone, Debug)]
struct Chan {
    up1: Oversampler2x,
    up2: Oversampler2x,
    dn_a: Oversampler2x,
    dn_b2: Oversampler2x,
    dn_b1: Oversampler2x,
    /// the dry's down-samplers (its up-samplers are the wet's)
    dd2: Oversampler2x,
    dd4a: Oversampler2x,
    dd4b: Oversampler2x,
    dry_live: bool,
    drive: Adaa1Fn,
    dio_a: Adaa1Fn,
    dio_b: Adaa1Fn,
    fb2: FbLoop,
    fb4: FbLoop,
    ssb: SsbShifter,
    dc: Svf,
    hp: Svf,
    lp: Svf,
    hp_live: bool,
    lp_live: bool,
    bk_hi: [Svf; 2],
    clean_live: bool,
    diode_live: bool,
    ssb_live: bool,
}

impl Chan {
    fn new() -> Self {
        Self {
            up1: Oversampler2x::new(),
            up2: Oversampler2x::new(),
            dn_a: Oversampler2x::new(),
            dn_b2: Oversampler2x::new(),
            dn_b1: Oversampler2x::new(),
            dd2: Oversampler2x::new(),
            dd4a: Oversampler2x::new(),
            dd4b: Oversampler2x::new(),
            dry_live: false,
            drive: Adaa1Fn::default(),
            dio_a: Adaa1Fn::default(),
            dio_b: Adaa1Fn::default(),
            fb2: FbLoop::default(),
            fb4: FbLoop::default(),
            ssb: SsbShifter::default(),
            dc: Svf::new(),
            hp: Svf::new(),
            lp: Svf::new(),
            hp_live: false,
            lp_live: false,
            bk_hi: [Svf::new(), Svf::new()],
            clean_live: false,
            diode_live: false,
            ssb_live: false,
        }
    }

    fn reset(&mut self) {
        for o in [&mut self.up1, &mut self.up2, &mut self.dn_a, &mut self.dn_b2, &mut self.dn_b1, &mut self.dd2, &mut self.dd4a, &mut self.dd4b] {
            o.reset();
        }
        self.dry_live = false;
        self.drive.reset();
        self.dio_a.reset();
        self.dio_b.reset();
        self.fb2.reset();
        self.fb4.reset();
        self.ssb.reset();
        let [b0, b1] = &mut self.bk_hi;
        for f in [&mut self.dc, &mut self.hp, &mut self.lp, b0, b1] {
            f.reset();
        }
        self.hp_live = false;
        self.lp_live = false;
        self.clean_live = false;
        self.diode_live = false;
        self.ssb_live = false;
    }
}

/// The carrier of one channel for one host sample: its values at the 4× sub-samples (the 2× ones are
/// 0 and 2), and (cos, sin) at the 2× sub-samples for the sidebands.
#[derive(Clone, Copy, Debug, Default)]
struct CarrierFrame {
    c4: [f64; 4],
    cs2: [(f64, f64); 2],
}

/// Per-sample controls shared by the channels.
#[derive(Clone, Copy, Debug)]
struct Ctl {
    w_clean: f64,
    w_diode: f64,
    w_ssb: f64,
    balance: f64,
    ring: f64,
    voice: f64,
    fb: f64,
    drive_k: f64,
    drive_w: f64,
    diode_gain: f64,
    diode_norm: f64,
    cl: f64,
    pair_a: DiodePair,
    dry_on: bool,
}

pub struct RingMod {
    v: [f32; NP],
    sr: f32,
    nch: usize,
    transport: TransportInfo,
    tables: Tables,
    design: HilbertDesign,
    ch: [Chan; 2],
    // smoothers
    freq: ExpSmoother,
    fine: ExpSmoother,
    pitch: ExpSmoother,
    ring: ExpSmoother,
    feedback: ExpSmoother,
    mix: ExpSmoother,
    rate: ExpSmoother,
    lfo_depth: ExpSmoother,
    phase: OnePole,
    drive: OnePole,
    carrier_level: OnePole,
    balance: OnePole,
    stereo_phase: OnePole,
    env_amount: OnePole,
    hpf: OnePole,
    lpf: OnePole,
    bass_keep: OnePole,
    level: OnePole,
    lowcut: OnePole,
    highcut: OnePole,
    free_log: f64,
    free_a: f64,
    r_cents: f64,
    /// cached derivations: [interval, ratio, free target, free Hz, carrier level, level, drive k, pair]
    memo: [Memo; 8],
    carrier_x: LinearRamp,
    mode_x: Xfade,
    model_x: LinearRamp,
    wave_x: Xfade,
    tuning_x: LinearRamp,
    hp_x: LinearRamp,
    lp_x: LinearRamp,
    bk_x: LinearRamp,
    // modulation
    lfo: ModLfo,
    lfo_phase: f64,
    lfo_k: f32,
    env: PeakFollower,
    env_out: f32,
    // detection
    voice_env: PeakFollower,
    /// the gate's level: the input's maximum over 35 ms (one period of 30 Hz: no ripple, and silence
    /// is seen 35 ms after the input stops)
    gate_env: SlidingMax,
    gate_open: bool,
    gate_lin: (f64, f64),
    squelch: f64,
    sq_open: f64,
    sq_close: f64,
    det_lc: Svf,
    det_hc: Svf,
    yin: PitchDetector,
    f_min: f32,
    tracker: Tracker,
    // onset pre-estimate
    pk_fast: f64,
    det_lvl: f64,
    pk_slow: f64,
    pk_slow_att: f64,
    pk_armed: bool,
    pk_refr: u32,
    pk_a: (f64, f64, f64),
    pre_left: u32,
    pre_x1: f64,
    pre_low: bool,
    pre_t: [f64; 3],
    pre_n: usize,
    yin_mute: u32,
    sample: u64,
    // track carrier state
    target_hz: f64,
    glide_log: f64,
    glide_a: f64,
    snap_left: u32,
    arrived: bool,
    hand: LinearRamp,
    track_hz: f64,
    // carrier phases (turns) and the fundamental's phase mod 2
    ph_free: f64,
    ph_track: f64,
    psi2: f64,
    prev_psi: f64,
    // dry low band (bass_keep), shared and mono
    bk_lo: [Svf; 2],
    bk_set: f32,
    // compensation of the oversamplers' phase at the tracked note (turns), cached per note
    comp_hz: f64,
    comp2: f64,
    comp4: f64,
    diode_norm: f64,
    // meters
    carrier_hz: f32,
    state: u8,
    clock: u64,
}

impl Default for RingMod {
    fn default() -> Self {
        Self::new()
    }
}

/// The fundamental Fourier coefficient of G′(sin θ) for matched diodes at a unit carrier: the gain of
/// the Diode ring on a small input, so its sidebands match Clean's.
pub fn diode_gain_b1() -> f64 {
    let n = 8192;
    let mut s = 0.0;
    for i in 0..n {
        let th = 2.0 * PI * (i as f64 + 0.5) / n as f64;
        let c = th.sin();
        // G′(c) = sign(c) · D′(|c|)
        let a = c.abs();
        let dd = if a <= VB {
            0.0
        } else if a <= VL {
            (a - VB) / (VL - VB)
        } else {
            1.0
        };
        s += dd.copysign(c) * th.sin();
    }
    2.0 * s / n as f64
}

impl RingMod {
    pub fn new() -> Self {
        let v: [f32; NP] = core::array::from_fn(|i| PARAMS[i].default);
        let es = |i: usize| ExpSmoother::new(PARAMS[i].min, PARAMS[i].max, v[i]);
        let op = |i: usize, snap: f32| OnePole::new(20.0, v[i], snap);
        Self {
            v,
            sr: 48000.0,
            nch: 1,
            transport: TransportInfo::default(),
            tables: Tables::default(),
            design: HilbertDesign::for_rate(96000.0),
            ch: [Chan::new(), Chan::new()],
            freq: es(p::FREQ),
            fine: es(p::FINE),
            pitch: es(p::PITCH),
            ring: es(p::RING),
            feedback: es(p::FEEDBACK),
            mix: es(p::MIX),
            rate: es(p::RATE),
            lfo_depth: es(p::LFO_DEPTH),
            phase: op(p::PHASE, 1e-3),
            drive: op(p::DRIVE, 1e-3),
            carrier_level: op(p::CARRIER_LEVEL, 1e-3),
            balance: op(p::BALANCE, 1e-3),
            stereo_phase: op(p::STEREO_PHASE, 1e-3),
            env_amount: op(p::ENV_AMOUNT, 1e-4),
            hpf: op(p::HPF, 1e-2),
            lpf: op(p::LPF, 1e-1),
            bass_keep: op(p::BASS_KEEP, 1e-2),
            level: op(p::LEVEL, 1e-4),
            lowcut: op(p::LOWCUT, 1e-5),
            highcut: op(p::HIGHCUT, 1e-5),
            free_log: 0.0,
            free_a: 0.0,
            r_cents: 0.0,
            memo: [Memo::new(); 8],
            carrier_x: LinearRamp::new(20.0, 0.0),
            mode_x: Xfade::new(0),
            model_x: LinearRamp::new(20.0, 0.0),
            wave_x: Xfade::new(0),
            tuning_x: LinearRamp::new(20.0, 1.0),
            hp_x: LinearRamp::new(20.0, 0.0),
            lp_x: LinearRamp::new(20.0, 0.0),
            bk_x: LinearRamp::new(20.0, 0.0),
            lfo: ModLfo::new(),
            lfo_phase: 0.0,
            lfo_k: 0.0,
            env: PeakFollower::new(5.0, 200.0),
            env_out: 0.0,
            voice_env: PeakFollower::new(5.0, 500.0),
            gate_env: SlidingMax::new(),
            gate_open: false,
            gate_lin: (0.0, 0.0),
            squelch: 0.0,
            sq_open: 0.0,
            sq_close: 0.0,
            det_lc: Svf::new(),
            det_hc: Svf::new(),
            yin: PitchDetector::default(),
            f_min: 75.0,
            tracker: Tracker::new(),
            pk_fast: 0.0,
            det_lvl: 0.0,
            pk_slow: 0.0,
            pk_slow_att: 0.0,
            pk_armed: true,
            pk_refr: 0,
            pk_a: (0.0, 0.0, 0.0),
            pre_left: 0,
            pre_x1: 0.0,
            pre_low: false,
            pre_t: [0.0; 3],
            pre_n: 0,
            yin_mute: 0,
            sample: 0,
            target_hz: 0.0,
            glide_log: 0.0,
            glide_a: 0.0,
            snap_left: 0,
            arrived: false,
            hand: LinearRamp::new(HAND_MS, 0.0),
            track_hz: 0.0,
            ph_free: 0.0,
            ph_track: 0.0,
            psi2: 0.0,
            prev_psi: 0.0,
            bk_lo: [Svf::new(), Svf::new()],
            bk_set: -1.0,
            comp_hz: -1.0,
            comp2: 0.0,
            comp4: 0.0,
            diode_norm: 1.0 / diode_gain_b1(),
            carrier_hz: 0.0,
            state: 0,
            clock: 0,
        }
    }

    fn bool_v(&self, i: usize) -> bool {
        self.v[i] >= 0.5
    }

    /// The carrier's frequency now (Hz), for the read-only meter.
    pub fn carrier_hz(&self) -> f32 {
        self.carrier_hz
    }

    /// 0 = free or holding, 1 = tracking, 2 = locked (read-only meter `state`).
    pub fn state(&self) -> u8 {
        self.state
    }

    /// The tracked note (Hz, 0 before any) and the tracker's lock: for tests and the bench's trace.
    pub fn tracked(&self) -> (f64, bool) {
        (self.track_hz, self.tracker.reference().locked)
    }

    /// The Track carrier's phase against its `phase` target (turns): for tests.
    pub fn track_phase(&self) -> (f64, f64) {
        (self.ph_track, self.psi2)
    }

    fn apply(&mut self, i: usize) {
        let v = self.v[i];
        let now = self.clock;
        match i {
            p::FREQ => self.freq.set_target(v, now),
            p::FINE => self.fine.set_target(v, now),
            p::PITCH => self.pitch.set_target(v, now),
            p::RING => self.ring.set_target(v, now),
            p::FEEDBACK => self.feedback.set_target(v, now),
            p::MIX => self.mix.set_target(v, now),
            p::RATE => self.rate.set_target(v, now),
            p::LFO_DEPTH => self.lfo_depth.set_target(v, now),
            p::PHASE => self.phase.set_target(v),
            p::DRIVE => self.drive.set_target(v),
            p::CARRIER_LEVEL => self.carrier_level.set_target(v),
            p::BALANCE => self.balance.set_target(v),
            p::STEREO_PHASE => self.stereo_phase.set_target(v),
            p::ENV_AMOUNT => self.env_amount.set_target(v),
            p::HPF => {
                self.hpf.set_target(v);
                self.hp_x.set_target(if v > 0.5 { 1.0 } else { 0.0 });
            }
            p::LPF => {
                self.lpf.set_target(v);
                self.lp_x.set_target(if v < 19999.0 { 1.0 } else { 0.0 });
            }
            p::BASS_KEEP => {
                self.bass_keep.set_target(v.max(20.0));
                self.bk_x.set_target(if v > 0.5 { 1.0 } else { 0.0 });
            }
            p::LEVEL => self.level.set_target(v),
            p::LOWCUT => self.lowcut.set_target(v),
            p::HIGHCUT => self.highcut.set_target(v),
            p::CARRIER => self.carrier_x.set_target(idx(v, 2) as f32),
            p::MODE => self.mode_x.set(idx(v, 3)),
            p::MODEL => self.model_x.set_target(idx(v, 2) as f32),
            p::WAVE => self.wave_x.set(idx(v, 3)),
            p::TUNING => self.tuning_x.set_target(idx(v, 2) as f32),
            p::SPEED => self.set_glide(),
            p::ENV_ATT | p::ENV_REL => self.env.set_times(self.v[p::ENV_ATT], self.v[p::ENV_REL], self.sr),
            p::THR => self.tracker.set_floor(thr_db(v as f64) as f32),
            _ => {}
        }
    }

    fn set_glide(&mut self) {
        let tau = speed_tau(self.v[p::SPEED] as f64);
        self.glide_a = 1.0 - (-1.0 / (tau * self.sr as f64)).exp();
    }

    /// Detector filters, the lowest tracked note, the wet filters, the bass split (control rate).
    fn set_filters(&mut self, force: bool) {
        let sr = self.sr;
        let q = std::f32::consts::FRAC_1_SQRT_2;
        if force || !self.lowcut.is_settled() || !self.highcut.is_settled() {
            let lc = cut_hz(self.lowcut.value() as f64).min(0.45 * sr as f64) as f32;
            let hc = cut_hz(self.highcut.value() as f64).min(0.45 * sr as f64) as f32;
            self.det_lc.set(SvfMode::HighPass, lc, q, 0.0, sr);
            self.det_hc.set(SvfMode::LowPass, hc, q, 0.0, sr);
            let f_min = (lc / 1.75).clamp(YIN_MIN, 150.0);
            if (f_min - self.f_min).abs() > 0.5 || force {
                self.f_min = f_min;
                self.yin.set_range(f_min, YIN_MAX);
            }
        }
        if force || !self.hpf.is_settled() || !self.lpf.is_settled() {
            let hp = self.hpf.value().max(10.0);
            let lp = self.lpf.value().min(0.45 * sr);
            for c in self.ch.iter_mut() {
                c.hp.set(SvfMode::HighPass, hp, q, 0.0, sr);
                c.lp.set(SvfMode::LowPass, lp, q, 0.0, sr);
            }
        }
        let bk = self.bass_keep.value();
        if force || (bk - self.bk_set).abs() > 0.05 {
            self.bk_set = bk;
            for f in self.bk_lo.iter_mut() {
                f.set(SvfMode::LowPass, bk, q, 0.0, sr);
            }
            for c in self.ch.iter_mut() {
                for f in c.bk_hi.iter_mut() {
                    f.set(SvfMode::HighPass, bk, q, 0.0, sr);
                }
            }
        }
    }

    /// The oversamplers' phase at the tracked note (turns), cached.
    fn compensation(&mut self, hz: f64) -> (f64, f64) {
        if (hz - self.comp_hz).abs() > 0.05 * hz.max(1.0) || self.comp_hz < 0.0 {
            let sr = self.sr as f64;
            self.comp_hz = hz;
            self.comp2 = up_phase(hz, sr);
            self.comp4 = self.comp2 + up_phase(hz, 2.0 * sr);
        }
        (self.comp2, self.comp4)
    }

    /// One detector sample: gate, YIN, onset pre-estimate, tracker. Returns the voice envelope.
    #[inline]
    fn detect(&mut self, x: f32, peak: f32) -> f32 {
        let sr = self.sr as f64;
        self.sample += 1;
        // gate (unfiltered input) with hysteresis; `thr` 0 never closes (the bleed stays)
        let g = self.gate_env.next(peak) as f64;
        if self.v[p::THR] <= 0.0 {
            self.gate_open = true;
        } else if self.gate_open {
            if g < self.gate_lin.1 {
                self.gate_open = false;
            }
        } else if g > self.gate_lin.0 {
            self.gate_open = true;
        }
        let ve = self.voice_env.next(peak);
        let track = self.v[p::CARRIER] >= 0.5 || !self.carrier_x.is_settled();
        if !track {
            return ve;
        }
        // the detector's filters feed YIN and the onset pre-estimate only
        let xd = self.det_hc.process(self.det_lc.process(x)) as f64;
        let fresh = self.yin.push(xd as f32);
        // onset: fast / slow envelope difference on the unfiltered input (through `lowcut`'s skirt a
        // low note's vibrato would swing the level and read as picks)
        let a = peak as f64;
        let (k_fa, k_fr, k_s) = self.pk_a;
        let ad = xd.abs();
        let kd = if ad > self.det_lvl { k_fa } else { k_fr };
        self.det_lvl = ad + (self.det_lvl - ad) * kd;
        let kf = if a > self.pk_fast { k_fa } else { k_fr };
        self.pk_fast = a + (self.pk_fast - a) * kf;
        // the slow one is a peak follower too (10 ms up, 200 ms down): a steady note's crest factor
        // never reads as a pick, a new pick rises over the decayed level
        let ks = if a > self.pk_slow { self.pk_slow_att } else { k_s };
        self.pk_slow = a + (self.pk_slow - a) * ks;
        // in linear terms: fast over slow by 9 dB (re-armed under 4.5 dB), fast above `thr`
        self.pk_refr = self.pk_refr.saturating_sub(1);
        if self.pk_armed && self.pk_refr == 0 && self.pk_fast > PICK_RISE * self.pk_slow && self.pk_fast > self.gate_lin.0 {
            self.pk_armed = false;
            self.pk_refr = (0.04 * sr) as u32;
            self.pre_left = (PRE_WINDOW_S * sr) as u32;
            self.pre_n = 0;
            self.pre_low = false;
            // a pick: the loop lets go of the old note and holds its 150 ms mean until the new note is
            // named (else it follows the attack's transient wherever it pulls, up to its ±1 st clamp)
            self.tracker.hold();
        } else if !self.pk_armed && self.pk_fast < PICK_REARM * self.pk_slow {
            self.pk_armed = true;
        }
        // zero-crossing periods after the onset: two agreeing periods name the note early
        if self.pre_left > 0 {
            self.pre_left -= 1;
            // a Schmitt trigger: armed under −h, a crossing counts when the signal then rises through 0
            // (h = 30 % of the attack's level: the harmonics' small wiggles do not count)
            let h = PRE_HYST * self.det_lvl;
            if xd < -h {
                self.pre_low = true;
            }
            if self.pre_low && self.pre_x1 < 0.0 && xd >= 0.0 {
                self.pre_low = false;
                let t = self.sample as f64 - 1.0 + (-self.pre_x1) / (xd - self.pre_x1);
                self.pre_t = [self.pre_t[1], self.pre_t[2], t];
                self.pre_n += 1;
                if self.pre_n >= 3 {
                    let (p1, p2) = (self.pre_t[1] - self.pre_t[0], self.pre_t[2] - self.pre_t[1]);
                    let pm = 0.5 * (p1 + p2);
                    let f = sr / pm;
                    if (p1 - p2).abs() <= PRE_AGREE * pm && f >= PRE_MIN_HZ.max(self.f_min as f64) && f <= YIN_MAX as f64 && self.gate_open {
                        self.set_target(f);
                        self.pre_left = 0;
                        // YIN's frames still hold the old note for one window
                        self.yin_mute = (2.0 / self.f_min as f64 * sr) as u32;
                    }
                }
            }
        }
        self.pre_x1 = xd;
        self.yin_mute = self.yin_mute.saturating_sub(1);
        if fresh && self.yin_mute == 0 && self.gate_open {
            if let Some(hz) = self.yin.hz() {
                if self.yin.clarity() >= CLARITY {
                    self.set_target(hz as f64);
                }
            }
        }
        ve
    }

    /// A confident note: the glide's target and the tracker's feed-forward.
    fn set_target(&mut self, hz: f64) {
        let new_note = self.target_hz <= 0.0 || cents(hz, self.target_hz).abs() > NEW_NOTE_C;
        if self.target_hz <= 0.0 {
            // the first note: the carrier starts on it
            self.glide_log = hz.log2();
        } else if new_note {
            if self.bool_v(p::SNAP) {
                self.snap_left = (SNAP_MS * 1e-3 * self.sr as f64).max(1.0) as u32;
            }
            if self.arrived {
                // the glide starts from where the carrier is
                self.glide_log = self.track_hz.max(1.0).log2();
                self.arrived = false;
                self.hand.set_immediate(0.0);
            }
        }
        self.target_hz = hz;
        self.tracker.resume();
        self.tracker.set_f0(hz as f32, true);
    }

    /// The Track carrier's base frequency for this sample (Hz) and whether it is the locked note.
    #[inline]
    fn track_base(&mut self, lock: bool) -> (f64, bool) {
        if self.target_hz <= 0.0 {
            return (0.0, false);
        }
        let sr = self.sr as f64;
        let tr = self.tracker.reference();
        let t_hz = if lock && tr.inc > 0.0 { tr.inc * sr } else { self.target_hz };
        if lock && self.arrived && self.snap_left == 0 && self.hand.is_settled() && self.hand.value() >= 1.0 {
            // on the note: the loop's frequency itself, no glide to compute
            self.track_hz = t_hz;
            return (t_hz, tr.locked);
        }
        let t_log = t_hz.log2();
        if self.snap_left > 0 {
            // `snap`: the new note is reached in 10 ms, a straight line in log frequency
            self.glide_log += (t_log - self.glide_log) / self.snap_left as f64;
            self.snap_left -= 1;
        } else {
            self.glide_log += self.glide_a * (t_log - self.glide_log);
        }
        if !lock {
            self.arrived = false;
            self.hand.set_immediate(0.0);
            self.track_hz = 2f64.powf(self.glide_log);
            return (self.track_hz, false);
        }
        if !self.arrived && (t_log - self.glide_log).abs() * 1200.0 < ARRIVED_C {
            self.arrived = true;
            self.hand.set_target(1.0);
        }
        let h = self.hand.next() as f64;
        let out = if h >= 1.0 { (t_hz, tr.locked) } else { (2f64.powf(self.glide_log + h * (t_log - self.glide_log)), false) };
        self.track_hz = out.0;
        out
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(self.nch).min(2);
        if nch == 0 {
            return;
        }
        let n = buffers[0].len();
        let sr = self.sr;
        let srd = sr as f64;
        let v = self.v;
        let shape = idx(v[p::SHAPE], 4);
        let beats = Sync::from_index(idx(v[p::SYNC], 17)).beats();
        let sens = v[p::ENV_SENS] * 0.01;
        let lock = self.bool_v(p::LOCK);
        let fmax = (20000.0f64).min(0.45 * srd);
        let thr_on = v[p::THR] > 0.0;
        let lfo_live = v[p::LFO_DEPTH] != 0.0 || !self.lfo_depth.is_settled();
        let env_live = v[p::ENV_AMOUNT] != 0.0 || !self.env_amount.is_settled();
        // the gate's thresholds, linear (open above `thr`, close 6 dB under it)
        let thr = thr_db(v[p::THR] as f64);
        self.gate_lin = (10f64.powf(thr / 20.0), 10f64.powf((thr - GATE_HYST) / 20.0));

        for i in 0..n {
            self.clock += 1;
            // ---- modulation
            let rate = self.rate.next() as f64;
            let prev = self.lfo_phase;
            self.lfo_phase = match beats.and_then(|b| self.transport.phase_at(i, b, 0.0, sr)) {
                Some(ph) => ph,
                None => {
                    let hz = match beats {
                        Some(b) => self.transport.bpm() / 60.0 / b,
                        None => rate,
                    };
                    (prev + hz / srd).rem_euclid(1.0)
                }
            };
            self.lfo.set_phase(self.lfo_phase);
            // the LFO's value only while it has a depth (its phase always runs)
            if lfo_live {
                let raw = self.lfo.raw(shape);
                self.lfo.out = if shape == 3 { self.lfo.out + (raw - self.lfo.out) * self.lfo_k } else { raw };
            }
            let mut mono = 0.0f32;
            let mut peak = 0.0f32;
            for c in 0..nch {
                mono += buffers[c][i];
                peak = peak.max(buffers[c][i].abs());
            }
            mono /= nch as f32;
            let e = self.env.next(peak);
            if env_live {
                let floor = -20.0 - 40.0 * sens;
                self.env_out = ((20.0 * e.max(1e-6).log10() - floor) / 30.0).clamp(0.0, 1.0);
            }

            // ---- detection (gate, YIN, onsets), the tracker on the unfiltered input
            let venv = self.detect(mono, peak) as f64;
            let track_on = v[p::CARRIER] >= 0.5 || !self.carrier_x.is_settled();
            if track_on {
                let l = self.tracker.process(mono);
                let d = phase_dist(self.prev_psi, l.psi);
                self.psi2 = (self.psi2 + d).rem_euclid(2.0);
                self.prev_psi = l.psi;
            }

            // ---- smoothed controls
            let freq = self.freq.next() as f64;
            let fine = self.fine.next() as f64;
            let pitch = self.pitch.next() as f64;
            let ring = self.ring.next() as f64;
            let fb = self.feedback.next() as f64 * FB_LAW;
            let mix = self.mix.next();
            let ld = self.lfo_depth.next() as f64;
            let ea = self.env_amount.next() as f64;
            let ph_par = self.phase.next() as f64 / 360.0;
            let drive = self.drive.next() as f64 * 0.01;
            let cl = self.memo[4].get(self.carrier_level.next() as f64, 0.0, |a, _| 10f64.powf(a / 20.0));
            let bal = self.balance.next() as f64 * 0.01;
            let sp = self.stereo_phase.next() as f64 / 360.0;
            let level = self.memo[5].get(self.level.next() as f64, 0.0, |a, _| 10f64.powf(a / 20.0)) as f32;
            self.hpf.next();
            self.lpf.next();
            self.bass_keep.next();
            self.lowcut.next();
            self.highcut.next();
            if self.clock.is_multiple_of(CTRL) {
                self.set_filters(false);
            }
            let cx = self.carrier_x.next() as f64;
            self.mode_x.next();
            let mx = self.model_x.next() as f64;
            self.wave_x.next();
            let jx = self.tuning_x.next() as f64;
            let hpx = self.hp_x.next();
            let lpx = self.lp_x.next();
            let bkx = self.bk_x.next();

            // ---- carrier frequencies
            let motion = ld * self.lfo.out as f64 + ea * self.env_out as f64;
            // the interval, smoothed 20 ms in cents (the EXP smoother's ramp is as short as 5 ms)
            let rc = self.memo[0].get(pitch, jx, interval_cents);
            if self.r_cents != rc {
                self.r_cents += self.free_a * (rc - self.r_cents);
                if (rc - self.r_cents).abs() < 1e-6 {
                    self.r_cents = rc;
                }
            }
            let r = self.memo[1].get(self.r_cents, 0.0, |a, _| 2f64.powf(a / 1200.0));
            let rt = if motion != 0.0 { r * 2f64.powf(motion / 12.0) } else { r };
            // the free base, smoothed in log(f + 1) (exactly 0 at 0)
            let ft = self.memo[2].get(freq, fine, |a, b| (free_hz(a, b) + 1.0).log2());
            if self.free_log != ft {
                self.free_log += self.free_a * (ft - self.free_log);
                if (ft - self.free_log).abs() < 1e-9 {
                    self.free_log = ft;
                }
            }
            let f_free = self.memo[3].get(self.free_log, 0.0, |a, _| (2f64.powf(a) - 1.0).max(0.0));
            let fc_free = (f_free * rt).clamp(0.0, fmax);
            let (f_track, locked_note) = if track_on { self.track_base(lock) } else { (0.0, false) };
            let fc_track = (f_track * rt).clamp(0.0, fmax);
            let inc_free = fc_free / srd;
            let mut inc_track = fc_track / srd;
            if locked_note && fc_track < fmax {
                // exactly r times the loop's increment
                inc_track = rt * self.tracker.reference().inc;
            }
            // `phase`: where 2 r is an integer and r is still, the carrier is pulled onto r ψ₂ + phase
            let still = self.pitch.is_settled() && self.r_cents == rc && self.tuning_x.is_settled() && ld == 0.0 && ea == 0.0 && self.lfo_depth.is_settled();
            let twice = 2.0 * rt;
            if locked_note && still && (twice / twice.round() - 1.0).abs() < 1e-5 && twice.round() >= 1.0 {
                let (c2, c4) = self.compensation(f_track);
                // the input meets the carrier late by the up-sampler (two stages for the Diode's share)
                let comp = c2 + (c4 - c2) * mx * self.mode_x.w(0) as f64;
                let rr = 0.5 * twice.round();
                // the carrier's sine at φ + ¼ is a cosine: DC = (a₁ / 2) cos(phase) at unison
                let target = (rr * (self.psi2 + comp) + ph_par + 0.25).rem_euclid(1.0);
                let d = phase_dist(self.ph_track, target);
                let max = fc_track * (2f64.powf(PHASE_SLEW_C / 1200.0) - 1.0) / srd;
                inc_track += (d / (PHASE_TAU_S * srd)).clamp(-max, max);
            }
            self.carrier_hz = ((1.0 - cx) * fc_free + cx * inc_track * srd) as f32;
            self.state = if cx < 0.5 {
                0
            } else if locked_note && self.tracker.reference().locked {
                2
            } else if self.gate_open && self.target_hz > 0.0 {
                1
            } else {
                0
            };

            // ---- the modulator's weights
            let w_ring = self.mode_x.w(0) as f64;
            let (wu, wl) = (self.mode_x.w(1) as f64, self.mode_x.w(2) as f64);
            let w_ssb = wu + wl;
            let need4 = w_ring * mx > 0.0;

            // ---- carrier frames per channel (the 4× sub-samples only for Diode, the (cos, sin) pairs
            // only for the sidebands)
            // the octave position matters to the tabled waves only (the sine is computed)
            let tabled = self.wave_x.from != 0 || self.wave_x.to != 0;
            let oct_f = if tabled { Tables::octave(fc_free) } else { (0, 0.0) };
            let oct_t = if tabled { Tables::octave(inc_track * srd) } else { (0, 0.0) };
            let (wf, wt) = (self.wave_x.from, self.wave_x.to);
            let (wwf, wwt) = (self.wave_x.w(wf) as f64, self.wave_x.w(wt) as f64);
            let mut frames = [CarrierFrame::default(); 2];
            for (c, fr) in frames.iter_mut().enumerate().take(nch) {
                let off = if c == 1 { sp } else { 0.0 };
                let step = if need4 { 1 } else { 2 };
                for k in (0..4).step_by(step) {
                    let pf = self.ph_free + off + 0.25 * k as f64 * inc_free;
                    let pt = self.ph_track + off + 0.25 * k as f64 * inc_track;
                    let mut val = 0.0;
                    if cx < 1.0 {
                        let mut a = wwt * self.tables.value(wt, pf, oct_f) as f64;
                        if wf != wt && wwf > 0.0 {
                            a += wwf * self.tables.value(wf, pf, oct_f) as f64;
                        }
                        val += (1.0 - cx) * a;
                    }
                    if cx > 0.0 {
                        let mut a = wwt * self.tables.value(wt, pt, oct_t) as f64;
                        if wf != wt && wwf > 0.0 {
                            a += wwf * self.tables.value(wf, pt, oct_t) as f64;
                        }
                        val += cx * a;
                    }
                    fr.c4[k] = val;
                    if k % 2 == 0 && w_ssb > 0.0 {
                        let (mut cc, mut ss) = (0.0, 0.0);
                        if cx < 1.0 {
                            cc += (1.0 - cx) * self.tables.sine(pf + 0.25) as f64;
                            ss += (1.0 - cx) * self.tables.sine(pf) as f64;
                        }
                        if cx > 0.0 {
                            cc += cx * self.tables.sine(pt + 0.25) as f64;
                            ss += cx * self.tables.sine(pt) as f64;
                        }
                        fr.cs2[k / 2] = (cc, ss);
                    }
                }
            }
            self.ph_free = (self.ph_free + inc_free).rem_euclid(1.0);
            self.ph_track = (self.ph_track + inc_track).rem_euclid(1.0);

            // ---- squelch
            if thr_on {
                let tgt = if self.gate_open { 1.0 } else { 0.0 };
                self.squelch = if tgt >= self.squelch { (self.squelch + self.sq_open).min(tgt) } else { (self.squelch - self.sq_close).max(tgt) };
            } else {
                // `thr` 0: never squelched (the bleed stays), opened at the open rate
                self.squelch = (self.squelch + self.sq_open).min(1.0);
            }

            // ---- the dry is heard (mix under 1, bass_keep) or about to be
            let dry_on = mix < 1.0 || !self.mix.is_settled() || v[p::MIX] < 1.0 || bkx > 0.0 || v[p::BASS_KEEP] > 0.5;

            // ---- shared modulator controls
            let dk = self.memo[6].get(drive, 0.0, |a, _| 10f64.powf(24.0 * a / 20.0));
            let ctl = Ctl {
                w_clean: w_ring * (1.0 - mx),
                w_diode: w_ring * mx,
                w_ssb,
                balance: if w_ssb > 0.0 { (wu - wl) / w_ssb } else { 1.0 },
                ring,
                voice: (1.0 - ring) * venv,
                fb,
                drive_k: dk,
                drive_w: (drive / 0.05).min(1.0),
                diode_gain: dk,
                diode_norm: self.diode_norm,
                cl,
                pair_a: DiodePair { d1: ParkerDiode::new(VB * (1.0 + MISMATCH * bal), VL, 1.0), d2: ParkerDiode::new(VB * (1.0 - MISMATCH * bal), VL, 1.0) },
                dry_on,
            };

            // ---- channels
            let mut outs = [(0.0f32, 0.0f32); 2];
            for c in 0..nch {
                outs[c] = Self::tick(&mut self.ch[c], buffers[c][i], &ctl, &frames[c]);
            }
            let mut dry_mono = 0.0f32;
            for o in outs.iter().take(nch) {
                dry_mono += o.1;
            }
            dry_mono /= nch as f32;
            let lo = if bkx > 0.0 {
                let l0 = self.bk_lo[0].process(dry_mono);
                self.bk_lo[1].process(l0)
            } else {
                0.0
            };
            if bkx <= 0.0 && self.bk_x.is_settled() {
                self.bk_lo[0].reset();
                self.bk_lo[1].reset();
            }
            let sq = self.squelch as f32;
            for c in 0..nch {
                let ch = &mut self.ch[c];
                let (mut wet, dry) = outs[c];
                wet = ch.dc.process(wet);
                if hpx > 0.0 {
                    let h = ch.hp.process(wet);
                    wet += (h - wet) * hpx;
                    ch.hp_live = true;
                } else if ch.hp_live {
                    ch.hp.reset();
                    ch.hp_live = false;
                }
                if lpx > 0.0 {
                    let l = ch.lp.process(wet);
                    wet += (l - wet) * lpx;
                    ch.lp_live = true;
                } else if ch.lp_live {
                    ch.lp.reset();
                    ch.lp_live = false;
                }
                let mut y = dry * (1.0 - mix) + wet * sq * mix;
                if bkx > 0.0 {
                    let h0 = ch.bk_hi[0].process(y);
                    let hi = ch.bk_hi[1].process(h0);
                    y += (hi + lo - y) * bkx;
                } else if self.bk_x.is_settled() {
                    ch.bk_hi[0].reset();
                    ch.bk_hi[1].reset();
                }
                buffers[c][i] = y * level;
            }
        }
    }

    /// One channel, one host sample: (wet, matched dry).
    #[inline]
    fn tick(ch: &mut Chan, x: f32, k: &Ctl, fr: &CarrierFrame) -> (f32, f32) {
        let u = ch.up1.up(x);
        // ---- the 2× path: Clean ring, sidebands, the voice
        let mut a = [0.0f32; 2];
        let clean_on = k.w_clean > 0.0;
        let ssb_on = k.w_ssb > 0.0;
        for j in 0..2 {
            let s = u[j] as f64;
            let c = fr.c4[2 * j];
            let mut w = k.voice * c;
            if clean_on {
                let mut si = s;
                if k.drive_w > 0.0 {
                    si += k.drive_w * ch.drive.process(&TanhResidual { k: k.drive_k }, s);
                }
                let vin = si + k.fb * ch.fb2.read();
                let y = vin * c;
                ch.fb2.push(lim(y));
                w += k.ring * k.w_clean * y;
            }
            if ssb_on {
                let (cc, ss) = fr.cs2[j];
                w += k.ring * k.w_ssb * SSB_GAIN * ch.ssb.process(s, cc, ss, k.balance, false);
            }
            a[j] = w as f32;
        }
        if clean_on {
            ch.clean_live = true;
        } else if ch.clean_live {
            ch.drive.reset();
            ch.fb2.reset();
            ch.clean_live = false;
        }
        if ssb_on {
            ch.ssb_live = true;
        } else if ch.ssb_live {
            ch.ssb.reset();
            ch.ssb_live = false;
        }
        let mut wet = ch.dn_a.down(a);
        // ---- the 4× path: Parker's diode ring
        let mut d4 = 0.0f32;
        if k.w_diode > 0.0 {
            let pb = DiodePair { d1: ParkerDiode::new(VB, VL, 1.0), d2: ParkerDiode::new(VB, VL, 1.0) };
            // `drive` scales the input into the ring; the output is scaled back, so a small input keeps
            // Clean's sideband level (−6 dB each) whatever the drive
            let g = k.diode_gain;
            let norm = k.diode_norm / g;
            let out_g = k.ring * k.w_diode;
            let mut b = [0.0f32; 2];
            let mut t = [0.0f32; 2];
            for j in 0..2 {
                let q = ch.up2.up(u[j]);
                if k.dry_on {
                    // the dry's 4× path shares the up-samplers: same filters, same input
                    t[j] = ch.dd4b.down(q);
                }
                let mut y4 = [0.0f32; 2];
                for m in 0..2 {
                    // mda's recursion around the ring: (input + fb · y[−1 / 48 kHz]) into the diodes
                    let s = g * (q[m] as f64 + k.fb * ch.fb4.read());
                    let cc = k.cl * fr.c4[2 * j + m];
                    let ya = ch.dio_a.process(&k.pair_a, cc + 0.5 * s);
                    let yb = ch.dio_b.process(&pb, cc - 0.5 * s);
                    let y = (ya - yb) * norm;
                    ch.fb4.push(lim(y));
                    y4[m] = (out_g * y) as f32;
                }
                b[j] = ch.dn_b2.down(y4);
            }
            wet += ch.dn_b1.down(b);
            if k.dry_on {
                d4 = ch.dd4a.down(t);
            }
            ch.diode_live = true;
        } else if ch.diode_live {
            for o in [&mut ch.up2, &mut ch.dn_b2, &mut ch.dn_b1, &mut ch.dd4a, &mut ch.dd4b] {
                o.reset();
            }
            ch.dio_a.reset();
            ch.dio_b.reset();
            ch.fb4.reset();
            ch.diode_live = false;
        }
        // ---- the dry through the same paths (2×, and 4× for the Diode's share), only while heard
        let dry = if k.dry_on {
            let d2 = ch.dd2.down(u);
            ch.dry_live = true;
            if k.w_diode > 0.0 {
                d2 + (d4 - d2) * k.w_diode as f32
            } else {
                d2
            }
        } else {
            if ch.dry_live {
                for o in [&mut ch.dd2, &mut ch.dd4a, &mut ch.dd4b] {
                    o.reset();
                }
                ch.dry_live = false;
            }
            0.0
        };
        (wet, dry)
    }
}

impl Effect for RingMod {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate;
        self.nch = channels.clamp(1, 2);
        if !self.tables.is_built() {
            self.tables = Tables::build();
        }
        let srd = sample_rate as f64;
        self.design = HilbertDesign::for_rate(2.0 * srd);
        for c in self.ch.iter_mut() {
            c.ssb = SsbShifter::new(&self.design);
            c.fb2.prepare(2.0 * srd);
            c.fb4.prepare(4.0 * srd);
            c.dc.set(SvfMode::HighPass, DC_HZ, std::f32::consts::FRAC_1_SQRT_2, 0.0, sample_rate);
        }
        for e in [&mut self.freq, &mut self.fine, &mut self.pitch, &mut self.ring, &mut self.feedback, &mut self.mix, &mut self.rate, &mut self.lfo_depth] {
            e.prepare(sample_rate);
        }
        for o in [&mut self.phase, &mut self.drive, &mut self.carrier_level, &mut self.balance, &mut self.stereo_phase, &mut self.env_amount, &mut self.hpf, &mut self.lpf, &mut self.bass_keep, &mut self.level, &mut self.lowcut, &mut self.highcut] {
            o.prepare(sample_rate);
        }
        for r in [&mut self.carrier_x, &mut self.model_x, &mut self.tuning_x, &mut self.hp_x, &mut self.lp_x, &mut self.bk_x, &mut self.hand] {
            r.prepare(sample_rate);
        }
        self.mode_x.prepare(sample_rate);
        self.wave_x.prepare(sample_rate);
        self.free_a = 1.0 - (-1.0 / (0.02 * srd)).exp();
        self.lfo_k = 1.0 - (-1.0 / (0.002 * sample_rate)).exp();
        self.env.prepare(sample_rate);
        self.voice_env.prepare(sample_rate);
        self.voice_env.set_times(5.0, 500.0, sample_rate);
        self.gate_env.prepare((GATE_WINDOW_S * srd) as usize + 1);
        self.gate_env.set_window((GATE_WINDOW_S * srd) as usize);
        self.sq_open = 1.0 / (SQ_OPEN_MS * 1e-3 * srd);
        self.sq_close = 1.0 / (SQ_CLOSE_MS * 1e-3 * srd);
        self.pk_a = ((-1.0 / (0.0005 * srd)).exp(), (-1.0 / (0.01 * srd)).exp(), (-1.0 / (0.2 * srd)).exp());
        self.pk_slow_att = (-1.0 / (0.01 * srd)).exp();
        self.yin.prepare_range(sample_rate, YIN_MIN, YIN_MAX, true);
        self.tracker.prepare(sample_rate, 0);
        self.tracker.set_ff_time(10.0);
        // ψ is a reference here, never heard: the carrier accumulates the loop's increment and is pulled
        // onto its `phase` target by itself, so the loop may snap onto the input (Keep would bias the
        // increment by up to 5 ¢ for seconds on low notes while it pulls the offset in)
        self.tracker.engage(Engage::Snap);
        self.reset();
    }

    fn reset(&mut self) {
        let v = self.v;
        for (i, e) in [(p::FREQ, &mut self.freq), (p::FINE, &mut self.fine), (p::PITCH, &mut self.pitch), (p::RING, &mut self.ring), (p::FEEDBACK, &mut self.feedback), (p::MIX, &mut self.mix), (p::RATE, &mut self.rate), (p::LFO_DEPTH, &mut self.lfo_depth)] {
            e.settle(v[i]);
            e.reset();
        }
        for (i, o) in [
            (p::PHASE, &mut self.phase),
            (p::DRIVE, &mut self.drive),
            (p::CARRIER_LEVEL, &mut self.carrier_level),
            (p::BALANCE, &mut self.balance),
            (p::STEREO_PHASE, &mut self.stereo_phase),
            (p::ENV_AMOUNT, &mut self.env_amount),
            (p::HPF, &mut self.hpf),
            (p::LPF, &mut self.lpf),
            (p::LEVEL, &mut self.level),
            (p::LOWCUT, &mut self.lowcut),
            (p::HIGHCUT, &mut self.highcut),
        ] {
            o.set_target(v[i]);
            o.settle();
        }
        self.bass_keep.set_target(v[p::BASS_KEEP].max(20.0));
        self.bass_keep.settle();
        self.free_log = (free_hz(v[p::FREQ] as f64, v[p::FINE] as f64) + 1.0).log2();
        self.r_cents = interval_cents(v[p::PITCH] as f64, idx(v[p::TUNING], 2) as f64);
        self.memo = [Memo::new(); 8];
        self.carrier_x.set_immediate(idx(v[p::CARRIER], 2) as f32);
        self.mode_x.immediate(idx(v[p::MODE], 3));
        self.model_x.set_immediate(idx(v[p::MODEL], 2) as f32);
        self.wave_x.immediate(idx(v[p::WAVE], 3));
        self.tuning_x.set_immediate(idx(v[p::TUNING], 2) as f32);
        self.hp_x.set_immediate(if v[p::HPF] > 0.5 { 1.0 } else { 0.0 });
        self.lp_x.set_immediate(if v[p::LPF] < 19999.0 { 1.0 } else { 0.0 });
        self.bk_x.set_immediate(if v[p::BASS_KEEP] > 0.5 { 1.0 } else { 0.0 });
        for c in self.ch.iter_mut() {
            c.reset();
        }
        for f in self.bk_lo.iter_mut() {
            f.reset();
        }
        self.bk_set = -1.0;
        self.det_lc.reset();
        self.det_hc.reset();
        self.f_min = -1.0;
        self.set_filters(true);
        self.set_glide();
        self.lfo.reset();
        self.lfo_phase = 0.0;
        self.env.set_times(v[p::ENV_ATT], v[p::ENV_REL], self.sr);
        self.env.reset();
        self.env_out = 0.0;
        self.voice_env.reset();
        self.gate_env.reset();
        self.gate_open = false;
        self.squelch = if v[p::THR] <= 0.0 { 1.0 } else { 0.0 };
        self.yin.reset();
        self.tracker.reset();
        self.tracker.set_floor(thr_db(v[p::THR] as f64) as f32);
        self.pk_fast = 0.0;
        self.det_lvl = 0.0;
        self.pk_slow = 0.0;
        self.pk_armed = true;
        self.pk_refr = 0;
        self.pre_left = 0;
        self.pre_x1 = 0.0;
        self.pre_low = false;
        self.pre_t = [0.0; 3];
        self.pre_n = 0;
        self.yin_mute = 0;
        self.sample = 0;
        self.target_hz = 0.0;
        self.glide_log = 0.0;
        self.snap_left = 0;
        self.arrived = false;
        self.hand.set_immediate(0.0);
        self.track_hz = 0.0;
        self.ph_free = 0.0;
        self.ph_track = 0.0;
        self.psi2 = 0.0;
        self.prev_psi = 0.0;
        self.comp_hz = -1.0;
        self.carrier_hz = 0.0;
        self.state = 0;
        self.clock = 0;
    }

    fn set_param(&mut self, id: &str, value: f32) {
        if let Some(i) = PARAMS.iter().position(|pi| pi.id == id) {
            let pi = &PARAMS[i];
            let v = value.clamp(pi.min, pi.max);
            if v != self.v[i] {
                self.v[i] = v;
                self.apply(i);
            }
        }
    }

    fn set_transport(&mut self, transport: TransportInfo) {
        self.transport = transport;
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_block(buffers);
    }
}
