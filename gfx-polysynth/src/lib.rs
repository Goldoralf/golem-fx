// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-polysynth — golem's polyphonic guitar synthesizer (design: README.md; v1 = Spectral + Mono).
//!
//! - **Spectral core** (spectral.rs, at a bank rate of 22.05 – 24 kHz at every host rate, between gfx-dsp
//!   `oversample` half-band cascades): a fast and a clean complex filterbank, one band per partial,
//!   each band turned into a waveform at its own phase (harmonic shaper), a per-band hold, the
//!   event-driven fast → clean handover, the ownership mask of Mono's voice.
//! - **Mono** (side chain at the host rate): YIN (gfx-dsp `detect`), the pick and legato detectors
//!   (gfx-dsp `onset` A and C), gfx-dsp `multipitch` for the lowest note of a chord, one gfx-dsp
//!   `track::Tracker` per voice slot (two: a release overlaps the next note). The voice: two gfx-dsp
//!   `osc` oscillators (`detune`), sub, noise, its own `vcf`; it takes over from the core on YIN's
//!   reading (the bridge: envelopes from the pick, 10 ms fade, level matched to the core bands it
//!   replaces, its oscillator started on the core band's phase).
//! - **Filter and envelopes**: gfx-dsp `vcf` (drive → SVF × 1 / × 2), cut-off moved by an AD filter
//!   envelope (`filt_env`, velocity) and `key_track` (voices); amp: Follow (the string's level through
//!   `compress`, `attack` as the minimum rise) or Gate (an ADSR from every pick).
//! - **Output**: `synth` gain → a ceiling (transparent under −6 dBFS, an ADAA tanh above it, never
//!   past 0 dBFS) → + the dry (`dry`, sample-aligned, untouched).
//!
//! Zero reported latency (the synth path has the half-bands' few samples and the bands' own lags).
//! The 2 / 2 layout processes the mono sum; `spread` places the unison pair. Real-time safe,
//! deterministic, block-size independent.

#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

pub mod spectral;

#[cfg(feature = "plugin")]
mod plugin;

#[cfg(test)]
mod tests;

use crate::spectral::{coefs, BankMode, Core, CoreSettings, NH};
use gfx_dsp::adaa::{Adaa1, HalfSample, Shaper};
use gfx_dsp::detect::PitchDetector;
use gfx_dsp::envelope::SlidingMax;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::multipitch::Analyzer;
use gfx_dsp::noise::White;
use gfx_dsp::onset::{Pick, PitchJump};
use gfx_dsp::osc::{pulse, pulse_width, saw, triangle, Phasor};
use gfx_dsp::oversample::{OversamplerN, MAX_FACTOR};
use gfx_dsp::smooth::OnePole;
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::track::{Engage, Tracker};
use gfx_dsp::vcf::{q_of, Vcf, VcfType};
use gfx_dsp::{Effect, ParamInfo, ParamKind};
use std::f64::consts::{FRAC_1_SQRT_2, PI, SQRT_2, TAU};

pub const NP: usize = 34;

const fn int(id: &'static str, name: &'static str, min: f32, max: f32, default: f32) -> ParamInfo {
    ParamInfo { id, name, min, max, default, kind: ParamKind::Int, exp: false }
}

pub static PARAMS: [ParamInfo; NP] = [
    // front panel
    ParamInfo::enumeration("engine", "Engine", 2, 0),
    ParamInfo::enumeration("wave", "Wave", 4, 0),
    ParamInfo::float("cutoff", "Cutoff", 40.0, 12000.0, 3000.0).with_exp(),
    ParamInfo::float("reso", "Resonance", 0.0, 100.0, 20.0).with_exp(),
    ParamInfo::float("filt_env", "Env Amount", -48.0, 48.0, 24.0).with_exp(),
    ParamInfo::float("synth", "Synth", -60.0, 12.0, 0.0).with_exp(),
    ParamInfo::float("dry", "Dry", -60.0, 0.0, -60.0).with_exp(),
    // expert
    ParamInfo::float("pw", "Pulse Width", 5.0, 95.0, 50.0).with_exp(),
    int("octave", "Octave", -2.0, 2.0, 0.0),
    ParamInfo::float("sub", "Sub", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("detune", "Detune", 0.0, 50.0, 0.0).with_exp(),
    ParamInfo::float("noise", "Noise", 0.0, 100.0, 0.0),
    ParamInfo::enumeration("amp_mode", "Amp Mode", 2, 0),
    ParamInfo::float("attack", "Attack", 0.0, 2000.0, 2.0).with_exp(),
    ParamInfo::float("decay", "Decay", 5.0, 4000.0, 400.0),
    ParamInfo::float("sustain", "Sustain", 0.0, 100.0, 70.0),
    ParamInfo::float("release", "Release", 5.0, 4000.0, 200.0),
    ParamInfo::float("compress", "Hold", 0.0, 100.0, 40.0).with_exp(),
    ParamInfo::enumeration("type", "Filter Type", 3, 0),
    ParamInfo::enumeration("poles", "Poles", 2, 1),
    ParamInfo::float("drive", "Drive", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("key_track", "Key Track", 0.0, 100.0, 0.0),
    ParamInfo::float("fenv_att", "Env Attack", 0.0, 2000.0, 0.0),
    ParamInfo::float("fenv_dec", "Env Decay", 5.0, 4000.0, 300.0).with_exp(),
    ParamInfo::float("fenv_vel", "Env Velocity", 0.0, 100.0, 50.0),
    ParamInfo::enumeration("bank", "Resolution", 3, 0),
    ParamInfo::float("residue", "Residue", 0.0, 100.0, 100.0),
    ParamInfo::float("glide", "Glide", 0.0, 1000.0, 0.0).with_exp(),
    ParamInfo::enumeration("quantize", "Pitch", 2, 0),
    ParamInfo::float("lowest", "Lowest Note", 30.0, 150.0, 70.0),
    ParamInfo::float("thresh", "Noise Floor", -96.0, -30.0, -66.0),
    ParamInfo::float("spread", "Spread", 0.0, 100.0, 0.0),
    ParamInfo::boolean("bypass", "Bypass", false),
    ParamInfo::boolean("trails", "Trails", true),
];

pub mod p {
    pub const ENGINE: usize = 0;
    pub const WAVE: usize = 1;
    pub const CUTOFF: usize = 2;
    pub const RESO: usize = 3;
    pub const FILT_ENV: usize = 4;
    pub const SYNTH: usize = 5;
    pub const DRY: usize = 6;
    pub const PW: usize = 7;
    pub const OCTAVE: usize = 8;
    pub const SUB: usize = 9;
    pub const DETUNE: usize = 10;
    pub const NOISE: usize = 11;
    pub const AMP_MODE: usize = 12;
    pub const ATTACK: usize = 13;
    pub const DECAY: usize = 14;
    pub const SUSTAIN: usize = 15;
    pub const RELEASE: usize = 16;
    pub const COMPRESS: usize = 17;
    pub const TYPE: usize = 18;
    pub const POLES: usize = 19;
    pub const DRIVE: usize = 20;
    pub const KEY_TRACK: usize = 21;
    pub const FENV_ATT: usize = 22;
    pub const FENV_DEC: usize = 23;
    pub const FENV_VEL: usize = 24;
    pub const BANK: usize = 25;
    pub const RESIDUE: usize = 26;
    pub const GLIDE: usize = 27;
    pub const QUANTIZE: usize = 28;
    pub const LOWEST: usize = 29;
    pub const THRESH: usize = 30;
    pub const SPREAD: usize = 31;
    pub const BYPASS: usize = 32;
    pub const TRAILS: usize = 33;
}

/// Option labels (enum order, final; append only).
pub const ENGINES: [&str; 2] = ["Spectral", "Mono"];
pub const WAVES: [&str; 4] = ["Saw", "Pulse", "Triangle", "Sine"];
pub const AMP_MODES: [&str; 2] = ["Follow", "Gate"];
pub const TYPES: [&str; 3] = ["Low-pass", "Band-pass", "High-pass"];
pub const POLES: [&str; 2] = ["2", "4"];
pub const BANKS: [&str; 3] = ["Dual", "Fast", "Clean"];
pub const QUANTIZES: [&str; 2] = ["Off", "Chromatic"];

// ---------------------------------------------------------------- constants

/// The engine's control tick (ms).
const CTRL_MS: f64 = 1.0;
/// Structural changes (`wave`, `octave`, `type`, `poles`) crossfade over this (ms); `engine` over 30.
const XFADE_MS: f64 = 20.0;
const ENGINE_MS: f64 = 30.0;
/// Bypass without trails, and the dry's return to unity (ms).
const BYPASS_MS: f64 = 10.0;
/// The voice's fade-in at the handover (ms).
const VOICE_FADE_MS: f64 = 10.0;
/// The voice's frequency is smoothed by two one-poles of this (ms): the tracker's loop jitters by
/// ±10 – 80 ¢ from sample to sample on a held D3.
const FREQ_SMOOTH_MS: f64 = 3.0;
/// The bridge's ratio is frozen this many periods after the voice starts (the tracker's level is read
/// over one period).
const MATCH_PERIODS: f64 = 3.0;
/// … and at least this long (s): the core bands are still rising 10 ms into the voice.
const MATCH_MIN_S: f64 = 0.020;
/// YIN: a reading is clear above this clarity.
const CLEAR: f32 = 0.8;
const YIN_MAX: f32 = 1500.0;
/// A reading feeds the voice's tracker within this of its note (cents).
const FEED_CENTS: f64 = 45.0;
/// Two successive clear readings agree within this (cents) to start a voice.
const AGREE_CENTS: f64 = 50.0;
/// A chord waits for `multipitch` this long after the pick (s), then takes YIN's reading.
const CHORD_WAIT_S: f64 = 0.4;
/// The octave guard: a ×2 / ÷2 reading must hold this long (s) before the voice follows it.
const OCTAVE_CONFIRM_S: f64 = 0.040;
/// The pick's own transient: no note end, no legato in the first 80 ms.
const BODY_S: f64 = 0.080;
/// Mute: the level falls this far (dB) under its maximum of the last 60 ms.
const MUTE_DB: f64 = 24.0;
const MUTE_WIN: usize = 60;
/// The voice's tracker level under `thresh` − 6 dB ends the note.
const TRACK_UNDER_DB: f64 = 6.0;
/// The pick detector listens down to `thresh` − 24 dB.
const PICK_UNDER_DB: f64 = 24.0;
/// Quantize hysteresis (semitones beyond the half step).
const Q_HYST: f64 = 0.35;
/// The voice level's hold (compress) cap.
const COMP_CAP: f64 = 31.622_776_601_683_79;
/// `drive` fades its tanh in over its first this many dB.
const DRIVE_FADE_DB: f32 = 3.0;
/// Envelopes: release and decay fall this far (dB) over their time.
const ENV_RANGE_DB: f64 = 80.0;
/// The filter envelope falls 60 dB over `fenv_dec`.
const FENV_RANGE_DB: f64 = 60.0;

#[inline]
fn db_to_lin(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

#[inline]
fn to_db(x: f64) -> f64 {
    20.0 * x.max(1e-12).log10()
}

#[inline]
fn cents(a: f64, b: f64) -> f64 {
    1200.0 * (a.max(1e-9) / b.max(1e-9)).log2()
}

fn idx(v: f32, n: usize) -> usize {
    (v.round().max(0.0) as usize).min(n - 1)
}

// ---------------------------------------------------------------- envelopes

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Off,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// The amp ADSR (Gate): attack linear in amplitude, decay and release linear in dB.
#[derive(Clone, Debug)]
struct Adsr {
    st: Stage,
    v: f64,
}

impl Adsr {
    fn new() -> Self {
        Self { st: Stage::Off, v: 0.0 }
    }
    fn trigger(&mut self) {
        self.st = Stage::Attack;
    }
    fn gate_off(&mut self) {
        if self.st != Stage::Off {
            self.st = Stage::Release;
        }
    }
    fn held(&self) -> bool {
        matches!(self.st, Stage::Attack | Stage::Decay | Stage::Sustain)
    }
    #[inline]
    fn next(&mut self, att_s: f64, dec_s: f64, sus: f64, rel_s: f64, fs: f64) -> f64 {
        match self.st {
            Stage::Off => self.v = 0.0,
            Stage::Attack => {
                if att_s <= 0.0 {
                    self.v = 1.0;
                } else {
                    self.v += 1.0 / (att_s * fs);
                }
                if self.v >= 1.0 {
                    self.v = 1.0;
                    self.st = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.v *= db_to_lin(-ENV_RANGE_DB / (dec_s.max(1e-4) * fs));
                if self.v <= sus {
                    self.v = sus;
                    self.st = Stage::Sustain;
                }
            }
            Stage::Sustain => {
                // the sustain level follows its knob (smoothed by the caller)
                self.v = sus;
            }
            Stage::Release => {
                self.v *= db_to_lin(-ENV_RANGE_DB / (rel_s.max(1e-4) * fs));
                if self.v < 1e-4 {
                    self.v = 0.0;
                    self.st = Stage::Off;
                }
            }
        }
        self.v
    }
}

/// The filter envelope: an AD (attack linear, decay −60 dB over its time), 0 … 1. The attack time is
/// taken at the trigger (a `fenv_att` move mid-attack would jump the cut-off).
#[derive(Clone, Debug)]
struct FiltEnv {
    attacking: bool,
    att_s: f64,
    v: f64,
}

impl FiltEnv {
    fn new() -> Self {
        Self { attacking: false, att_s: 0.0, v: 0.0 }
    }
    fn trigger(&mut self, att_s: f64) {
        self.attacking = true;
        self.att_s = att_s;
    }
    #[inline]
    fn next(&mut self, dec_s: f64, fs: f64) -> f64 {
        let att_s = self.att_s;
        if self.attacking {
            if att_s <= 0.0 {
                self.v = 1.0;
            } else {
                self.v += 1.0 / (att_s * fs);
            }
            if self.v >= 1.0 {
                self.v = 1.0;
                self.attacking = false;
            }
        } else {
            self.v *= db_to_lin(-FENV_RANGE_DB / (dec_s.max(1e-4) * fs));
            if self.v < 1e-6 {
                self.v = 0.0;
            }
        }
        self.v
    }
}

/// `attack` as the minimum rise of a level (Follow): A rises at most to E in `attack`, falls with E.
#[derive(Clone, Debug, Default)]
struct Rise {
    a: f64,
}

impl Rise {
    #[inline]
    fn next(&mut self, e: f64, att_s: f64, fs: f64) -> f64 {
        if att_s <= 0.0 || e <= self.a {
            self.a = e;
        } else {
            self.a = (self.a + e / (att_s * fs)).min(e);
        }
        self.a
    }
}

// ---------------------------------------------------------------- filter with a structural crossfade

/// `vcf`'s drive (an ADAA1 tanh of x · k_d over √k_d), run ahead of the filter so that it can fade in:
/// under 3 dB the driven signal is blended with the input (moved to the ADAA's half-sample timing as it
/// fades), so `drive` reaching 0 does not switch paths (gfx-autowah's 0 dB switch stepped by 8 % on a −6 dBFS signal:
/// −31 dB of HF in the bench's step check).
#[derive(Clone, Debug, Default)]
struct Drive {
    adaa: Adaa1,
    half: HalfSample,
}

impl Drive {
    fn reset(&mut self) {
        self.adaa.reset(Shaper::Tanh);
        self.half.reset();
    }
    #[inline]
    fn process(&mut self, x: f32, drive_db: f32) -> f32 {
        let kd = 10f32.powf(drive_db.max(0.0) / 20.0);
        let shaped = self.adaa.process(Shaper::Tanh, x * kd) / kd.sqrt();
        let half = self.half.process(x);
        if drive_db <= 0.0 {
            return x;
        }
        // the input itself at 0 dB (the half-sample average would take 0.7 dB at 6 kHz), moving to the
        // ADAA's half-sample timing as the tanh fades in
        let w = (drive_db / DRIVE_FADE_DB).min(1.0);
        let lin = x + (half - x) * w;
        lin + (shaped - lin) * w
    }
}

/// A `vcf` whose type and slope change through a 20 ms crossfade to a second filter; the drive ahead.
#[derive(Clone, Debug)]
struct VcfX {
    drive: Drive,
    a: Vcf,
    b: Vcf,
    cfg: (VcfType, bool),
    next: (VcfType, bool),
    x: f64,
}

impl VcfX {
    fn new() -> Self {
        let mut drive = Drive::default();
        drive.reset();
        Self { drive, a: Vcf::new(), b: Vcf::new(), cfg: (VcfType::LowPass, true), next: (VcfType::LowPass, true), x: 0.0 }
    }
    fn reset(&mut self, cfg: (VcfType, bool)) {
        self.drive.reset();
        self.a.reset();
        self.b.reset();
        self.cfg = cfg;
        self.next = cfg;
        self.x = 0.0;
    }
    /// Starts a crossfade to `cfg` (ignored while one runs; the caller retries).
    fn want(&mut self, cfg: (VcfType, bool)) {
        if cfg != self.cfg && self.x == 0.0 {
            self.next = cfg;
            self.b.reset();
            self.x = 1e-9;
        }
    }
    #[inline]
    fn process(&mut self, x: f32, fc: f32, q: f32, drive_db: f32, sr: f32, step: f64) -> f32 {
        let x = self.drive.process(x, drive_db);
        self.a.set(fc, q, self.cfg.0, self.cfg.1, sr);
        let ya = self.a.process(x, 0.0);
        if self.x == 0.0 {
            return ya;
        }
        self.b.set(fc, q, self.next.0, self.next.1, sr);
        let yb = self.b.process(x, 0.0);
        self.x += step;
        if self.x >= 1.0 {
            std::mem::swap(&mut self.a, &mut self.b);
            self.cfg = self.next;
            self.x = 0.0;
            return yb;
        }
        let w = self.x as f32;
        ya + (yb - ya) * w
    }
}

// ---------------------------------------------------------------- the ceiling

/// Transparent under −6 dBFS, 0.5 + 0.5 tanh(2 (|x| − 0.5)) above (ADAA1), never past 0 dBFS.
#[derive(Clone, Debug, Default)]
struct Ceiling {
    x1: f64,
}

impl Ceiling {
    #[inline]
    fn f(x: f64) -> f64 {
        let a = x.abs();
        if a <= 0.5 {
            x
        } else {
            (0.5 + 0.5 * (2.0 * (a - 0.5)).tanh()).copysign(x)
        }
    }
    #[inline]
    fn big_f(x: f64) -> f64 {
        let a = x.abs();
        if a <= 0.5 {
            0.5 * x * x
        } else {
            let u = 2.0 * (a - 0.5);
            // ln cosh u, stable
            let lc = u + (-2.0 * u).exp().ln_1p() - std::f64::consts::LN_2;
            0.125 + 0.5 * (a - 0.5) + 0.25 * lc
        }
    }
    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let x1 = self.x1;
        self.x1 = x;
        if x.abs() <= 0.5 && x1.abs() <= 0.5 {
            return x;
        }
        let d = x - x1;
        let y = if d.abs() < 1e-9 { Self::f(0.5 * (x + x1)) } else { (Self::big_f(x) - Self::big_f(x1)) / d };
        // the difference quotient's rounding: never past 0 dBFS
        y.clamp(-1.0, 1.0)
    }
}

// ---------------------------------------------------------------- the voice

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VState {
    Off,
    On,
    Release,
}

/// One Mono voice slot.
#[derive(Clone, Debug)]
struct Voice {
    st: VState,
    tr: Tracker,
    /// the string's note now (Hz)
    hz: f64,
    /// the quantized note (MIDI), held with hysteresis
    q: f64,
    /// portamento: a log2 offset decaying to 0
    glide: f64,
    p1: Phasor,
    p2: Phasor,
    ps: Phasor,
    fade: f64,
    k: f64,
    matched: bool,
    peak: f64,
    rise: Rise,
    rel: f64,
    /// the frequency (Hz, transposed) held in release
    held: f64,
    vcf: [VcfX; 2],
    t_on: u64,
    noise: White,
    /// the last level (tests, the end rule)
    level: f64,
    /// the string's frequency smoothed (log2 Hz, two one-poles): the loop's own FM noise filtered
    lf1: f64,
    lf2: f64,
    /// the bridge: the level matched to the core bands the voice takes, until `k` is frozen
    lvl_match: f64,
    lvl_target: f64,
}

impl Voice {
    fn new(seed: u64) -> Self {
        Self {
            st: VState::Off,
            tr: Tracker::new(),
            hz: 0.0,
            q: 0.0,
            glide: 0.0,
            p1: Phasor::new(0.0),
            p2: Phasor::new(0.0),
            ps: Phasor::new(0.0),
            fade: 0.0,
            k: 1.0,
            matched: false,
            peak: 0.0,
            rise: Rise::default(),
            rel: 1.0,
            held: 0.0,
            vcf: [VcfX::new(), VcfX::new()],
            t_on: 0,
            noise: White::new(seed),
            level: 0.0,
            lf1: 0.0,
            lf2: 0.0,
            lvl_match: 0.0,
            lvl_target: 0.0,
        }
    }

    fn reset(&mut self, cfg: (VcfType, bool)) {
        self.st = VState::Off;
        self.tr.reset();
        self.hz = 0.0;
        self.q = 0.0;
        self.glide = 0.0;
        self.p1 = Phasor::new(0.0);
        self.p2 = Phasor::new(0.0);
        self.ps = Phasor::new(0.0);
        self.fade = 0.0;
        self.k = 1.0;
        self.matched = false;
        self.peak = 0.0;
        self.rise = Rise::default();
        self.rel = 1.0;
        self.held = 0.0;
        for v in self.vcf.iter_mut() {
            v.reset(cfg);
        }
        self.noise.reset();
        self.level = 0.0;
        self.lf1 = 0.0;
        self.lf2 = 0.0;
        self.lvl_match = 0.0;
        self.lvl_target = 0.0;
    }
}

/// The voice's waveform at phase t (turns), on the core's phase convention (θ = 2π t), unit-sine RMS.
#[inline]
fn wave_at(wave: usize, p: &Phasor, pw: f64) -> f64 {
    match wave {
        // the falling saw (Σ sin nθ / n): RMS 1 / √3
        0 => -saw(p) * (1.5f64.sqrt()),
        // the pulse centred on θ = 0, mean removed: RMS 2 √(pw (1 − pw))
        1 => {
            let mut q = *p;
            q.t += 0.5 * pw;
            if q.t >= 1.0 {
                q.t -= 1.0;
            }
            pulse_width(&q, pw) * FRAC_1_SQRT_2 / (2.0 * (pw * (1.0 - pw)).sqrt()).max(1e-3)
        }
        // the triangle peaking at θ = π / 2: RMS 1 / √3
        2 => {
            let mut q = *p;
            q.t += 0.25;
            if q.t >= 1.0 {
                q.t -= 1.0;
            }
            triangle(&q) * (1.5f64.sqrt())
        }
        _ => (TAU * p.t).cos(),
    }
}

// ---------------------------------------------------------------- the processor

pub struct Polysynth {
    v: [f32; NP],
    sr: f32,
    nch: usize,
    clock: u64,
    ctrl_n: u64,
    // smoothed parameters
    drive_lp: f64,
    drive_a: f64,
    cutoff: ExpSmoother,
    reso: ExpSmoother,
    filt_env: ExpSmoother,
    synth: ExpSmoother,
    dry: ExpSmoother,
    pw: ExpSmoother,
    sub: ExpSmoother,
    detune: ExpSmoother,
    attack: ExpSmoother,
    compress: ExpSmoother,
    drive: ExpSmoother,
    fenv_dec: ExpSmoother,
    glide: ExpSmoother,
    noise: OnePole,
    key_track: OnePole,
    residue: OnePole,
    spread: OnePole,
    sustain: OnePole,
    // structural fades
    eng_x: f64,
    amp_x: f64,
    wave_a: usize,
    wave_b: usize,
    wave_x: f64,
    oct_a: i32,
    oct_b: i32,
    oct_x: f64,
    xstep: f64,
    estep: f64,
    // the bank path
    hpf: Svf,
    down: OversamplerN,
    up_a: OversamplerN,
    up_b: OversamplerN,
    factor: usize,
    dbuf: [f64; MAX_FACTOR],
    ua: [f64; MAX_FACTOR],
    ub: [f64; MAX_FACTOR],
    dn: usize,
    core: Core,
    cs: CoreSettings,
    // detection
    lvl: SlidingMax,
    lvl_now: f64,
    hist: [f64; MUTE_WIN],
    hist_at: usize,
    pick: Pick,
    jump: PitchJump,
    jump_at: u64,
    yin: PitchDetector,
    mp: Analyzer,
    lowest_set: f32,
    pick_flag: bool,
    onset_at: Option<u64>,
    await_from: Option<u64>,
    agree: f64,
    oct_since: Option<u64>,
    spent: bool,
    note_on: bool,
    strength_peak: f64,
    strength_ref: f64,
    strength_until: u64,
    fenv_depth: f64,
    // envelopes
    adsr: Adsr,
    fenv: FiltEnv,
    core_env: f64,
    core_rise: Rise,
    // voices and filters
    voices: [Voice; 2],
    vcf_core: [VcfX; 2],
    ceiling: [Ceiling; 2],
    // bypass
    byp: bool,
    byp_x: f64,
    byp_synth: f64,
    byp_step: f64,
    // meters
    m_state: f32,
    m_note: f32,
    out_env: f64,
}

impl Default for Polysynth {
    fn default() -> Self {
        Self::new()
    }
}

impl Polysynth {
    pub fn new() -> Self {
        let v: [f32; NP] = core::array::from_fn(|i| PARAMS[i].default);
        let es = |i: usize| ExpSmoother::new(PARAMS[i].min, PARAMS[i].max, v[i]);
        let op = |i: usize| OnePole::new(20.0, v[i], 1e-4);
        Self {
            v,
            sr: 48000.0,
            nch: 1,
            clock: 0,
            ctrl_n: 48,
            drive_lp: v[p::DRIVE] as f64 * 0.24,
            drive_a: 0.0,
            cutoff: es(p::CUTOFF),
            reso: es(p::RESO),
            filt_env: es(p::FILT_ENV),
            synth: es(p::SYNTH),
            dry: es(p::DRY),
            pw: es(p::PW),
            sub: es(p::SUB),
            detune: es(p::DETUNE),
            attack: es(p::ATTACK),
            compress: es(p::COMPRESS),
            drive: es(p::DRIVE),
            fenv_dec: es(p::FENV_DEC),
            glide: es(p::GLIDE),
            noise: op(p::NOISE),
            key_track: op(p::KEY_TRACK),
            residue: op(p::RESIDUE),
            spread: op(p::SPREAD),
            sustain: op(p::SUSTAIN),
            eng_x: 0.0,
            amp_x: 0.0,
            wave_a: 0,
            wave_b: 0,
            wave_x: 0.0,
            oct_a: 0,
            oct_b: 0,
            oct_x: 0.0,
            xstep: 0.0,
            estep: 0.0,
            hpf: Svf::new(),
            down: OversamplerN::new(1),
            up_a: OversamplerN::new(1),
            up_b: OversamplerN::new(1),
            factor: 2,
            dbuf: [0.0; MAX_FACTOR],
            ua: [0.0; MAX_FACTOR],
            ub: [0.0; MAX_FACTOR],
            dn: 0,
            core: Core::new(),
            cs: CoreSettings::default(),
            lvl: SlidingMax::new(),
            lvl_now: 0.0,
            hist: [-240.0; MUTE_WIN],
            hist_at: 0,
            pick: Pick::default(),
            jump: PitchJump::default(),
            jump_at: 0,
            yin: PitchDetector::default(),
            mp: Analyzer::new(),
            lowest_set: -1.0,
            pick_flag: false,
            onset_at: None,
            await_from: None,
            agree: 0.0,
            oct_since: None,
            spent: false,
            note_on: false,
            strength_peak: 0.0,
            strength_ref: 0.0,
            strength_until: 0,
            fenv_depth: 1.0,
            adsr: Adsr::new(),
            fenv: FiltEnv::new(),
            core_env: 0.0,
            core_rise: Rise::default(),
            voices: [Voice::new(0x5EED_0001), Voice::new(0x5EED_0002)],
            vcf_core: [VcfX::new(), VcfX::new()],
            ceiling: [Ceiling::default(), Ceiling::default()],
            byp: false,
            byp_x: 0.0,
            byp_synth: 1.0,
            byp_step: 0.0,
            m_state: 0.0,
            m_note: 0.0,
            out_env: 0.0,
        }
    }

    fn b(&self, i: usize) -> bool {
        self.v[i] >= 0.5
    }

    fn mono_engine(&self) -> bool {
        idx(self.v[p::ENGINE], 2) == 1
    }

    fn filter_cfg(&self) -> (VcfType, bool) {
        (VcfType::from_index(idx(self.v[p::TYPE], 3)), idx(self.v[p::POLES], 2) == 1)
    }

    /// `state` meter: 0 silence · 1 core only · 2 voice sounding.
    pub fn state(&self) -> f32 {
        self.m_state
    }

    /// `note` meter: the voice's tracked f0 (Hz), 0 = none or Spectral.
    pub fn note_hz(&self) -> f32 {
        self.m_note
    }

    /// The core (tests).
    pub fn spectral_core(&self) -> &Core {
        &self.core
    }

    /// The sounding voice's (transposed frequency Hz, fade, level k, state 0 off · 1 on · 2 release)
    /// (tests).
    pub fn voice_info(&self) -> Vec<(f64, f64, f64, u8)> {
        let sr = self.sr as f64;
        self.voices
            .iter()
            .filter(|v| v.st != VState::Off)
            .map(|v| {
                let f = if v.st == VState::Release { v.held } else { v.p1.dt * sr };
                (f, v.fade, v.k, if v.st == VState::On { 1 } else { 2 })
            })
            .collect()
    }

    fn set_lowest(&mut self, force: bool) {
        let lo = self.v[p::LOWEST].clamp(30.0, 150.0);
        if !force && (lo - self.lowest_set).abs() < 0.01 {
            return;
        }
        self.lowest_set = lo;
        self.yin.set_range(lo, YIN_MAX);
        self.mp.set_f_min(lo);
        self.lvl.set_window(((self.sr / 25.0).ceil() as usize).max(1));
    }

    fn structural(&mut self) {
        let w = idx(self.v[p::WAVE], 4);
        if w != self.wave_b && self.wave_x == 0.0 {
            self.wave_a = self.wave_b;
            self.wave_b = w;
            self.wave_x = 1e-9;
        }
        let o = self.v[p::OCTAVE].round().clamp(-2.0, 2.0) as i32;
        if o != self.oct_b && self.oct_x == 0.0 {
            self.oct_a = self.oct_b;
            self.oct_b = o;
            self.oct_x = 1e-9;
        }
        let cfg = self.filter_cfg();
        for f in self.vcf_core.iter_mut() {
            f.want(cfg);
        }
        for v in self.voices.iter_mut() {
            for f in v.vcf.iter_mut() {
                f.want(cfg);
            }
        }
    }

    // ---------------------------------------------------------------- Mono: voices

    fn start_voice(&mut self, hz: f64) {
        let sr = self.sr as f64;
        // the old voice (if any) releases; the free slot takes the note
        let rel_s = self.v[p::RELEASE] as f64 * 1e-3;
        for v in self.voices.iter_mut() {
            if v.st == VState::On {
                Self::release(v, sr, rel_s);
            }
        }
        let slot = match self.voices.iter().position(|v| v.st == VState::Off) {
            Some(i) => i,
            None => {
                // both busy: the quieter releasing one is taken
                if self.voices[0].rel < self.voices[1].rel {
                    0
                } else {
                    1
                }
            }
        };
        let oct = (self.oct_b as f64).exp2();
        let theta = self.core.phase_at(hz * oct).unwrap_or(0.0);
        let cfg = self.filter_cfg();
        let now = self.clock;
        let v = &mut self.voices[slot];
        v.reset(cfg);
        v.st = VState::On;
        // Snap: the voice takes only the tracker's frequency (its oscillator keeps its own phase), so the
        // NCO may jump onto the string; Keep would pull its phase in through the frequency, 5 ¢ off for
        // up to half a second
        v.tr.engage(Engage::Snap);
        v.tr.set_f0(hz as f32, true);
        v.hz = hz;
        v.lf1 = hz.log2();
        v.lf2 = v.lf1;
        v.q = (12.0 * (hz * oct / 440.0).log2() + 69.0).round();
        let t0 = (theta / TAU).rem_euclid(1.0);
        v.p1 = Phasor::new(t0);
        v.p2 = Phasor::new(t0);
        v.ps = Phasor::new(0.5 * t0);
        v.t_on = now;
        v.rise.a = 0.0;
        self.await_from = None;
        self.agree = 0.0;
        self.oct_since = None;
        if !self.note_on && self.onset_at.is_none_or(|t| now.saturating_sub(t) as f64 > BODY_S * sr) {
            // a note without a pick (a swell, a note after silence): the envelopes start here
            self.adsr.trigger();
            self.fenv.trigger(self.v[p::FENV_ATT] as f64 * 1e-3);
        }
        self.note_on = true;
    }

    fn release(v: &mut Voice, sr: f64, _rel_s: f64) {
        if v.st != VState::On {
            return;
        }
        v.held = v.p1.dt * sr;
        v.st = VState::Release;
    }

    fn release_voices(&mut self) {
        let sr = self.sr as f64;
        let rel_s = self.v[p::RELEASE] as f64 * 1e-3;
        for v in self.voices.iter_mut() {
            Self::release(v, sr, rel_s);
        }
    }

    fn active_voice(&self) -> Option<usize> {
        self.voices.iter().position(|v| v.st == VState::On)
    }

    /// A YIN reading (Mono).
    fn reading(&mut self, hz: Option<f32>, clarity: f32) {
        let now = self.clock;
        let sr = self.sr as f64;
        let thresh = db_to_lin(self.v[p::THRESH] as f64);
        let loud = self.lvl_now >= thresh;
        let clear = hz.is_some() && clarity >= CLEAR && loud;
        let f = hz.unwrap_or(0.0) as f64;
        // legato (detector C)
        let elapsed = now - self.jump_at;
        self.jump_at = now;
        let jump = self.jump.feed(hz, clarity, elapsed, PitchJump::clarity_for(0.5)).is_some();
        let since_pick = self.onset_at.map_or(f64::INFINITY, |t| (now - t) as f64 / sr);
        self.mp.set_active(!clear);
        if let Some(t0) = self.await_from {
            if clear {
                if self.agree > 0.0 && cents(f, self.agree).abs() <= AGREE_CENTS {
                    self.start_voice(f);
                } else {
                    self.agree = f;
                }
            } else if let Some(n) = self.mp.notes().first() {
                // a chord: its lowest confirmed note
                let h = n.hz as f64;
                self.start_voice(h);
            } else if (now - t0) as f64 / sr > CHORD_WAIT_S {
                if hz.is_some() && loud {
                    self.start_voice(f);
                } else {
                    self.await_from = None;
                }
            }
            return;
        }
        let Some(i) = self.active_voice() else {
            // no voice and no pick: a note that arrives by itself (after a release, a swell)
            if clear && !self.spent {
                self.await_from = Some(now);
                self.agree = f;
            }
            return;
        };
        let vh = self.voices[i].hz;
        if jump && since_pick > BODY_S {
            if let Some(r) = self.jump.reference_hz() {
                let target = if clear && cents(f, r as f64).abs() <= AGREE_CENTS { f } else { r as f64 };
                self.legato(i, target);
                return;
            }
        }
        if clear {
            let c = cents(f, vh);
            if c.abs() <= FEED_CENTS {
                self.voices[i].tr.set_f0(f as f32, true);
                self.oct_since = None;
            } else if (c.abs() - 1200.0).abs() <= 60.0 {
                // the octave guard: the voice follows a ×2 / ÷2 reading only once it holds 40 ms
                let t = *self.oct_since.get_or_insert(now);
                if (now - t) as f64 / sr >= OCTAVE_CONFIRM_S {
                    self.legato(i, f);
                    self.oct_since = None;
                }
            } else {
                self.oct_since = None;
            }
        }
    }

    /// A legato change: the voice moves (with `glide`), no envelope restarts.
    fn legato(&mut self, i: usize, f: f64) {
        let oct = (self.oct_b as f64).exp2();
        let v = &mut self.voices[i];
        let old = v.p1.dt * self.sr as f64;
        v.tr.set_f0(f as f32, true);
        v.hz = f;
        if old > 0.0 {
            v.glide += (old / (f * oct)).log2();
            if self.v[p::GLIDE] <= 0.0 {
                v.glide = 0.0;
            }
        }
    }

    // ---------------------------------------------------------------- control (1 ms)

    fn control(&mut self) {
        let now = self.clock;
        let sr = self.sr as f64;
        let v = self.v;
        let thresh = v[p::THRESH] as f64;
        let l_db = to_db(self.lvl_now);
        self.hist[self.hist_at] = l_db;
        self.hist_at = (self.hist_at + 1) % MUTE_WIN;
        let max60 = self.hist.iter().copied().fold(f64::MIN, f64::max);
        let mono = self.mono_engine();
        let since_pick = self.onset_at.map_or(f64::INFINITY, |t| (now - t) as f64 / sr);

        // ---- bypass
        let bypass = self.b(p::BYPASS);
        if bypass != self.byp {
            self.byp = bypass;
            let len = if bypass && self.b(p::TRAILS) { v[p::RELEASE] as f64 * 1e-3 } else { BYPASS_MS * 1e-3 };
            self.byp_step = 1.0 / (len * sr).max(1.0);
            if bypass {
                self.release_voices();
                self.adsr.gate_off();
            }
        }

        // ---- a pick: envelopes restart, the voice releases, the core goes to the fast bank
        if std::mem::take(&mut self.pick_flag) && !bypass {
            self.onset_at = Some(now);
            self.adsr.trigger();
            self.fenv.trigger(self.v[p::FENV_ATT] as f64 * 1e-3);
            self.core.pick();
            self.spent = false;
            self.note_on = true;
            self.strength_peak = self.lvl_now;
            self.strength_until = now + (0.010 * sr) as u64;
            if mono {
                self.release_voices();
                self.mp.onset();
                self.await_from = Some(now);
                self.agree = 0.0;
            }
        }
        // the pick's strength: its level over 10 ms, against the picks of the last 2 s
        if now <= self.strength_until {
            self.strength_peak = self.strength_peak.max(self.lvl_now);
            if self.strength_peak > self.strength_ref {
                self.strength_ref = self.strength_peak;
            }
            let s = if self.strength_ref > 0.0 { (self.strength_peak / self.strength_ref).clamp(0.0, 1.0) } else { 1.0 };
            let vel = v[p::FENV_VEL] as f64 * 0.01;
            self.fenv_depth = 1.0 - vel + vel * s;
        }
        self.strength_ref *= (-CTRL_MS * 1e-3 / 2.0).exp();

        // ---- end of note (both engines): mute or under `thresh`
        let ending = since_pick > BODY_S && (max60 - l_db > MUTE_DB || l_db < thresh);
        if ending && self.note_on {
            self.note_on = false;
            self.adsr.gate_off();
            if mono {
                self.release_voices();
                self.spent = true;
                self.await_from = None;
            }
        }
        if !self.note_on && l_db >= thresh + 6.0 && since_pick > BODY_S && !mono && !self.adsr.held() {
            // a note that rose without a pick (a swell): Gate starts on it
            self.note_on = true;
            self.adsr.trigger();
            self.fenv.trigger(self.v[p::FENV_ATT] as f64 * 1e-3);
        }
        if mono {
            // the tracker's level under `thresh` − 6 dB ends the voice
            if let Some(i) = self.active_voice() {
                let on_s = (now - self.voices[i].t_on) as f64 / sr;
                if on_s > BODY_S && to_db(self.voices[i].level) < thresh - TRACK_UNDER_DB {
                    let rel_s = v[p::RELEASE] as f64 * 1e-3;
                    Self::release(&mut self.voices[i], sr, rel_s);
                    self.spent = true;
                }
            }
            if self.active_voice().is_some() {
                self.note_on = true;
            }
        } else if self.active_voice().is_some() {
            self.release_voices();
        }

        // ---- the bridge: the voice starts at the level of the core bands it takes (their RMS over
        // 10 ms, before the mask); once the tracker reads the string (the fade and two periods), the
        // ratio k is frozen and Follow tracks the string's changes from there
        let am = self.amp_x;
        for i in 0..2 {
            let vc = &self.voices[i];
            if vc.st != VState::On || vc.matched {
                continue;
            }
            let rms = self.core.owned_rms(vc.hz);
            let l = (1.0 - am) * vc.rise.a + am;
            let target = if rms > 1e-9 { rms / FRAC_1_SQRT_2 } else { l };
            let wait = MATCH_MIN_S.max(MATCH_PERIODS / vc.hz.max(20.0));
            let vc = &mut self.voices[i];
            vc.lvl_target = target;
            if vc.lvl_match == 0.0 {
                vc.lvl_match = target;
            }
            if (now - vc.t_on) as f64 / sr >= wait && vc.fade >= 1.0 {
                vc.k = if l > 1e-9 { (vc.lvl_match / l).clamp(0.05, 20.0) } else { 1.0 };
                vc.matched = true;
            }
        }

        // ---- the core's settings for its coming ticks
        self.structural();
        let pw = (self.pw.value() as f64 * 0.01).clamp(0.05, 0.95);
        let ca = coefs(self.wave_a, pw);
        let cb = coefs(self.wave_b, pw);
        let wx = self.wave_x;
        let c: [(f64, f64); NH] = core::array::from_fn(|n| (ca[n].0 * (1.0 - wx) + cb[n].0 * wx, ca[n].1 * (1.0 - wx) + cb[n].1 * wx));
        let owner = self.active_voice().map_or(0.0, |i| self.voices[i].hz);
        self.core.set_owner(owner);
        self.cs = CoreSettings {
            bank: match idx(v[p::BANK], 3) {
                1 => BankMode::Fast,
                2 => BankMode::Clean,
                _ => BankMode::Dual,
            },
            c,
            oct_a: self.oct_a,
            oct_b: self.oct_b,
            oct_x: if self.oct_x == 0.0 { 1.0 } else { self.oct_x },
            sub: self.sub.value() as f64 * 0.01,
            noise: self.noise.value() as f64 * 0.01,
            compress: self.compress.value() as f64 * 0.01,
            detune: self.detune.value() as f64,
            thresh_db: thresh,
            lowest: self.lowest_set as f64,
            mono: self.eng_x,
            residue: self.residue.value() as f64 * 0.01,
        };
        self.core.set(self.cs);

        // ---- meters
        self.m_note = if mono { self.active_voice().map_or(0.0, |i| self.voices[i].hz as f32) } else { 0.0 };
        self.m_state = if self.voices.iter().any(|v| v.st != VState::Off && v.fade > 0.0) {
            2.0
        } else if self.out_env > db_to_lin(thresh) {
            1.0
        } else {
            0.0
        };
    }

    // ---------------------------------------------------------------- one sample

    #[inline]
    fn tick(&mut self, x: f32) -> [f32; 2] {
        let sr = self.sr as f64;
        // ---- smoothed parameters
        let cutoff = self.cutoff.next() as f64;
        let reso = self.reso.next() * 0.01;
        let filt_env = self.filt_env.next() as f64;
        let synth_db = self.synth.next() as f64;
        self.dry.next();
        let pw = (self.pw.next() as f64 * 0.01).clamp(0.05, 0.95);
        let sub = self.sub.next() as f64 * 0.01;
        let detune = self.detune.next() as f64;
        let att_s = self.attack.next() as f64 * 1e-3;
        let comp = (self.compress.next() as f64 * 0.01).clamp(0.0, 1.0);
        // the drive's dB through a further 30 ms: a 24 dB move of the tanh in the EXP ramp's 5 ms swept
        // the saturated harmonics (−33 dB of HF at the bench's step)
        let d_target = (self.drive.next() * 0.24).max(0.0) as f64;
        self.drive_lp += (d_target - self.drive_lp) * self.drive_a;
        if (self.drive_lp - d_target).abs() < 1e-4 {
            self.drive_lp = d_target;
        }
        let drive_db = self.drive_lp as f32;
        let fdec_s = self.fenv_dec.next() as f64 * 1e-3;
        let glide_s = self.glide.next() as f64 * 1e-3;
        let noise = self.noise.next() as f64 * 0.01;
        let key = self.key_track.next() as f64 * 0.01;
        let _residue = self.residue.next();
        let spread = (self.spread.next() as f64 * 0.01).clamp(0.0, 1.0);
        let sus = (self.sustain.next() as f64 * 0.01).clamp(0.0, 1.0);
        let mono = self.mono_engine();
        // ---- structural fades
        self.eng_x = if mono { (self.eng_x + self.estep).min(1.0) } else { (self.eng_x - self.estep).max(0.0) };
        let gate_mode = idx(self.v[p::AMP_MODE], 2) == 1;
        self.amp_x = if gate_mode { (self.amp_x + self.xstep).min(1.0) } else { (self.amp_x - self.xstep).max(0.0) };
        if self.wave_x > 0.0 {
            self.wave_x += self.xstep;
            if self.wave_x >= 1.0 {
                self.wave_x = 0.0;
                self.wave_a = self.wave_b;
            }
        }
        if self.oct_x > 0.0 {
            self.oct_x += self.xstep;
            if self.oct_x >= 1.0 {
                self.oct_x = 0.0;
                self.oct_a = self.oct_b;
            }
        }
        // ---- detection
        let l = self.lvl.next(x.abs()) as f64;
        self.lvl_now = l;
        let thresh_db = self.v[p::THRESH] as f64;
        if self.pick.next(x, l >= db_to_lin(thresh_db - PICK_UNDER_DB)) {
            self.pick_flag = true;
        }
        if mono || self.eng_x > 0.0 {
            self.mp.push(x);
            if self.yin.push(x) {
                let (hz, cl) = (self.yin.hz(), self.yin.clarity());
                if mono {
                    self.reading(hz, cl);
                }
            }
        }
        if self.clock.is_multiple_of(self.ctrl_n) {
            self.control();
        }
        self.clock += 1;

        // ---- the bank path: one bank sample per `factor` host samples
        let ca = self.ua[self.dn];
        let cb = self.ub[self.dn];
        let h = self.hpf.process(x) as f64;
        self.dbuf[self.dn] = h;
        self.dn += 1;
        if self.dn == self.factor {
            self.dn = 0;
            let mut buf = self.dbuf;
            let d = if self.factor > 1 { self.down.down(&mut buf) } else { buf[0] };
            let (a, b) = self.core.process(d);
            if self.factor > 1 {
                self.up_a.up(a, &mut self.ua);
                self.up_b.up(b, &mut self.ub);
            } else {
                self.ua[0] = a;
                self.ub[0] = b;
            }
        }

        // ---- envelopes
        let am = self.amp_x;
        let adsr = self.adsr.next(att_s, self.v[p::DECAY] as f64 * 1e-3, sus, self.v[p::RELEASE] as f64 * 1e-3, sr);
        let fe = self.fenv.next(fdec_s, sr) * self.fenv_depth;
        // the core's Follow: `attack` as its minimum rise
        let e = ca.abs().max(cb.abs());
        let k_env = if e > self.core_env { 1.0 - (-1.0 / (0.0005 * sr)).exp() } else { 1.0 - (-1.0 / (0.030 * sr)).exp() };
        self.core_env += (e - self.core_env) * k_env;
        let risen = self.core_rise.next(self.core_env, att_s, sr);
        let g_follow = if self.core_env > 1e-12 { risen / self.core_env } else { 1.0 };
        let g_core = (1.0 - am) * g_follow + am * adsr;

        // ---- the filter
        let q = q_of(reso);
        let fmax = (0.45 * sr).min(20000.0);
        let fc_core = (cutoff * (fe * filt_env / 12.0).exp2()).clamp(20.0, fmax) as f32;
        let step = self.xstep;
        let srf = self.sr;
        // the unison pair: L = √2 (a cl + b cr), R = √2 (a cr + b cl)
        let th = 0.25 * PI * (1.0 - spread);
        let (cl, cr) = (th.cos() * SQRT_2, th.sin() * SQRT_2);
        let nch = self.nch;
        let mut out = [0.0f32; 2];
        for c in 0..nch {
            let (ga, gb) = if c == 0 { (cl, cr) } else { (cr, cl) };
            let input = (ca * ga + cb * gb) as f32;
            let y = self.vcf_core[c].process(input, fc_core, q, drive_db, srf, step);
            out[c] = y * g_core as f32;
        }

        // ---- the voices
        if self.eng_x > 0.0 || self.voices.iter().any(|v| v.st != VState::Off) {
            let oct = (self.oct_b as f64).exp2();
            let chromatic = idx(self.v[p::QUANTIZE], 2) == 1;
            let rel_s = (self.v[p::RELEASE] as f64 * 1e-3).max(1e-3);
            let rel_k = db_to_lin(-ENV_RANGE_DB / (rel_s * sr));
            let fade_step = 1.0 / (VOICE_FADE_MS * 1e-3 * sr);
            let glide_k = if glide_s > 0.0 { (-3.0 / (glide_s * sr)).exp() } else { 0.0 };
            let det_ratio = (detune / 1200.0).exp2();
            let thd = 0.25 * PI * (detune / 2.0).clamp(0.0, 1.0);
            let (g1, g2) = (thd.cos(), if detune > 0.0 { thd.sin() } else { 0.0 });
            let eng = self.eng_x;
            let pk_att = 1.0 - (-1.0 / (0.001 * sr)).exp();
            let f_smooth = 1.0 - (-1.0 / (FREQ_SMOOTH_MS * 1e-3 * sr)).exp();
            let m_smooth = 1.0 - (-1.0 / (0.002 * sr)).exp();
            let pk_rel = (-1.0 / (3.0 * sr)).exp();
            let (wa, wb, wx) = (self.wave_a, self.wave_b, self.wave_x);
            for vi in 0..2 {
                let v = &mut self.voices[vi];
                if v.st == VState::Off {
                    continue;
                }
                // pitch
                let f = if v.st == VState::On {
                    let lock = v.tr.process(x);
                    v.level = lock.level as f64;
                    let fs = if lock.locked { lock.inc * sr } else { v.tr.ff_hz() };
                    if fs > 0.0 {
                        v.lf1 += (fs.log2() - v.lf1) * f_smooth;
                        v.lf2 += (v.lf1 - v.lf2) * f_smooth;
                        v.hz = v.lf2.exp2();
                    }
                    let mut ft = v.hz * oct;
                    if chromatic {
                        let m = 12.0 * (ft / 440.0).log2() + 69.0;
                        if (m - v.q).abs() > 0.5 + Q_HYST {
                            let nq = m.round();
                            if glide_s > 0.0 {
                                v.glide += (nq - v.q) / -12.0;
                            }
                            v.q = nq;
                        }
                        ft = 440.0 * ((v.q - 69.0) / 12.0).exp2();
                    }
                    v.glide *= glide_k;
                    ft * v.glide.exp2()
                } else {
                    v.held
                };
                v.p1.set_freq(f, sr);
                v.p2.set_freq(f * det_ratio, sr);
                v.ps.set_freq(0.5 * f, sr);
                // level (Follow: the tracker's d₁ through `compress`, `attack` as the minimum rise)
                if v.st == VState::On {
                    let d = v.level;
                    if d > v.peak {
                        v.peak += (d - v.peak) * pk_att;
                    } else {
                        v.peak *= pk_rel;
                    }
                    let lf = if d > 1e-12 && v.peak > d { d * ((comp * (v.peak / d).ln()).exp()).min(COMP_CAP) } else { d };
                    v.rise.next(lf, att_s, sr);
                    v.fade = (v.fade + fade_step).min(1.0);
                } else {
                    v.rel *= rel_k;
                    if v.rel < 1e-4 {
                        v.st = VState::Off;
                        continue;
                    }
                }
                let lvl = if v.matched {
                    v.k * v.fade * ((1.0 - am) * v.rise.a + am)
                } else {
                    // the bridge: the core bands' level until `k` is frozen
                    v.lvl_match += (v.lvl_target - v.lvl_match) * m_smooth;
                    v.lvl_match * v.fade
                };
                // oscillators
                let mut o1 = wave_at(wb, &v.p1, pw);
                let mut o2 = if g2 > 0.0 { wave_at(wb, &v.p2, pw) } else { 0.0 };
                if wx > 0.0 {
                    o1 = o1 * wx + wave_at(wa, &v.p1, pw) * (1.0 - wx);
                    if g2 > 0.0 {
                        o2 = o2 * wx + wave_at(wa, &v.p2, pw) * (1.0 - wx);
                    }
                }
                let mut va = g1 * o1;
                if sub > 0.0 {
                    va += sub * FRAC_1_SQRT_2 * pulse(&v.ps);
                }
                if noise > 0.0 {
                    va += noise * (FRAC_1_SQRT_2 * 3f64.sqrt()) * v.noise.next_f64();
                }
                let vb = g2 * o2;
                v.p1.advance();
                v.p2.advance();
                v.ps.advance();
                let note = 12.0 * (f.max(1.0) / 440.0).log2() + 69.0;
                let fc_v = (cutoff * ((fe * filt_env + key * (note - 60.0)) / 12.0).exp2()).clamp(20.0, fmax) as f32;
                let post = ((1.0 - am) + am * adsr) * v.rel * eng;
                for c in 0..nch {
                    let (ga, gb) = if c == 0 { (cl, cr) } else { (cr, cl) };
                    let input = ((va * ga + vb * gb) * lvl) as f32;
                    let y = v.vcf[c].process(input, fc_v, q, drive_db, srf, step);
                    out[c] += y * post as f32;
                }
            }
        }

        // ---- the bus: synth gain, bypass, ceiling, + dry
        if self.byp {
            self.byp_synth = (self.byp_synth - self.byp_step).max(0.0);
            self.byp_x = (self.byp_x + 1.0 / (BYPASS_MS * 1e-3 * sr)).min(1.0);
        } else {
            self.byp_synth = (self.byp_synth + self.byp_step.max(1.0 / (BYPASS_MS * 1e-3 * sr))).min(1.0);
            self.byp_x = (self.byp_x - 1.0 / (BYPASS_MS * 1e-3 * sr)).max(0.0);
        }
        let g_synth = if synth_db <= -59.999 { 0.0 } else { db_to_lin(synth_db) } * self.byp_synth;
        let mut res = [0.0f32; 2];
        for c in 0..nch {
            let s = self.ceiling[c].process(out[c] as f64 * g_synth);
            res[c] = s as f32;
        }
        let e = (res[0] as f64).abs();
        self.out_env = if e > self.out_env { e } else { self.out_env * 0.9995 };
        res
    }
}

impl Effect for Polysynth {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate;
        self.nch = channels.clamp(1, 2);
        let srd = sample_rate as f64;
        self.ctrl_n = (CTRL_MS * 1e-3 * srd).round().max(1.0) as u64;
        for e in [&mut self.cutoff, &mut self.reso, &mut self.filt_env, &mut self.synth, &mut self.dry, &mut self.pw, &mut self.sub, &mut self.detune, &mut self.attack, &mut self.compress, &mut self.drive, &mut self.fenv_dec, &mut self.glide] {
            e.prepare(sample_rate);
        }
        for o in [&mut self.noise, &mut self.key_track, &mut self.residue, &mut self.spread, &mut self.sustain] {
            o.prepare(sample_rate);
        }
        // the bank rate: 22.05 – 24 kHz at every host rate (÷ 2, ÷ 4, ÷ 8)
        let stages = (srd / 24000.0).log2().round().clamp(0.0, 3.0) as usize;
        self.factor = 1 << stages;
        self.down = OversamplerN::new(stages);
        self.up_a = OversamplerN::new(stages);
        self.up_b = OversamplerN::new(stages);
        self.core.prepare(srd / self.factor as f64);
        self.hpf.set(SvfMode::HighPass, 25.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, sample_rate);
        self.xstep = 1.0 / (XFADE_MS * 1e-3 * srd);
        self.drive_a = 1.0 - (-1.0 / (0.030 * srd)).exp();
        self.estep = 1.0 / (ENGINE_MS * 1e-3 * srd);
        self.lvl.prepare(((srd / 25.0).ceil() as usize + 2).max(4));
        self.pick.prepare(sample_rate);
        self.jump.prepare(sample_rate);
        self.yin.prepare_range(sample_rate, 30.0, YIN_MAX, true);
        self.mp.prepare(sample_rate, 30.0, 6);
        for v in self.voices.iter_mut() {
            v.tr.prepare(sample_rate, 0);
        }
        self.reset();
    }

    fn reset(&mut self) {
        let v = self.v;
        for (i, e) in [
            (p::CUTOFF, &mut self.cutoff),
            (p::RESO, &mut self.reso),
            (p::FILT_ENV, &mut self.filt_env),
            (p::SYNTH, &mut self.synth),
            (p::DRY, &mut self.dry),
            (p::PW, &mut self.pw),
            (p::SUB, &mut self.sub),
            (p::DETUNE, &mut self.detune),
            (p::ATTACK, &mut self.attack),
            (p::COMPRESS, &mut self.compress),
            (p::DRIVE, &mut self.drive),
            (p::FENV_DEC, &mut self.fenv_dec),
            (p::GLIDE, &mut self.glide),
        ] {
            e.settle(v[i]);
            e.reset();
        }
        for (i, o) in [(p::NOISE, &mut self.noise), (p::KEY_TRACK, &mut self.key_track), (p::RESIDUE, &mut self.residue), (p::SPREAD, &mut self.spread), (p::SUSTAIN, &mut self.sustain)] {
            o.set_target(v[i]);
            o.settle();
        }
        self.clock = 0;
        self.drive_lp = v[p::DRIVE] as f64 * 0.24;
        self.eng_x = if self.mono_engine() { 1.0 } else { 0.0 };
        self.amp_x = if idx(v[p::AMP_MODE], 2) == 1 { 1.0 } else { 0.0 };
        self.wave_b = idx(v[p::WAVE], 4);
        self.wave_a = self.wave_b;
        self.wave_x = 0.0;
        self.oct_b = v[p::OCTAVE].round().clamp(-2.0, 2.0) as i32;
        self.oct_a = self.oct_b;
        self.oct_x = 0.0;
        self.hpf.reset();
        self.down.reset();
        self.up_a.reset();
        self.up_b.reset();
        self.dbuf = [0.0; MAX_FACTOR];
        self.ua = [0.0; MAX_FACTOR];
        self.ub = [0.0; MAX_FACTOR];
        self.dn = 0;
        self.lowest_set = -1.0;
        self.set_lowest(true);
        self.lvl.reset();
        self.lvl_now = 0.0;
        self.hist = [-240.0; MUTE_WIN];
        self.hist_at = 0;
        self.pick.reset();
        self.jump.reset();
        self.jump_at = 0;
        self.yin.reset();
        self.mp.reset();
        self.pick_flag = false;
        self.onset_at = None;
        self.await_from = None;
        self.agree = 0.0;
        self.oct_since = None;
        self.spent = false;
        self.note_on = false;
        self.strength_peak = 0.0;
        self.strength_ref = 0.0;
        self.strength_until = 0;
        self.fenv_depth = 1.0;
        self.adsr = Adsr::new();
        self.fenv = FiltEnv::new();
        self.core_env = 0.0;
        self.core_rise = Rise::default();
        let cfg = self.filter_cfg();
        for vc in self.voices.iter_mut() {
            vc.reset(cfg);
        }
        for f in self.vcf_core.iter_mut() {
            f.reset(cfg);
        }
        self.ceiling = [Ceiling::default(), Ceiling::default()];
        self.byp = self.b(p::BYPASS);
        self.byp_x = if self.byp { 1.0 } else { 0.0 };
        self.byp_synth = if self.byp { 0.0 } else { 1.0 };
        self.byp_step = 1.0 / (BYPASS_MS * 1e-3 * self.sr as f64);
        self.m_state = 0.0;
        self.m_note = 0.0;
        self.out_env = 0.0;
        // the core: its settings first (the bands under `lowest` start disabled)
        self.control_settings_only();
        self.core.reset();
    }

    fn set_param(&mut self, id: &str, value: f32) {
        let Some(i) = PARAMS.iter().position(|p| p.id == id) else {
            return;
        };
        let pi = PARAMS[i];
        let value = if value.is_nan() { pi.default } else { value.clamp(pi.min, pi.max) };
        self.v[i] = value;
        let now = self.clock;
        match i {
            p::CUTOFF => self.cutoff.set_target(value, now),
            p::RESO => self.reso.set_target(value, now),
            p::FILT_ENV => self.filt_env.set_target(value, now),
            p::SYNTH => self.synth.set_target(value, now),
            p::DRY => self.dry.set_target(value, now),
            p::PW => self.pw.set_target(value, now),
            p::SUB => self.sub.set_target(value, now),
            p::DETUNE => self.detune.set_target(value, now),
            p::ATTACK => self.attack.set_target(value, now),
            p::COMPRESS => self.compress.set_target(value, now),
            p::DRIVE => self.drive.set_target(value, now),
            p::FENV_DEC => self.fenv_dec.set_target(value, now),
            p::GLIDE => self.glide.set_target(value, now),
            p::NOISE => self.noise.set_target(value),
            p::KEY_TRACK => self.key_track.set_target(value),
            p::RESIDUE => self.residue.set_target(value),
            p::SPREAD => self.spread.set_target(value),
            p::SUSTAIN => self.sustain.set_target(value),
            p::LOWEST => self.set_lowest(false),
            _ => {}
        }
    }

    fn meter(&self, id: &str) -> Option<f32> {
        match id {
            "state" => Some(self.m_state),
            "note" => Some(self.m_note),
            _ => None,
        }
    }

    fn latency(&self) -> usize {
        0
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(self.nch.max(1));
        if nch == 0 {
            return;
        }
        let n = buffers[0].len();
        for i in 0..n {
            let x = if nch == 2 { 0.5 * (buffers[0][i] + buffers[1][i]) } else { buffers[0][i] };
            let dry = self.dry_gain();
            let y = self.tick(x);
            if nch == 2 {
                buffers[0][i] = buffers[0][i] * dry + y[0];
                buffers[1][i] = buffers[1][i] * dry + y[1];
            } else {
                buffers[0][i] = buffers[0][i] * dry + y[0];
            }
        }
    }
}

impl Polysynth {
    /// The dry's gain for the coming sample (`dry`, −60 = off; unity when bypassed).
    #[inline]
    fn dry_gain(&self) -> f32 {
        let d = self.dry.value() as f64;
        let g = if d <= -59.999 { 0.0 } else { db_to_lin(d) };
        (g * (1.0 - self.byp_x) + self.byp_x) as f32
    }

    /// The core's settings from the parameters, without the rest of the control tick (reset).
    fn control_settings_only(&mut self) {
        let v = self.v;
        let pw = (v[p::PW] as f64 * 0.01).clamp(0.05, 0.95);
        self.cs = CoreSettings {
            bank: match idx(v[p::BANK], 3) {
                1 => BankMode::Fast,
                2 => BankMode::Clean,
                _ => BankMode::Dual,
            },
            c: coefs(self.wave_b, pw),
            oct_a: self.oct_b,
            oct_b: self.oct_b,
            oct_x: 1.0,
            sub: v[p::SUB] as f64 * 0.01,
            noise: v[p::NOISE] as f64 * 0.01,
            compress: v[p::COMPRESS] as f64 * 0.01,
            detune: v[p::DETUNE] as f64,
            thresh_db: v[p::THRESH] as f64,
            lowest: self.lowest_set as f64,
            mono: self.eng_x,
            residue: v[p::RESIDUE] as f64 * 0.01,
        };
        self.core.set(self.cs);
    }
}
