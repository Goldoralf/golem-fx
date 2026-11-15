// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-sustainer — golem's sustainer and feedback generator (design: README.md).
//!
//! - **Detection** (side chain, on the main or the key signal): YIN (gfx-dsp `detect`, its own ÷ to
//!   ~12 kHz; under 60 Hz a second, long window at a quarter of that rate, gfx-tuner's pair) names
//!   single notes; gfx-dsp `multipitch` names the notes of a chord while YIN is unclear; `onset`'s pick
//!   detector (A) and pitch-jump detector (C) give re-picks and legato changes; a sliding maximum over
//!   one period gives the note's level (body, mute, touch).
//! - **Phase lock**: one gfx-dsp `track::Tracker` per voice: the string's partials, each phase-locked,
//!   with an inharmonicity trim; the generator adds g_k = max(0, a_k − γ · d_k) on partial k's own phase,
//!   so it supplies only what the string has lost.
//! - **Feedback**: the modes of `harmonic` (sub, 1 … 6) compete on the control side (1 ms, f64):
//!   d(ln a_k)/dt = β · [G_k / G_max − u_k − c · Σ_(j≠k) u_j], c = 1.2, u = (a / A)², A = body × `bloom`;
//!   β in closed form from `rise`, β_j ≥ 17 s⁻¹ once in Hold, losers resting at −30 dB. Natural types:
//!   G_k = w_k (1 + cos Φ_k) / 2 with the loop phase Φ_k = 2π f_k τ, τ = `distance` / 343 + 1 ms.
//!   A colour phasor cos φ + c₂ cos 2φ + c₃ cos 3φ (Chebyshev, computed: nothing aliases).
//! - **Sustain**: a partial bank, every partial k ≤ K held at A_k = `bloom` · b_k · k^`tone`.
//! - Note state per voice: Idle → Wait → Rise → Hold → Release; two voices alternate so a release
//!   overlaps the next wait; `chord` All gives each note (≤ 4) its own voice.
//!
//! Zero latency: the dry is one gain. Real-time safe (memory sized in `prepare`), deterministic,
//! block-size independent.

#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

#[cfg(feature = "plugin")]
mod plugin;

#[cfg(test)]
mod tests;

use gfx_dsp::detect::PitchDetector;
use gfx_dsp::envelope::SlidingMax;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::lfo::{Lfo, Shape};
use gfx_dsp::multipitch::Analyzer;
use gfx_dsp::onset::{Pick, PitchJump};
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::track::{Engage, Tracker};
use gfx_dsp::{Effect, ParamInfo};

pub const NP: usize = 23;

pub static PARAMS: [ParamInfo; NP] = [
    // front panel
    ParamInfo::enumeration("harmonic", "Harmonic", 9, 2),
    ParamInfo::float("delay", "Onset Delay", 0.0, 3000.0, 300.0),
    ParamInfo::float("rise", "Rise", 50.0, 6000.0, 1200.0).with_exp(),
    ParamInfo::float("bloom", "Bloom", -24.0, 6.0, -6.0).with_exp(),
    ParamInfo::float("dry", "Dry", -60.0, 0.0, 0.0).with_exp(),
    ParamInfo::enumeration("mode", "Mode", 3, 0),
    // expert
    ParamInfo::enumeration("trigger", "Trigger", 2, 0),
    ParamInfo::boolean("feed", "Feed", false),
    ParamInfo::float("distance", "Distance", 0.2, 6.0, 1.5).with_exp(),
    ParamInfo::float("drift", "Drift", 0.0, 100.0, 0.0),
    ParamInfo::enumeration("chord", "Chord", 4, 2),
    ParamInfo::boolean("carry", "Legato Carry", true),
    ParamInfo::float("release", "Release", 20.0, 2000.0, 150.0),
    ParamInfo::float("mute_sens", "Mute Sensitivity", 0.0, 100.0, 50.0),
    ParamInfo::float("max_time", "Max Hold", 1.0, 60.0, 15.0),
    ParamInfo::float("duck", "Duck", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("color", "Color", 0.0, 100.0, 50.0),
    ParamInfo::float("tone", "Tone", -100.0, 100.0, 0.0),
    ParamInfo::float("lowest", "Lowest Note", 30.0, 150.0, 70.0),
    ParamInfo::float("thresh", "Noise Floor", -96.0, -30.0, -66.0),
    ParamInfo::enumeration("source", "Detector Source", 2, 0),
    ParamInfo::boolean("bypass", "Bypass", false),
    ParamInfo::boolean("trails", "Trails", true),
];

pub mod p {
    pub const HARMONIC: usize = 0;
    pub const DELAY: usize = 1;
    pub const RISE: usize = 2;
    pub const BLOOM: usize = 3;
    pub const DRY: usize = 4;
    pub const MODE: usize = 5;
    pub const TRIGGER: usize = 6;
    pub const FEED: usize = 7;
    pub const DISTANCE: usize = 8;
    pub const DRIFT: usize = 9;
    pub const CHORD: usize = 10;
    pub const CARRY: usize = 11;
    pub const RELEASE: usize = 12;
    pub const MUTE_SENS: usize = 13;
    pub const MAX_TIME: usize = 14;
    pub const DUCK: usize = 15;
    pub const COLOR: usize = 16;
    pub const TONE: usize = 17;
    pub const LOWEST: usize = 18;
    pub const THRESH: usize = 19;
    pub const SOURCE: usize = 20;
    pub const BYPASS: usize = 21;
    pub const TRAILS: usize = 22;
}

/// Enum options (order free until the first release, then append only).
pub const HARMONICS: [&str; 9] = ["Sub", "Fundamental", "Octave", "Octave + Fifth", "Two Octaves", "Two Octaves + Third", "Natural Low", "Natural High", "Climb"];
pub const MODES: [&str; 3] = ["Feedback", "Sustain", "Both"];
pub const TRIGGERS: [&str; 2] = ["Auto", "Switch"];
pub const CHORDS: [&str; 4] = ["Lowest", "Highest", "Strongest", "All"];
pub const SOURCES: [&str; 2] = ["Main", "Key"];

/// A voice's commanded levels (tests): A, the modes' a (0 = sub), the bank's a, the body levels b_k.
pub type Levels = (f64, [f64; 7], [f64; 17], [f64; 17]);

/// The note state of a voice (the `state` meter's values).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Idle = 0,
    Wait = 1,
    Rise = 2,
    Hold = 3,
    Release = 4,
}

// ---------------------------------------------------------------- constants

/// Voices: up to 4 notes (`chord` All), each with a second slot for its release.
const NV: usize = 8;
const MAX_SLOTS: usize = 4;
/// Partials of a voice (the bank's K) and the Feedback modes (index 0 = the sub, 1 … 6 = partial k).
const KMAX: usize = 16;
const K_ALL: usize = 8;
const NM: usize = 7;
const BANK_TOP_HZ: f64 = 5000.0;
/// The partials' loops run every this many samples at most (gfx-dsp `track`, opt-in; and fs / n ≥ 4 f0).
const PART_RATE: usize = 8;
/// A note named by `multipitch` (inside a chord) is tracked with a comb of up to this many periods.
const CHORD_COMB_MAX: usize = 6;
/// The tracker learns the frequency it will hold only this far above the hold level (dB).
const HOLD_MARGIN_DB: f32 = 12.0;
/// Control period (ms).
const CTRL_MS: f64 = 1.0;
/// YIN: the analysis rate (two detectors, gfx-tuner's pair), a clear reading (CMNDF ≤ 0.2).
const YIN_RATE: f32 = 12000.0;
const YIN_MAX: f32 = 1500.0;
const LONG_BELOW: f32 = 60.0;
const CLEAR: f32 = 0.8;
/// A reading feeds a voice when it is this close to its note (cents; the tracker's own new-note rule
/// is 50 ¢ between readings).
const FEED_CENTS: f64 = 45.0;
/// Chords: Wait is extended until `multipitch` names them, at most this long after the onset (s).
const CHORD_WAIT_S: f64 = 0.4;
/// A waiting voice that has not locked this long after its delay (or the chord wait) gives up (s).
const WAIT_LOCK_S: f64 = 1.0;
/// The body is the note's level this long after the onset (s).
const BODY_S: f64 = 0.08;
/// Mute: the drop window (s) and the drop D from `mute_sens` (dB); the second rule's drop.
const MUTE_WIN: usize = 60;
const MUTE_D0: f64 = 30.0;
const MUTE_D1: f64 = 10.0;
const MUTE_QUIET_DB: f64 = 6.0;
/// The tracker's hold level and the pick detector's floor under `thresh` (dB).
const HOLD_UNDER: f32 = 6.0;
const PICK_UNDER: f32 = 24.0;
/// Legato jumps within this of a pick belong to the pick (s).
const JUMP_AFTER_PICK_S: f64 = 0.08;
/// Competition (README §2.4).
const COMP_C: f64 = 1.2;
const LOSER_DB: f64 = -30.0;
const SEED_DB: f64 = -40.0;
const BETA_JUMP: f64 = 17.0;
/// A mode outside the voice's set fades at this rate (s⁻¹, ln a).
const OUT_RATE: f64 = 17.0;
/// Natural: weights, the electronics' delay (s), the speed of sound (m / s), drift (m at 100 %, Hz).
const W_LOW: [f64; 4] = [1.0, 0.9, 0.7, 0.5];
const W_HIGH: [f64; 6] = [0.3, 0.8, 1.0, 0.9, 0.7, 0.5];
const TAU_EL: f64 = 0.001;
const SOUND: f64 = 343.0;
const DRIFT_M: f64 = 0.3;
const DRIFT_HZ: f32 = 0.1;
/// Climb: the octave takes over `rise` + this after the start of Rise (s); the fundamental's gain then.
const CLIMB_S: f64 = 1.0;
const CLIMB_G1: f64 = 0.6;
/// Colour at 50 %: −30 dBc at full bloom.
const COLOR_K: f64 = 0.031_622_776;
/// Bypass without trails: the fade (ms).
const BYPASS_MS: f32 = 10.0;
/// Key: silent this long (s) = not connected.
const KEY_SILENT_S: f64 = 1.0;

#[inline]
fn idx(v: f32, n: usize) -> usize {
    (v.round().max(0.0) as usize).min(n - 1)
}

#[inline]
fn cents(a: f64, b: f64) -> f64 {
    1200.0 * (a / b).log2()
}

#[inline]
fn to_db(x: f64) -> f64 {
    if x <= 1e-9 {
        -180.0
    } else {
        20.0 * x.log10()
    }
}

/// cos(2π t), t in turns: wrapped to a quarter turn, then a degree-10 even polynomial (< 5e−7).
#[inline]
pub fn cos_turns(t: f64) -> f32 {
    let x = (t - t.round()) as f32;
    let mut y = x.abs();
    let neg = y > 0.25;
    if neg {
        y = 0.5 - y;
    }
    let w = y * std::f32::consts::TAU;
    let z = w * w;
    let c = 1.0 + z * (-0.5 + z * (1.0 / 24.0 + z * (-1.0 / 720.0 + z * (1.0 / 40320.0 - z * (1.0 / 3_628_800.0)))));
    if neg {
        -c
    } else {
        c
    }
}

/// Which modes a `harmonic` option sounds (index 0 = the sub, 1 … 6 = partial k).
fn mode_set(h: usize) -> [bool; NM] {
    let mut s = [false; NM];
    match h {
        0 => s[0] = true,
        1..=5 => s[h] = true,
        6 => s[1..=4].iter_mut().for_each(|v| *v = true),
        7 => s[1..=6].iter_mut().for_each(|v| *v = true),
        _ => {
            s[1] = true;
            s[2] = true;
        }
    }
    s
}

/// The β of a logistic in u that reaches A − 1 dB at `rise` from u₀ (README §2.4, closed form).
#[inline]
fn beta_for(u0: f64, rise_s: f64) -> f64 {
    let u0 = u0.clamp(1e-6, 0.5);
    (3.86 * (1.0 / u0 - 1.0)).ln() / (2.0 * rise_s.max(0.01))
}

/// One control step of the mode competition (README §2.4): d(ln a_k)/dt = β · [G_k / G_max − u_k −
/// c · Σ_(j≠k) u_j], u = (a / A)², c = 1.2, for the modes in `set` (losers rest at −30 dB re A); a
/// mode outside the set fades out at a further 17 s⁻¹. Returns the bloom's progress max u (≤ 1).
pub fn compete(a: &mut [f64; 7], a_t: f64, g: &[f64; 7], set: &[bool; 7], beta: f64, dt: f64) -> f64 {
    let floor = a_t * 10f64.powf(LOSER_DB / 20.0);
    let gmax = (0..NM).filter(|&m| set[m]).map(|m| g[m]).fold(1e-6, f64::max);
    let mut u = [0.0f64; NM];
    let mut su = 0.0;
    for m in 0..NM {
        u[m] = (a[m] / a_t).powi(2);
        su += u[m];
    }
    for m in 0..NM {
        if a[m] <= 0.0 {
            continue;
        }
        let others = su - u[m];
        let old = a[m];
        let mut x = if set[m] {
            old * (beta * (g[m] / gmax - u[m] - COMP_C * others) * dt).exp()
        } else {
            old * (-(beta * (u[m] + COMP_C * others) + OUT_RATE) * dt).exp()
        };
        if set[m] {
            // losers rest at −30 dB re A
            if x < old {
                x = x.max(old.min(floor));
            }
        } else if x < a_t * 1e-5 {
            x = 0.0;
        }
        a[m] = x.min(4.0 * a_t);
    }
    (0..NM).filter(|&m| set[m]).map(|m| u[m]).fold(0.0, f64::max).min(1.0)
}

/// The Natural types' loop gains: G_k = w_k (1 + cos Φ_k) / 2, Φ_k = 2π f_k τ (`fk[k]`: partial k's
/// frequency, Hz; τ = `distance` / 343 + 1 ms).
pub fn natural_gains(high: bool, fk: &[f64; 7], tau: f64) -> [f64; 7] {
    let w: &[f64] = if high { &W_HIGH } else { &W_LOW };
    let mut g = [0.0f64; NM];
    for (j, &wk) in w.iter().enumerate() {
        let phi = 2.0 * std::f64::consts::PI * fk[j + 1] * tau;
        g[j + 1] = wk * 0.5 * (1.0 + phi.cos());
    }
    g
}

/// The delay of the Natural loop (s) at a distance (m).
pub fn loop_tau(distance_m: f64) -> f64 {
    distance_m / SOUND + TAU_EL
}

/// One voice: a tracker and its generator.
#[derive(Clone, Debug)]
struct Voice {
    tr: Tracker,
    st: State,
    slot: usize,
    /// the note the voice follows (Hz) and where it was named (multipitch or YIN)
    hz: f64,
    from_mp: bool,
    t_on: u64,
    t_rise: u64,
    body: f64,
    bk: [f64; KMAX + 1],
    body_taken: bool,
    /// Feedback: target A, seeds' u₀, mode levels a (0 = sub)
    a_ref: f64,
    u0: f64,
    a_fb: [f64; NM],
    /// Sustain: the bank's levels and their u₀
    a_b: [f64; KMAX + 1],
    u0b: [f64; KMAX + 1],
    /// commanded level per partial (0 = sub), its per-sample step, ownership (`chord` All)
    amp: [f64; KMAX + 1],
    step: [f64; KMAX + 1],
    own: [bool; KMAX + 1],
    /// the Feedback modes' own levels, per sample (the colour phasor's share of each partial)
    fba: [f64; NM],
    fbs: [f64; NM],
    kmax: usize,
    sub_psi: f64,
    rel_n: u64,
    rel_len: u64,
    /// the bloom's progress for `duck` (0 – 1)
    prog: f64,
    hold: bool,
}

impl Voice {
    fn new() -> Self {
        Self {
            tr: Tracker::new(),
            st: State::Idle,
            slot: 0,
            hz: 0.0,
            from_mp: false,
            t_on: 0,
            t_rise: 0,
            body: 0.0,
            bk: [0.0; KMAX + 1],
            body_taken: false,
            a_ref: 0.0,
            u0: 0.5,
            a_fb: [0.0; NM],
            a_b: [0.0; KMAX + 1],
            u0b: [0.5; KMAX + 1],
            amp: [0.0; KMAX + 1],
            step: [0.0; KMAX + 1],
            own: [true; KMAX + 1],
            fba: [0.0; NM],
            fbs: [0.0; NM],
            kmax: 1,
            sub_psi: 0.0,
            rel_n: 0,
            rel_len: 1,
            prog: 0.0,
            hold: false,
        }
    }

    fn active(&self) -> bool {
        matches!(self.st, State::Wait | State::Rise | State::Hold)
    }

    fn sounding(&self) -> bool {
        matches!(self.st, State::Rise | State::Hold | State::Release)
    }

    /// The tracked frequency now (Hz).
    fn f0(&self, sr: f64) -> f64 {
        let r = self.tr.reference();
        if r.inc > 0.0 {
            r.inc * sr
        } else {
            self.hz
        }
    }

    /// Partial k's level from the tracker (k = 1: the reference).
    fn d(&self, k: usize) -> f64 {
        if k == 1 {
            self.tr.reference().level as f64
        } else {
            self.tr.partial(k).level as f64
        }
    }
}

/// Per-sample controls shared by the voices.
#[derive(Clone, Copy, Debug)]
struct Synth {
    gamma: f64,
    /// the hold level (linear): a partial's level under it is not subtracted
    floor: f64,
    inv_floor: f64,
    kappa: f64,
    nyq: f64,
}

pub struct Sustainer {
    v: [f32; NP],
    sr: f32,
    nch: usize,
    clock: u64,
    ctrl_n: u64,
    // smoothers
    rise: ExpSmoother,
    bloom: ExpSmoother,
    dry: ExpSmoother,
    duck: ExpSmoother,
    distance: ExpSmoother,
    color: OnePole,
    tone: OnePole,
    drift: OnePole,
    byp_x: LinearRamp,
    gamma_dry: f64,
    // detection
    aa: [Svf; 2],
    dec: usize,
    dphase: usize,
    fd: f32,
    short: PitchDetector,
    long: PitchDetector,
    lp4: [Svf; 2],
    d4: usize,
    yin_hz: f64,
    yin_clear: bool,
    yin_voiced: bool,
    yin_at: u64,
    jump_at: u64,
    pick: Pick,
    jump: PitchJump,
    mp: Analyzer,
    mp_seen: u64,
    lvl: SlidingMax,
    lvl_now: f64,
    /// the pick detector's floor (linear): `thresh` − 24 dB; `thresh` (linear)
    pick_floor: f64,
    thresh_lin: f64,
    hold_lin: f64,
    hold_s: f64,
    hold_a: f64,
    /// `dry` in dB and its gain (memo); the bloom's progress over the voices (control rate)
    dry_db: f32,
    dry_lin: f64,
    prog: f64,
    lvl_win: usize,
    hist: [f64; MUTE_WIN],
    hist_at: usize,
    key_silent: u64,
    // note
    onset_at: Option<u64>,
    pick_flag: bool,
    jump_flag: Option<f64>,
    spent: bool,
    was_feed: bool,
    was_bypassed: bool,
    lowest_set: f32,
    voices: Vec<Voice>,
    drift_lfo: Lfo,
    drift_m: f64,
    // meters
    m_state: State,
    m_note: f32,
    m_harm: f32,
    m_lock: f32,
    /// generated output of the last sample (tests)
    last_gen: f32,
}

impl Default for Sustainer {
    fn default() -> Self {
        Self::new()
    }
}

impl Sustainer {
    pub fn new() -> Self {
        let v: [f32; NP] = core::array::from_fn(|i| PARAMS[i].default);
        let es = |i: usize| ExpSmoother::new(PARAMS[i].min, PARAMS[i].max, v[i]);
        Self {
            v,
            sr: 48000.0,
            nch: 1,
            clock: 0,
            ctrl_n: 48,
            rise: es(p::RISE),
            bloom: es(p::BLOOM),
            dry: es(p::DRY),
            duck: es(p::DUCK),
            distance: es(p::DISTANCE),
            color: OnePole::new(20.0, v[p::COLOR], 1e-3),
            tone: OnePole::new(20.0, v[p::TONE], 1e-3),
            drift: OnePole::new(20.0, v[p::DRIFT], 1e-3),
            byp_x: LinearRamp::new(BYPASS_MS, 0.0),
            gamma_dry: 1.0,
            aa: [Svf::new(), Svf::new()],
            dec: 4,
            dphase: 0,
            fd: YIN_RATE,
            short: PitchDetector::default(),
            long: PitchDetector::default(),
            lp4: [Svf::new(), Svf::new()],
            d4: 0,
            yin_hz: 0.0,
            yin_clear: false,
            yin_voiced: false,
            yin_at: 0,
            jump_at: 0,
            pick: Pick::default(),
            jump: PitchJump::default(),
            mp: Analyzer::new(),
            mp_seen: 0,
            lvl: SlidingMax::new(),
            lvl_now: 0.0,
            pick_floor: 0.0,
            thresh_lin: 0.0,
            hold_lin: 0.0,
            hold_s: 0.0,
            hold_a: 0.0,
            dry_db: f32::NAN,
            dry_lin: 1.0,
            prog: 0.0,
            lvl_win: 1,
            hist: [-180.0; MUTE_WIN],
            hist_at: 0,
            key_silent: 0,
            onset_at: None,
            pick_flag: false,
            jump_flag: None,
            spent: false,
            was_feed: false,
            was_bypassed: false,
            lowest_set: -1.0,
            voices: (0..NV).map(|_| Voice::new()).collect(),
            drift_lfo: Lfo::new(Shape::Random, 0.0, 0x5E57_414E),
            drift_m: 0.0,
            m_state: State::Idle,
            m_note: 0.0,
            m_harm: 0.0,
            m_lock: 0.0,
            last_gen: 0.0,
        }
    }

    fn b(&self, i: usize) -> bool {
        self.v[i] >= 0.5
    }

    /// `state` meter: the loudest voice's note state.
    pub fn state(&self) -> State {
        self.m_state
    }

    /// `note` meter: the tracked f0 (Hz), 0 = none.
    pub fn note_hz(&self) -> f32 {
        self.m_note
    }

    /// `harm` meter: the sounding mode (0.5, 1 … 6; 1 in Sustain), 0 = none.
    pub fn harm(&self) -> f32 {
        self.m_harm
    }

    /// `lock` meter: 1 = the string is followed, 0 = its pitch is held (or no note).
    pub fn lock(&self) -> f32 {
        self.m_lock
    }

    /// The generated part of the last output sample (tests).
    pub fn last_generated(&self) -> f32 {
        self.last_gen
    }

    /// The commanded levels of the loudest voice: (A, a per mode 0 … 6, bank a per partial) (tests).
    pub fn levels(&self) -> Option<Levels> {
        self.loudest().map(|i| {
            let v = &self.voices[i];
            (v.a_ref, v.a_fb, v.a_b, v.bk)
        })
    }

    /// The tracked frequency of partial k of the loudest voice (Hz) and its lock (tests).
    pub fn partial_hz(&self, k: usize) -> Option<(f64, bool)> {
        self.loudest().map(|i| {
            let v = &self.voices[i];
            let l = if k == 1 { v.tr.reference() } else { v.tr.partial(k) };
            (l.inc * self.sr as f64, l.locked)
        })
    }

    /// Every non-idle voice's (state, tracked f0, commanded level per partial 0 … 16) (tests).
    pub fn voice_amps(&self) -> Vec<(State, f64, [f64; KMAX + 1])> {
        self.voices.iter().filter(|v| v.st != State::Idle).map(|v| (v.st, v.f0(self.sr as f64), v.amp)).collect()
    }

    /// The voices' (state, note Hz) (tests).
    pub fn voice_states(&self) -> Vec<(State, f64)> {
        self.voices.iter().filter(|v| v.st != State::Idle).map(|v| (v.st, v.f0(self.sr as f64))).collect()
    }

    fn loudest(&self) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for (i, v) in self.voices.iter().enumerate() {
            if v.st == State::Idle {
                continue;
            }
            let mut l: f64 = v.amp.iter().copied().fold(0.0, f64::max);
            if v.st == State::Release {
                l *= 0.5;
            }
            // a waiting voice counts, below any sounding one
            let score = if v.sounding() { 1.0 + l } else { 0.5 };
            if best.is_none_or(|b| score > b.1) {
                best = Some((i, score));
            }
        }
        best.map(|b| b.0)
    }

    fn apply(&mut self, i: usize) {
        let v = self.v[i];
        let now = self.clock;
        match i {
            p::RISE => self.rise.set_target(v, now),
            p::BLOOM => self.bloom.set_target(v, now),
            p::DRY => self.dry.set_target(v, now),
            p::DUCK => self.duck.set_target(v, now),
            p::DISTANCE => self.distance.set_target(v, now),
            p::COLOR => self.color.set_target(v),
            p::TONE => self.tone.set_target(v),
            p::DRIFT => self.drift.set_target(v),
            p::THRESH => self.set_floors(),
            p::LOWEST => self.set_lowest(false),
            _ => {}
        }
    }

    fn set_floors(&mut self) {
        let t = self.v[p::THRESH];
        self.pick_floor = 10f64.powf((t - PICK_UNDER) as f64 / 20.0);
        self.thresh_lin = 10f64.powf(t as f64 / 20.0);
        self.hold_lin = 10f64.powf((t - HOLD_UNDER) as f64 / 20.0);
        self.short.set_floor(t - HOLD_UNDER);
        self.long.set_floor(t - HOLD_UNDER);
        for v in self.voices.iter_mut() {
            v.tr.set_floor(t - HOLD_UNDER);
        }
    }

    fn set_lowest(&mut self, force: bool) {
        let lo = self.v[p::LOWEST].clamp(30.0, 150.0);
        if !force && (lo - self.lowest_set).abs() < 0.01 {
            return;
        }
        self.lowest_set = lo;
        self.short.set_range(lo.max(LONG_BELOW), YIN_MAX);
        self.long.set_range(lo.min(LONG_BELOW), 150.0);
        self.mp.set_f_min(lo);
    }

    /// The detector's wait after an onset before a YIN reading belongs to the new note (s): the window
    /// and a hop.
    fn yin_wait(&self) -> f64 {
        let lo = self.lowest_set.max(30.0) as f64;
        1.0 / lo + 0.003
    }

    /// The comb length for a note of the current chord reading.
    fn chord_narrow(&self, hz: f64) -> usize {
        let others: [f64; 6] = core::array::from_fn(|k| self.mp.notes().get(k).map_or(0.0, |n| n.hz as f64));
        chord_comb(hz, &others)
    }

    /// Chooses the note of a chord (`chord` Lowest / Highest / Strongest).
    fn choose(&self) -> Option<f64> {
        let n = self.mp.notes();
        if n.is_empty() {
            return None;
        }
        Some(match idx(self.v[p::CHORD], 4) {
            0 | 3 => n[0].hz as f64,
            1 => n[n.len() - 1].hz as f64,
            _ => n.iter().max_by(|a, b| a.salience.total_cmp(&b.salience)).map_or(n[0].hz, |x| x.hz) as f64,
        })
    }

    fn free_voice(&self) -> usize {
        if let Some(i) = self.voices.iter().position(|v| v.st == State::Idle) {
            return i;
        }
        // steal the release furthest on
        let mut best = (0usize, -1.0f64);
        for (i, v) in self.voices.iter().enumerate() {
            if v.st == State::Release {
                let p = v.rel_n as f64 / v.rel_len.max(1) as f64;
                if p > best.1 {
                    best = (i, p);
                }
            }
        }
        best.0
    }

    /// A new voice on `hz`, in Wait from `t_on`.
    fn start_voice(&mut self, hz: f64, from_mp: bool, slot: usize, t_on: u64) -> usize {
        let i = self.free_voice();
        let narrow = if from_mp { self.chord_narrow(hz) } else { 1 };
        let thresh = self.v[p::THRESH];
        let n_tr = self.partials_needed(hz, slot_all(self.v[p::CHORD]));
        let v = &mut self.voices[i];
        v.tr.reset();
        v.tr.set_floor(thresh - HOLD_UNDER);
        v.tr.set_reference(1);
        v.tr.engage(Engage::Snap);
        // a note named inside a chord: the narrow tracker (its comb's zeros on the other notes)
        v.tr.set_narrow(narrow);
        v.tr.set_partials(n_tr);
        v.tr.set_f0(hz as f32, true);
        v.st = State::Wait;
        v.slot = slot;
        v.hz = hz;
        v.from_mp = from_mp;
        v.t_on = t_on;
        v.t_rise = 0;
        v.body = 0.0;
        v.bk = [0.0; KMAX + 1];
        v.body_taken = false;
        v.a_ref = 0.0;
        v.u0 = 0.5;
        v.a_fb = [0.0; NM];
        v.a_b = [0.0; KMAX + 1];
        v.u0b = [0.5; KMAX + 1];
        v.amp = [0.0; KMAX + 1];
        v.step = [0.0; KMAX + 1];
        v.own = [true; KMAX + 1];
        v.fba = [0.0; NM];
        v.fbs = [0.0; NM];
        v.kmax = 1;
        v.sub_psi = 0.0;
        v.rel_n = 0;
        v.prog = 0.0;
        v.hold = false;
        i
    }

    /// Partials a voice tracks beyond the reference: the Feedback modes' highest k, or the bank's K;
    /// at least 2 (partials 2 and 3 decide the weak-fundamental rule).
    fn partials_needed(&self, hz: f64, all: bool) -> usize {
        let mode = idx(self.v[p::MODE], 3);
        let fb = match idx(self.v[p::HARMONIC], 9) {
            0 | 1 => 1,
            h @ 2..=5 => h,
            6 => 4,
            7 => 6,
            _ => 2,
        };
        let bank = if mode >= 1 { bank_k(hz, self.sr as f64, all) } else { 0 };
        let k = if mode == 1 { bank } else { fb.max(bank) };
        k.clamp(3, KMAX) - 1
    }

    fn release_voice(&mut self, i: usize, len_s: f64) {
        let sr = self.sr as f64;
        let v = &mut self.voices[i];
        match v.st {
            State::Wait => v.st = State::Idle,
            State::Rise | State::Hold => {
                v.st = State::Release;
                v.rel_n = 0;
                v.rel_len = (len_s * sr).max(1.0) as u64;
                // the frequency is held (no chirp); the levels freeze
                v.tr.hold();
                for k in 0..=KMAX {
                    v.step[k] = 0.0;
                }
                v.fbs = [0.0; NM];
            }
            _ => {}
        }
    }

    fn release_all(&mut self, len_s: f64) {
        for i in 0..NV {
            self.release_voice(i, len_s);
        }
    }

    // ---------------------------------------------------------------- detection

    /// One detection sample: level, pick, legato, YIN, multipitch.
    #[inline]
    fn detect(&mut self, xd: f32) {
        let l = self.lvl.next(xd) as f64;
        self.lvl_now = l;
        if self.pick.next(xd, l >= self.pick_floor) {
            self.pick_flag = true;
        }
        self.mp.push(xd);
        // YIN at ~12 kHz, the long window at a quarter of it (only under 60 Hz)
        let a = self.aa[0].process(xd);
        let y = self.aa[1].process(a);
        self.dphase += 1;
        if self.dphase < self.dec {
            return;
        }
        self.dphase = 0;
        let long_on = self.lowest_set < LONG_BELOW;
        if long_on {
            let q = self.lp4[0].process(y);
            let q = self.lp4[1].process(q);
            self.d4 += 1;
            if self.d4 == 4 {
                self.d4 = 0;
                self.long.push_decimated(q);
            }
        }
        if !self.short.push_decimated(y) {
            return;
        }
        let (mut hz, mut clarity) = (self.short.hz(), self.short.clarity());
        if long_on && (hz.is_none() || clarity < CLEAR) && self.short.period_hz() < 200.0 {
            if let Some(h) = self.long.hz() {
                hz = Some(h);
                clarity = self.long.clarity();
            }
        }
        // legato (detector C) on these readings
        let elapsed = self.clock - self.jump_at;
        self.jump_at = self.clock;
        if self.jump.feed(hz, clarity, elapsed, PitchJump::clarity_for(0.5)).is_some() {
            if let Some(h) = self.jump.reference_hz() {
                self.jump_flag = Some(h as f64);
            }
        }
        self.yin_voiced = hz.is_some();
        self.yin_clear = hz.is_some() && clarity >= CLEAR;
        self.yin_hz = hz.unwrap_or(0.0) as f64;
        self.yin_at = self.clock;
        // multipitch runs while YIN is unclear
        self.mp.set_active(!self.yin_clear);
        // a clear reading feeds the voices on its note while the string is above `thresh` (a fading
        // string's readings are noise: they would walk the note away), within 45 ¢ of what each
        // tracker follows now
        if self.yin_clear && self.lvl_now >= self.thresh_lin {
            let f = self.yin_hz;
            let sr = self.sr as f64;
            let since_ok = self.onset_at.is_none_or(|t| (self.clock - t) as f64 >= self.yin_wait() * sr);
            for v in self.voices.iter_mut() {
                if v.active() && since_ok && cents(f, v.f0(sr)).abs() <= FEED_CENTS {
                    v.hz = f;
                    v.tr.set_f0(f as f32, true);
                }
            }
        }
    }

    // ---------------------------------------------------------------- control (1 ms)

    fn control(&mut self) {
        let now = self.clock;
        let sr = self.sr as f64;
        let ms = sr * 1e-3;
        let v = self.v;
        let thresh = v[p::THRESH] as f64;
        let l_db = to_db(self.lvl_now);
        self.hist[self.hist_at] = l_db;
        self.hist_at = (self.hist_at + 1) % MUTE_WIN;
        let max60 = self.hist.iter().copied().fold(f64::MIN, f64::max);
        let all = slot_all(v[p::CHORD]);
        let auto = idx(v[p::TRIGGER], 2) == 0;
        let feed = self.b(p::FEED);
        let bypassed = self.b(p::BYPASS);
        let release_s = v[p::RELEASE].clamp(20.0, 2000.0) as f64 * 1e-3;
        let since_onset = self.onset_at.map_or(f64::INFINITY, |t| (now - t) as f64 / sr);

        // ---- bypass: trails = a release of every voice, else a 10 ms fade; the dry goes to unity
        if bypassed != self.was_bypassed {
            self.was_bypassed = bypassed;
            self.byp_x.set_target(if bypassed { 1.0 } else { 0.0 });
            if bypassed {
                let len = if self.b(p::TRAILS) { release_s } else { BYPASS_MS as f64 * 1e-3 };
                self.release_all(len);
            } else {
                self.spent = false;
            }
        }
        // ---- the footswitch gesture (Switch)
        if feed != self.was_feed {
            self.was_feed = feed;
            if !auto {
                if feed {
                    self.spent = false;
                } else {
                    self.release_all(release_s);
                }
            }
        }
        let allowed = !bypassed && (auto || feed);

        // ---- a pick: the old note releases, the analysis restarts
        if std::mem::take(&mut self.pick_flag) {
            self.onset_at = Some(now);
            self.mp.onset();
            self.spent = false;
            self.release_all(release_s);
        }
        // ---- legato: `carry` moves the voice, else it is a new note
        if let Some(h) = self.jump_flag.take() {
            if since_onset > JUMP_AFTER_PICK_S {
                let main = (0..NV).find(|&i| self.voices[i].active() && self.voices[i].slot == 0);
                if self.b(p::CARRY) && !all && main.is_some() {
                    let i = main.unwrap_or(0);
                    // the reading the jump was confirmed on, or YIN's own when it agrees
                    let f = if self.yin_clear && cents(self.yin_hz, h).abs() <= 50.0 { self.yin_hz } else { h };
                    let vc = &mut self.voices[i];
                    vc.tr.engage(if vc.sounding() { Engage::Keep } else { Engage::Snap });
                    vc.hz = f;
                    vc.tr.set_f0(f as f32, true);
                    // the partial levels are read again on the new note
                    vc.body_taken = false;
                    vc.t_on = now.saturating_sub((BODY_S * sr) as u64);
                } else {
                    self.onset_at = Some(now);
                    self.mp.onset();
                    self.spent = false;
                    self.release_all(release_s);
                }
            }
        }
        let since_onset = self.onset_at.map_or(f64::INFINITY, |t| (now - t) as f64 / sr);
        let any_active = self.voices.iter().any(|x| x.active());

        // ---- end of note: mute (two rules), the onset's own transient excepted
        if any_active && since_onset > BODY_S {
            let ms_v = (v[p::MUTE_SENS] as f64 * 0.01).clamp(0.0, 1.0);
            let d = MUTE_D0 + (MUTE_D1 - MUTE_D0) * ms_v;
            let drop = max60 - l_db;
            if drop > d || (l_db < thresh && drop > MUTE_QUIET_DB) {
                self.release_all(release_s);
                self.spent = true;
            }
        }
        // ---- max_time (from the start of Rise; 60 = no limit); a waiting note that died
        let max_t = v[p::MAX_TIME] as f64;
        for i in 0..NV {
            let x = &self.voices[i];
            if matches!(x.st, State::Rise | State::Hold) && max_t < 59.999 && (now - x.t_rise) as f64 / sr >= max_t {
                self.release_voice(i, release_s);
                self.spent = true;
            } else if x.st == State::Wait && l_db < thresh - HOLD_UNDER as f64 {
                self.voices[i].st = State::Idle;
            } else if x.st == State::Wait && (now - x.t_on) as f64 / sr > (v[p::DELAY] as f64 * 1e-3).max(CHORD_WAIT_S) + WAIT_LOCK_S {
                // never locked (noise, an unstable pitch): the note is given up until the next onset
                self.voices[i].st = State::Idle;
                self.spent = true;
            }
        }

        // ---- start: a confirmed note above `thresh`
        if allowed && !self.spent && l_db >= thresh {
            self.start_notes(now, since_onset, all);
        }

        // ---- multipitch updates: feed the voices it named
        let mp_n = self.mp.analyses();
        if mp_n != self.mp_seen {
            self.mp_seen = mp_n;
            let notes: [f64; 6] = core::array::from_fn(|k| self.mp.notes().get(k).map_or(0.0, |n| n.hz as f64));
            let lvl_ok = self.lvl_now >= self.thresh_lin;
            for x in self.voices.iter_mut().filter(|x| x.active() && x.from_mp && lvl_ok) {
                let fx = x.f0(sr);
                if let Some(&f) = notes.iter().filter(|&&f| f > 0.0).min_by(|a, b| cents(**a, fx).abs().total_cmp(&cents(**b, fx).abs())) {
                    if cents(f, fx).abs() <= FEED_CENTS {
                        x.hz = f;
                        x.tr.set_f0(f as f32, true);
                    }
                }
            }
        }

        // ---- smoothed controls at this tick
        let rise_s = self.rise.value() as f64 * 1e-3;
        let bloom = 10f64.powf(self.bloom.value() as f64 / 20.0);
        let duck = (self.duck.value() as f64 * 0.01).clamp(0.0, 1.0);
        let tone = (self.tone.value() as f64 * 0.01).clamp(-1.0, 1.0);
        let mode = idx(v[p::MODE], 3);
        let fb_on = mode != 1;
        let bank_on = mode != 0;
        let harmonic = idx(v[p::HARMONIC], 9);
        let set = mode_set(harmonic);
        // drift: a slow seeded walk of the distance
        let dr = self.drift_lfo.next() as f64;
        self.drift_m = dr * DRIFT_M * (self.drift.value() as f64 * 0.01);
        let dist = (self.distance.value() as f64 + self.drift_m).max(0.0);
        let tau = loop_tau(dist);
        let gamma = self.gamma_dry;

        // ---- per voice
        let order = self.voice_order();
        for oi in 0..NV {
            let i = order[oi];
            if !self.voices[i].active() {
                continue;
            }
            // ownership of shared partials (`chord` All): a partial within 0.4 f0 of a lower note's
            // partial belongs to the lower note
            let mut own = [true; KMAX + 1];
            if all {
                let f_v = self.voices[i].f0(sr);
                for &j in order[..oi].iter() {
                    let u = &self.voices[j];
                    if !u.active() || u.slot == self.voices[i].slot {
                        continue;
                    }
                    let f_u = u.f0(sr);
                    if f_u <= 0.0 || f_u >= f_v {
                        continue;
                    }
                    for k in 1..=KMAX {
                        let r = k as f64 * f_v / f_u;
                        if (r - r.round()).abs() * f_u < 0.4 * f_u && r.round() >= 1.0 {
                            own[k] = false;
                        }
                    }
                }
            }
            self.voice_control(i, now, since_onset, rise_s, bloom, duck, tone, fb_on, bank_on, harmonic, &set, tau, gamma, &own, all);
        }
        // releases end; the bloom's progress for `duck` (a release fades it too)
        let mut prog = 0.0f64;
        for x in self.voices.iter_mut() {
            if x.st == State::Release && x.rel_n >= x.rel_len {
                x.st = State::Idle;
            }
            match x.st {
                State::Rise | State::Hold => prog = prog.max(x.prog),
                State::Release => {
                    let r = x.rel_n as f64 / x.rel_len.max(1) as f64;
                    prog = prog.max(x.prog * 0.5 * (1.0 + (std::f64::consts::PI * r.min(1.0)).cos()));
                }
                _ => {}
            }
        }
        self.prog = prog;
        // the level window: one period of the main voice's note, else of `lowest`
        let f_main = self.voices.iter().filter(|x| x.active()).map(|x| x.f0(sr)).fold(0.0, f64::max);
        let per = if f_main > 0.0 { sr / f_main } else { sr / self.lowest_set.max(30.0) as f64 };
        let w = (per.ceil() as usize + 1).max(1);
        if w != self.lvl_win {
            self.lvl_win = w;
            self.lvl.set_window(w);
        }
        let _ = ms;
        self.meters(fb_on);
    }

    /// Voices sorted by their note (lowest first): ownership of shared partials.
    fn voice_order(&self) -> [usize; NV] {
        let sr = self.sr as f64;
        let mut o: [usize; NV] = core::array::from_fn(|i| i);
        o.sort_by(|&a, &b| {
            let fa = if self.voices[a].active() { self.voices[a].f0(sr) } else { f64::MAX };
            let fb = if self.voices[b].active() { self.voices[b].f0(sr) } else { f64::MAX };
            fa.total_cmp(&fb)
        });
        o
    }

    fn start_notes(&mut self, now: u64, since_onset: f64, all: bool) {
        let sr = self.sr as f64;
        let t_on = match self.onset_at {
            Some(t) if since_onset < 0.2 => t,
            _ => now,
        };
        let yin_ok = self.yin_clear && since_onset >= self.yin_wait() && self.yin_at + (0.01 * sr) as u64 >= now;
        let has_mp = !self.mp.notes().is_empty();
        if all && has_mp && !yin_ok {
            // a voice per named note; a waiting YIN voice gives way to the chord's
            let notes: [f64; 6] = core::array::from_fn(|k| self.mp.notes().get(k).map_or(0.0, |n| n.hz as f64));
            // a waiting voice that is not a note of this reading gives way (YIN's, or a note an earlier
            // reading named and this one revised)
            for i in 0..NV {
                let x = &self.voices[i];
                if x.st == State::Wait && (!x.from_mp || !notes.iter().any(|&f| f > 0.0 && cents(f, x.hz).abs() <= 60.0)) {
                    self.voices[i].st = State::Idle;
                }
            }
            for (k, &f) in notes.iter().enumerate().take(MAX_SLOTS) {
                if f <= 0.0 {
                    continue;
                }
                let taken = self.voices.iter().any(|x| x.active() && cents(f, x.hz).abs() <= 60.0);
                if !taken {
                    let slot = (0..MAX_SLOTS).find(|s| !self.voices.iter().any(|x| x.active() && x.slot == *s)).unwrap_or(k);
                    self.start_voice(f, true, slot, t_on);
                }
            }
            return;
        }
        if self.voices.iter().any(|x| x.active()) {
            return;
        }
        if yin_ok {
            self.start_voice(self.yin_hz, false, 0, t_on);
        } else if has_mp {
            if let Some(f) = self.choose() {
                self.start_voice(f, true, 0, t_on);
            }
        } else if self.onset_at.is_some() && since_onset >= CHORD_WAIT_S && self.yin_voiced {
            // a strum still unnamed 400 ms after its onset: YIN's candidate
            self.start_voice(self.yin_hz, false, 0, t_on);
        }
    }

    fn voice_control(
        &mut self,
        i: usize,
        now: u64,
        since_onset: f64,
        rise_s: f64,
        bloom: f64,
        duck: f64,
        tone: f64,
        fb_on: bool,
        bank_on: bool,
        harmonic: usize,
        set: &[bool; NM],
        tau: f64,
        gamma: f64,
        own: &[bool; KMAX + 1],
        all: bool,
    ) {
        let sr = self.sr as f64;
        let ctrl = self.ctrl_n as f64;
        let dt = CTRL_MS * 1e-3;
        let lvl = self.lvl_now;
        let yin_clear_now = self.yin_clear;
        let chosen = if all { None } else { self.choose() };
        let chosen_narrow = chosen.map_or(1, |f| self.chord_narrow(f));
        let n_tr = {
            let f = self.voices[i].f0(sr);
            self.partials_needed(f, all)
        };
        let delay = self.v[p::DELAY] as f64 * 1e-3;
        let follow_min = 10f64.powf((self.v[p::THRESH] - HOLD_UNDER + HOLD_MARGIN_DB) as f64 / 20.0);
        let v = &mut self.voices[i];
        // never drop a partial that still sounds (its phase would restart)
        let n_tr = n_tr.max(v.kmax.saturating_sub(1)).min(KMAX - 1);
        v.tr.set_partials(n_tr);
        let f0 = v.f0(sr);
        let kb = if bank_on { bank_k(f0, sr, all) } else { 0 };

        if v.st == State::Wait {
            let t = (now - v.t_on) as f64 / sr;
            let locked = v.tr.reference().locked;
            // the body: the note's level 80 ms after the onset (or at lock, if later), each partial's d_k
            if !v.body_taken && t >= BODY_S && locked {
                v.body_taken = true;
                v.body = lvl;
                for k in 1..=KMAX {
                    v.bk[k] = v.d(k);
                }
                // weak fundamental: the reference loop runs on partial 2 or 3
                let (b2, b3) = (v.bk[2], v.bk[3]);
                if v.bk[1] < 0.1 * b2.max(b3) {
                    v.tr.set_reference(if b2 >= b3 { 2 } else { 3 });
                }
            }
            // a waiting chord voice follows the chord's latest reading (the first one may be revised)
            if v.from_mp && !all {
                if let Some(f) = chosen {
                    if cents(f, v.f0(sr)).abs() > 50.0 {
                        v.tr.set_narrow(chosen_narrow);
                        v.hz = f;
                        v.tr.set_f0(f as f32, true);
                        v.body_taken = false;
                    }
                }
            }
            // chords: the pitch must be named (YIN clear, or multipitch, or 400 ms)
            let mut pitch_ok = v.from_mp || yin_clear_now || since_onset >= CHORD_WAIT_S;
            if !pitch_ok {
                if let Some(f) = chosen {
                    v.tr.set_narrow(chosen_narrow);
                    if cents(f, v.hz).abs() > 50.0 {
                        v.hz = f;
                        v.tr.set_f0(f as f32, true);
                    }
                    v.from_mp = true;
                    pitch_ok = true;
                }
            }
            if t >= delay && pitch_ok && v.body_taken && locked && v.body > 1e-9 {
                // ---- Rise
                v.st = State::Rise;
                v.t_rise = now;
                v.tr.engage(Engage::Keep);
                v.a_ref = v.body * bloom;
            } else {
                return;
            }
        }

        // ---- Rise / Hold
        // the string fading into the noise: the voice stops following it 12 dB above the hold level
        // and holds the frequency it learned there (the loop's own wander grows as the SNR falls)
        if !v.tr.is_held() && (v.tr.reference().level as f64) < follow_min {
            v.tr.hold();
        }
        if v.st == State::Rise && (now - v.t_rise) as f64 / sr >= rise_s {
            v.st = State::Hold;
            v.hold = true;
        }
        let a_t = v.body * bloom;
        v.a_ref = a_t;
        let seed_min = a_t * 10f64.powf(SEED_DB / 20.0);
        // Feedback modes
        if fb_on {
            // seeds: a mode in the set with no level starts from the string's partial as heard, at least
            // 40 dB under the target, never above it
            let mut new_seed = false;
            for m in 0..NM {
                if set[m] && v.a_fb[m] <= 0.0 {
                    let d = if m == 0 { 0.0 } else { gamma * v.d(m) };
                    v.a_fb[m] = d.max(seed_min).min(a_t);
                    new_seed = true;
                }
            }
            if new_seed {
                let mx = (0..NM).filter(|&m| set[m]).map(|m| v.a_fb[m]).fold(0.0, f64::max);
                v.u0 = (mx / a_t).powi(2);
            }
            // loop gains
            let mut g = [0.0f64; NM];
            match harmonic {
                6 | 7 => {
                    let mut fk = [0.0f64; NM];
                    for k in 1..NM {
                        let f = if k == 1 { f0 } else { v.tr.partial(k).inc * sr };
                        fk[k] = if f > 0.0 { f } else { k as f64 * f0 };
                    }
                    g = natural_gains(harmonic == 7, &fk, tau);
                }
                8 => {
                    let climbed = (now - v.t_rise) as f64 / sr >= rise_s + CLIMB_S;
                    g[1] = if climbed { CLIMB_G1 } else { 1.0 };
                    g[2] = if climbed { 1.0 } else { 0.0 };
                }
                _ => {
                    for m in 0..NM {
                        g[m] = if set[m] { 1.0 } else { 0.0 };
                    }
                }
            }
            let beta = beta_for(v.u0, rise_s);
            let bj = if v.hold { beta.max(BETA_JUMP) } else { beta };
            v.prog = compete(&mut v.a_fb, a_t, &g, set, bj, dt);
        } else {
            for m in 0..NM {
                v.a_fb[m] *= (-OUT_RATE * dt).exp();
                if v.a_fb[m] < a_t * 1e-5 {
                    v.a_fb[m] = 0.0;
                }
            }
            v.prog = 0.0;
        }
        // the bank
        let duck_f = if fb_on { 1.0 - duck * v.prog } else { 1.0 };
        for k in 1..=KMAX {
            if bank_on && k <= kb {
                let ak = (bloom * v.bk[k] * (k as f64).powf(tone) * duck_f).max(1e-12);
                if v.a_b[k] <= 0.0 {
                    let d = gamma * v.d(k);
                    v.a_b[k] = d.max(ak * 1e-2).min(ak);
                    v.u0b[k] = (v.a_b[k] / ak).powi(2);
                }
                let uk = (v.a_b[k] / ak).powi(2);
                let bk = beta_for(v.u0b[k], rise_s);
                v.a_b[k] = (v.a_b[k] * (bk * (1.0 - uk) * dt).exp()).min(4.0 * ak);
            } else if v.a_b[k] > 0.0 {
                v.a_b[k] *= (-OUT_RATE * dt).exp();
                if v.a_b[k] < 1e-9 {
                    v.a_b[k] = 0.0;
                }
            }
        }
        // commanded levels per partial, ramped over the next tick
        let mut kmax = 1;
        for k in 0..=KMAX {
            let fb = if k < NM { v.a_fb[k] } else { 0.0 };
            let bk = if k >= 1 { v.a_b[k] } else { 0.0 };
            let t = if own[k] || k == 0 { fb.max(bk) } else { 0.0 };
            v.step[k] = (t - v.amp[k]) / ctrl;
            if k < NM {
                // the colour follows the Feedback mode's own level: no switch when a mode leaves
                let tf = if own[k] || k == 0 { fb } else { 0.0 };
                v.fbs[k] = (tf - v.fba[k]) / ctrl;
            }
            if t > 0.0 || v.amp[k] > 0.0 {
                kmax = kmax.max(k);
            }
        }
        v.kmax = kmax;
    }

    fn meters(&mut self, fb_on: bool) {
        let sr = self.sr as f64;
        match self.loudest() {
            Some(i) => {
                let v = &self.voices[i];
                self.m_state = v.st;
                self.m_note = v.f0(sr) as f32;
                self.m_lock = if v.tr.reference().locked { 1.0 } else { 0.0 };
                self.m_harm = if !v.sounding() {
                    0.0
                } else if fb_on {
                    let (mut b, mut bv) = (1usize, 0.0);
                    for m in 0..NM {
                        if v.a_fb[m] > bv {
                            bv = v.a_fb[m];
                            b = m;
                        }
                    }
                    if b == 0 {
                        0.5
                    } else {
                        b as f32
                    }
                } else {
                    1.0
                };
            }
            None => {
                self.m_state = State::Idle;
                self.m_note = 0.0;
                self.m_harm = 0.0;
                self.m_lock = 0.0;
            }
        }
    }

    // ---------------------------------------------------------------- audio

    /// One voice, one sample: the generated signal.
    #[inline]
    fn synth(v: &mut Voice, xd: f32, s: &Synth) -> f64 {
        let r = v.tr.process(xd);
        if !v.sounding() {
            return 0.0;
        }
        let rel = if v.st == State::Release {
            let x = v.rel_n as f64 / v.rel_len as f64;
            v.rel_n += 1;
            if x >= 1.0 {
                0.0
            } else {
                0.5 * (1.0 + (std::f64::consts::PI * x).cos())
            }
        } else {
            1.0
        };
        let body = v.body.max(1e-9);
        let mut out = 0.0f64;
        // the sub: its own accumulator at half the reference's increment
        v.sub_psi = (v.sub_psi + 0.5 * r.inc).rem_euclid(1.0);
        v.amp[0] += v.step[0];
        for m in 0..NM {
            v.fba[m] += v.fbs[m];
        }
        if v.amp[0] > 0.0 {
            let g = v.amp[0] * rel;
            out += Self::phasor(g, g, cos_turns(v.sub_psi) as f64, 0.5 * r.inc, s.kappa, body, s.nyq);
        }
        for k in 1..=v.kmax {
            v.amp[k] += v.step[k];
            let a = v.amp[k];
            if a <= 0.0 {
                continue;
            }
            let l = if k == 1 { r } else { v.tr.partial(k) };
            if l.inc <= 0.0 || l.inc >= s.nyq {
                continue;
            }
            // the string's partial as heard; near the hold level it fades out of the subtraction (under
            // it the level is noise, not the string), softly: no step when the level or `thresh` moves
            let lv = l.level as f64;
            let d = lv * ((lv - s.floor) * s.inv_floor).clamp(0.0, 1.0);
            let g = (a - s.gamma * d).max(0.0) * rel;
            if g <= 0.0 {
                continue;
            }
            let gc = if k < NM { g.min(v.fba[k] * rel).max(0.0) } else { 0.0 };
            // the tracker's own phasor for k ≥ 2 (decimated loops), else the polynomial
            let c = v.tr.partial_cos(k).unwrap_or_else(|| cos_turns(l.psi) as f64);
            out += Self::phasor(g, gc, c, l.inc, s.kappa, body, s.nyq);
        }
        out
    }

    /// g · cos φ + g_c · (c₂ cos 2φ + c₃ cos 3φ): the colour on the Feedback share g_c of the partial,
    /// c₂ = κ · g_c / b, c₃ = c₂² / COLOR_K (Chebyshev: c_n ∝ (g / b)^(n−1); terms above 0.45 fs
    /// dropped).
    #[inline]
    fn phasor(g: f64, gc: f64, c: f64, inc: f64, kappa: f64, body: f64, nyq: f64) -> f64 {
        let kap = kappa * gc / body;
        if kap <= 0.0 || gc <= 0.0 {
            return g * c;
        }
        let mut y = g * c;
        if 2.0 * inc < nyq {
            y += gc * kap * (2.0 * c * c - 1.0);
            if 3.0 * inc < nyq {
                y += gc * kap * kap / COLOR_K * (4.0 * c * c * c - 3.0 * c);
            }
        }
        y
    }

    pub fn process_with_key(&mut self, buffers: &mut [&mut [f32]], key: Option<&[&[f32]]>) {
        let nch = buffers.len().min(self.nch).min(2);
        if nch == 0 {
            return;
        }
        let n = buffers[0].len();
        let sr = self.sr;
        // the key: Main unless `source` Key and a key that has not been silent for 1 s
        let mut key = if idx(self.v[p::SOURCE], 2) == 1 { key.filter(|k| !k.is_empty()) } else { None };
        if let Some(k) = key {
            if k.iter().any(|c| c.iter().any(|&s| s != 0.0)) {
                self.key_silent = 0;
            } else {
                self.key_silent += n as u64;
            }
            if self.key_silent as f64 >= KEY_SILENT_S * sr as f64 {
                key = None;
            }
        }
        let ksc = key.map_or(1.0, |k| 1.0 / k.len() as f32);
        let msc = 1.0 / nch as f32;
        let nyq = 0.45;
        for i in 0..n {
            self.clock += 1;
            let xd = match key {
                Some(k) => k.iter().map(|c| c.get(i).copied().unwrap_or(0.0)).sum::<f32>() * ksc,
                None => (0..nch).map(|c| buffers[c][i]).sum::<f32>() * msc,
            };
            self.detect(xd);
            // smoothed controls (every sample: block-size independent)
            self.rise.next();
            self.bloom.next();
            self.duck.next();
            self.distance.next();
            self.color.next();
            self.tone.next();
            self.drift.next();
            let dry_db = self.dry.next();
            let byp = self.byp_x.next() as f64;
            if self.clock.is_multiple_of(self.ctrl_n) {
                self.control();
            }
            // dry gain: `dry` (−60 = off), lowered by `duck` as the bloom grows, unity in bypass
            if dry_db != self.dry_db {
                self.dry_db = dry_db;
                self.dry_lin = if dry_db <= -59.99 { 0.0 } else { 10f64.powf(dry_db as f64 / 20.0) };
            }
            let duck = (self.duck.value() as f64 * 0.01).clamp(0.0, 1.0);
            let gamma = self.dry_lin * (1.0 - duck * self.prog);
            self.gamma_dry = gamma;
            let g_out = gamma + (1.0 - gamma) * byp;
            let kappa = COLOR_K * (self.color.value() as f64 / 50.0).clamp(0.0, 2.0);
            // the hold level glides (30 ms): a `thresh` step moves no subtraction at once
            self.hold_s += self.hold_a * (self.hold_lin - self.hold_s);
            let s = Synth { gamma, floor: self.hold_s, inv_floor: 1.0 / self.hold_s.max(1e-12), kappa, nyq };
            let mut gen = 0.0f64;
            for v in self.voices.iter_mut() {
                if v.st != State::Idle {
                    gen += Self::synth(v, xd, &s);
                }
            }
            let gen = gen as f32;
            self.last_gen = gen;
            for c in 0..nch {
                buffers[c][i] = buffers[c][i] * g_out as f32 + gen;
            }
        }
    }
}

/// The comb length (periods) for a voice on `f` inside a chord whose other notes are `others`: the
/// shortest n ≤ 6 whose zeros (every f / n) land on the other notes' first three partials (a fifth
/// above or below: 2, a major third: 4 or 5); the leakage of each partial is |sinc(n (g / f − 1))| / j.
pub fn chord_comb(f: f64, others: &[f64]) -> usize {
    let mut best = (1usize, f64::MAX);
    for n in 1..=CHORD_COMB_MAX {
        let mut worst = 0.0f64;
        for &g0 in others.iter().filter(|&&g| g > 0.0 && cents(g, f).abs() > 50.0) {
            for j in 1..=3 {
                let r = j as f64 * g0 / f - 1.0;
                // a partial on the voice's own frequency (an octave below) is shared, not a neighbour
                if r.abs() < 0.01 {
                    continue;
                }
                let x = n as f64 * r;
                let l = ((std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)).abs();
                worst = worst.max(l / j as f64);
            }
        }
        // a shorter comb unless a longer one is clearly cleaner
        if worst < best.1 * 0.7 {
            best = (n, worst);
        }
    }
    best.0
}

/// `chord` All.
#[inline]
fn slot_all(v: f32) -> bool {
    idx(v, 4) == 3
}

/// The bank's K: min(16, ⌊min(5 kHz, 0.45 fs) / f0⌋), 8 per note in `chord` All.
#[inline]
fn bank_k(f0: f64, sr: f64, all: bool) -> usize {
    if f0 <= 0.0 {
        return 1;
    }
    let top = BANK_TOP_HZ.min(0.45 * sr);
    ((top / f0).floor() as usize).clamp(1, if all { K_ALL } else { KMAX })
}

impl Effect for Sustainer {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate;
        self.nch = channels.clamp(1, 2);
        let srd = sample_rate as f64;
        self.ctrl_n = (CTRL_MS * 1e-3 * srd).round().max(1.0) as u64;
        for e in [&mut self.rise, &mut self.bloom, &mut self.dry, &mut self.duck, &mut self.distance] {
            e.prepare(sample_rate);
        }
        for o in [&mut self.color, &mut self.tone, &mut self.drift] {
            o.prepare(sample_rate);
        }
        self.byp_x.prepare(sample_rate);
        self.hold_a = 1.0 - (-1.0 / (0.03 * srd)).exp();
        // YIN's decimator (gfx-dsp `detect`'s design) and gfx-tuner's long window at a quarter rate
        self.dec = (sample_rate / YIN_RATE).round().max(1.0) as usize;
        self.fd = sample_rate / self.dec as f32;
        let fc = (0.2 * self.fd).min(2500.0);
        for s in self.aa.iter_mut() {
            s.set(SvfMode::LowPass, fc, std::f32::consts::FRAC_1_SQRT_2, 0.0, sample_rate);
        }
        self.lp4[0].set(SvfMode::LowPass, 600.0, 0.541_196, 0.0, self.fd);
        self.lp4[1].set(SvfMode::LowPass, 600.0, 1.306_563, 0.0, self.fd);
        self.short.prepare_decimated(self.fd, LONG_BELOW, YIN_MAX, true);
        self.long.prepare_decimated(self.fd / 4.0, 28.0, 150.0, true);
        self.long.set_hop(8);
        self.pick.prepare(sample_rate);
        self.jump.prepare(sample_rate);
        self.mp.prepare(sample_rate, 30.0, MAX_SLOTS);
        self.lvl.prepare((srd / 25.0).ceil() as usize + 2);
        for v in self.voices.iter_mut() {
            v.tr.prepare(sample_rate, KMAX - 1);
            // the partials' loops every 16 samples at most (README §2.6: "the baseband filters and loops
            // may run at fs / 8"; the first two one-poles and the mixer stay at fs)
            v.tr.set_partial_rate(PART_RATE);
            // the held pitch is the one measured 12 dB above the hold level (under hiss the plain
            // 150 ms mean drifted)
            v.tr.set_hold_margin(Some(HOLD_MARGIN_DB));
            v.tr.prepare_narrow(CHORD_COMB_MAX, 30.0);
        }
        self.drift_lfo.set_rate(DRIFT_HZ, (1e3 / CTRL_MS) as f32);
        self.reset();
    }

    fn reset(&mut self) {
        let v = self.v;
        for (i, e) in [(p::RISE, &mut self.rise), (p::BLOOM, &mut self.bloom), (p::DRY, &mut self.dry), (p::DUCK, &mut self.duck), (p::DISTANCE, &mut self.distance)] {
            e.settle(v[i]);
            e.reset();
        }
        for (i, o) in [(p::COLOR, &mut self.color), (p::TONE, &mut self.tone), (p::DRIFT, &mut self.drift)] {
            o.set_target(v[i]);
            o.settle();
        }
        self.clock = 0;
        self.was_bypassed = self.b(p::BYPASS);
        self.byp_x.set_immediate(if self.was_bypassed { 1.0 } else { 0.0 });
        self.was_feed = self.b(p::FEED);
        self.gamma_dry = 1.0;
        for s in self.aa.iter_mut() {
            s.reset();
        }
        for s in self.lp4.iter_mut() {
            s.reset();
        }
        self.dphase = 0;
        self.d4 = 0;
        self.short.reset();
        self.long.reset();
        self.lowest_set = -1.0;
        self.set_lowest(true);
        self.set_floors();
        self.hold_s = self.hold_lin;
        self.yin_hz = 0.0;
        self.yin_clear = false;
        self.yin_voiced = false;
        self.yin_at = 0;
        self.jump_at = 0;
        self.pick.reset();
        self.jump.reset();
        self.mp.reset();
        self.mp.set_f_min(self.lowest_set);
        self.mp_seen = 0;
        self.lvl.reset();
        self.lvl_win = 0;
        self.lvl_now = 0.0;
        self.hist = [-180.0; MUTE_WIN];
        self.hist_at = 0;
        self.key_silent = 0;
        self.dry_db = f32::NAN;
        self.prog = 0.0;
        self.onset_at = None;
        self.pick_flag = false;
        self.jump_flag = None;
        self.spent = false;
        for x in self.voices.iter_mut() {
            x.tr.reset();
            x.st = State::Idle;
            x.amp = [0.0; KMAX + 1];
            x.step = [0.0; KMAX + 1];
        }
        self.drift_lfo.reset();
        self.drift_m = 0.0;
        self.m_state = State::Idle;
        self.m_note = 0.0;
        self.m_harm = 0.0;
        self.m_lock = 0.0;
        self.last_gen = 0.0;
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

    fn meter(&self, id: &str) -> Option<f32> {
        match id {
            "state" => Some(self.m_state as u8 as f32),
            "note" => Some(self.m_note),
            "harm" => Some(self.m_harm),
            "lock" => Some(self.m_lock),
            _ => None,
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_with_key(buffers, None);
    }

    fn process_with_key(&mut self, buffers: &mut [&mut [f32]], key: Option<&[&[f32]]>) {
        Sustainer::process_with_key(self, buffers, key);
    }
}
