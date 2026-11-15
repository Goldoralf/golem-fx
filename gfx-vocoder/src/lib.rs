// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-vocoder — golem's channel vocoder (design: README.md): a bank of band-pass filters measures the
//! modulator's (the main input's) spectral envelope and imposes it, band by band, on a carrier.
//!
//! - **Banks** (gfx-dsp `filterbank`): N complex 4th-order gammatone-like cascades on gfx-vocoder's
//!   scale (`GammaDesign`: ERB or Log, 40 Hz floor, width law, edge trims, G_Σ). Analysis widths
//!   × `width`, synthesis widths × min(`width`, 100 %). `bands`, `spacing`, `low`, `high` and `width`
//!   design a second core (banks, followers, sibilance filter), the two run together for 40 ms and
//!   their outputs are crossfaded.
//! - **Envelopes**: 2 |z| of each analysis band, a peak follower (`env_att` / `env_rel`), × the gate's
//!   gain (`thresh`: a sliding maximum over one period of `register`'s lowest note, 6 dB of hysteresis),
//!   a 60 ms slow copy for `freeze`, the formant remap (a fractional band index along the scale, 6 dB
//!   per band beyond the edges) and `tilt`, all at the control tick (1 / 1,500 s) for their weights.
//! - **Carrier** (`source`): Tracked (YIN on `register`'s range, a 40 ms octave guard, `quantize` with
//!   hysteresis, `chord` from `harmony_interval`, a pending state at phrase starts: −12 dB under the
//!   noise until the first voiced frame), Drone (a diatonic chord on `degree` of `key` / `scale`),
//!   Noise (seeded white) or Self (the main input). gfx-dsp `osc`: Saw, Pulse, Buzz, Supersaw.
//! - **Flatten**: band k × (M_k / C̄) · (C̄ / max(C_k, C̄ / 100))^`flatten`, C_k the synthesis band's
//!   2 |z| followed 5 / 30 ms, C̄ their power mean over 200 ms (per tick); 1 / C̄ ≤ +42 dB and a carrier
//!   under −70 dBFS fades out.
//! - **V/UV**: the analysis bands' raw energy from 3 kHz up (plus the sibilance path) against the
//!   energy under 1 kHz, on 2 / 10 ms followers, 3 dB of hysteresis, gate open; u slews 3 ms up / 20 ms
//!   down. a = `unvoiced` · u crossfades each carrier band (cos) with its noise band (sin), a separate
//!   bank of white noise from 500 Hz up at unit envelope; `noise` adds the noise bands always.
//! - **Sibilance**: the modulator through a 4th-order Linkwitz-Riley high-pass at `high`, × `sib` ·
//!   (0.25 + 0.75 u) · the gate's gain.
//! - **Out**: G_Σ · Σ, −1.5 dB, `spread` (even bands left, odd right, constant power), equal-power `mix`
//!   with the dry main, `output`, a 10 ms `bypass` crossfade.
//!
//! Zero latency. Real-time safe (all memory sized in `prepare`), deterministic, block-size independent.

#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

#[cfg(feature = "plugin")]
mod plugin;

#[cfg(test)]
mod tests;

use gfx_dsp::detect::{harmony_interval, midi_of, PitchDetector, SCALES};
use gfx_dsp::envelope::SlidingMax;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::filterbank::{noise_power, BankFade, ComplexBank, GammaDesign, Spacing};
use gfx_dsp::noise::{White, WHITE_RMS};
use gfx_dsp::osc::{buzz, pulse, saw, Supersaw};
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::util::one_pole_coef;
use gfx_dsp::{Effect, ParamInfo, ParamKind};
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, SQRT_2};

pub const NP: usize = 30;

const fn int(id: &'static str, name: &'static str, min: f32, max: f32, default: f32) -> ParamInfo {
    ParamInfo { id, name, min, max, default, kind: ParamKind::Int, exp: false }
}

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::enumeration("source", "Carrier", 4, 0),
    ParamInfo::enumeration("chord", "Chord", 6, 0),
    ParamInfo::float("formant", "Formant", -12.0, 12.0, 0.0).with_exp(),
    ParamInfo::float("env_rel", "Release", 20.0, 2000.0, 40.0).with_exp(),
    ParamInfo::float("sib", "Sibilance", 0.0, 100.0, 50.0).with_exp(),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::boolean("freeze", "Freeze", false),
    int("bands", "Bands", 8.0, 40.0, 24.0),
    ParamInfo::enumeration("spacing", "Spacing", 2, 0),
    ParamInfo::float("low", "Low", 50.0, 500.0, 100.0),
    ParamInfo::float("high", "High", 2000.0, 12000.0, 7000.0),
    ParamInfo::float("width", "Width", 50.0, 200.0, 100.0),
    ParamInfo::float("env_att", "Attack", 1.0, 100.0, 3.0),
    ParamInfo::float("thresh", "Gate", -90.0, 0.0, -60.0),
    ParamInfo::float("flatten", "Flatten", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::float("unvoiced", "Unvoiced", 0.0, 100.0, 50.0),
    ParamInfo::float("noise", "Noise", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("tilt", "Tilt", -6.0, 6.0, 0.0).with_exp(),
    ParamInfo::enumeration("wave", "Wave", 4, 0),
    ParamInfo::float("detune", "Detune", 0.0, 50.0, 15.0),
    ParamInfo::enumeration("register", "Register", 4, 1),
    ParamInfo::enumeration("quantize", "Quantize", 3, 0),
    ParamInfo::float("glide", "Glide", 0.0, 200.0, 0.0),
    int("octave", "Octave", -2.0, 2.0, 0.0),
    int("degree", "Degree", 1.0, 7.0, 1.0),
    ParamInfo::enumeration("key", "Key", 12, 0),
    ParamInfo::enumeration("scale", "Scale", 10, 0),
    ParamInfo::float("spread", "Spread", 0.0, 100.0, 0.0),
    ParamInfo::float("output", "Output", -18.0, 18.0, 0.0),
    ParamInfo::boolean("bypass", "Bypass", false),
];

pub mod p {
    pub const SOURCE: usize = 0;
    pub const CHORD: usize = 1;
    pub const FORMANT: usize = 2;
    pub const ENV_REL: usize = 3;
    pub const SIB: usize = 4;
    pub const MIX: usize = 5;
    pub const FREEZE: usize = 6;
    pub const BANDS: usize = 7;
    pub const SPACING: usize = 8;
    pub const LOW: usize = 9;
    pub const HIGH: usize = 10;
    pub const WIDTH: usize = 11;
    pub const ENV_ATT: usize = 12;
    pub const THRESH: usize = 13;
    pub const FLATTEN: usize = 14;
    pub const UNVOICED: usize = 15;
    pub const NOISE: usize = 16;
    pub const TILT: usize = 17;
    pub const WAVE: usize = 18;
    pub const DETUNE: usize = 19;
    pub const REGISTER: usize = 20;
    pub const QUANTIZE: usize = 21;
    pub const GLIDE: usize = 22;
    pub const OCTAVE: usize = 23;
    pub const DEGREE: usize = 24;
    pub const KEY: usize = 25;
    pub const SCALE: usize = 26;
    pub const SPREAD: usize = 27;
    pub const OUTPUT: usize = 28;
    pub const BYPASS: usize = 29;
}

/// `source` option labels (enum order, final: append only; Key Input, Talk Box and MIDI come later).
pub const SOURCES: [&str; 4] = ["Tracked", "Drone", "Noise", "Self"];
/// `chord` option labels (enum order, final).
pub const CHORDS: [&str; 6] = ["Unison", "Octave", "Fifth", "Triad", "Seventh", "Ninth"];
/// `spacing` option labels.
pub const SPACINGS: [&str; 2] = ["ERB", "Log"];
/// `wave` option labels.
pub const WAVES: [&str; 4] = ["Saw", "Pulse", "Buzz", "Supersaw"];
/// `register`: (lowest, highest) fundamental, Hz — gfx-golem's three voice ranges, Instrument appended.
pub const REGISTERS: [(f32, f32); 4] = [(60.0, 500.0), (90.0, 800.0), (150.0, 1200.0), (75.0, 1500.0)];
/// `register` labels.
pub const REGISTER_NAMES: [&str; 4] = ["Low", "Mid", "High", "Instrument"];
/// `quantize` labels.
pub const QUANTIZES: [&str; 3] = ["Off", "Chromatic", "Key"];
/// `key` labels (gfx-pitch's).
pub const KEYS: [&str; 12] = ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];
/// `scale` labels (gfx-pitch's order).
pub const SCALE_NAMES: [&str; 10] =
    ["Major", "Minor", "Harmonic Minor", "Melodic Minor", "Dorian", "Phrygian", "Lydian", "Mixolydian", "Major Pentatonic", "Minor Pentatonic"];
/// Read-only meters (`meter`).
pub const METERS: [&str; 2] = ["f0", "state"];

const TRACKED: usize = 0;
const DRONE: usize = 1;
const NOISE: usize = 2;
const SELF: usize = 3;
const SUPERSAW: usize = 3;
const MAX_BANDS: usize = 40;
const VOICES: usize = 5;
const TICK_HZ: f32 = 1500.0;
/// A ×2 / ÷2 jump must last this long to be believed (gfx-golem's `OCTAVE_CONFIRM_MS`).
pub const OCTAVE_CONFIRM_MS: f32 = 40.0;
/// No voiced frame for this long: the next one starts a phrase (pending carrier until then).
pub const PHRASE_MS: f32 = 80.0;
/// The pending carrier's level.
pub const PENDING_GAIN: f64 = 0.25;
/// The wet's fixed trim at `flatten` 100 % (dB): centres the followers' peak bias.
pub const LEVEL_TRIM_DB: f64 = -1.5;
/// Quantize: a new note must be this much nearer than the held one (semitones), three analyses running.
const QUANT_MARGIN: f32 = 0.3;
const QUANT_CONFIRM: u8 = 3;
const SLOW_MS: f32 = 60.0;
const FREEZE_MS: f32 = 30.0;
const SOURCE_MS: f32 = 30.0;
const WAVE_MS: f32 = 20.0;
const VOICE_FADE_MS: f32 = 5.0;
const CAR_ATT_MS: f32 = 5.0;
const CAR_REL_MS: f32 = 30.0;
const CBAR_MS: f32 = 200.0;
/// 1 / C̄ is clamped at +24 dB above a −18 dBFS carrier.
const INV_MAX: f64 = 125.892_541_179_416_7;
/// Flatten's floor under C̄ (−40 dB).
const FLAT_FLOOR: f64 = 0.01;
/// The noise bands' level against a flattened carrier band (−2.7 dB).
const NOISE_BAND_TRIM: f64 = 0.732_824;
/// Per-`wave` trim at `flatten` 0 (dB, scaled by 1 − `flatten`): the level a saw's tilt adds under a
/// pink modulator (§2.7; measured: saw +3.4, pulse +2.2, supersaw +3.1 dB; buzz flat).
pub const WAVE_TRIM_DB: [f64; 4] = [-3.4, -2.2, 0.0, -3.1];
const VUV_HI_HZ: f64 = 3000.0;
const VUV_LO_HZ: f64 = 1000.0;
const NOISE_FROM_HZ: f64 = 500.0;

#[inline]
fn idx(v: f32, n: usize) -> usize {
    (v.round().max(0.0) as usize).min(n - 1)
}

#[inline]
fn hz_of(midi: f64) -> f64 {
    440.0 * ((midi - 69.0) / 12.0).exp2()
}

/// The note held by `quantize`: a new note is taken when it is QUANT_MARGIN semitones nearer than the
/// held one in QUANT_CONFIRM analyses running (a vibrato across a boundary does not trill).
#[derive(Clone, Copy, Debug, Default)]
struct Quant {
    note: Option<i32>,
    cand: i32,
    count: u8,
}

impl Quant {
    fn update(&mut self, m: f32, allowed: impl Fn(i32) -> bool) {
        let r = m.round() as i32;
        let mut best = r;
        let mut bd = f32::MAX;
        for n in r - 6..=r + 6 {
            let d = (m - n as f32).abs();
            if allowed(n) && d < bd {
                bd = d;
                best = n;
            }
        }
        match self.note {
            None => self.note = Some(best),
            Some(cur) if cur == best => self.count = 0,
            Some(cur) => {
                if bd + QUANT_MARGIN < (m - cur as f32).abs() {
                    if self.cand == best {
                        self.count += 1;
                    } else {
                        self.cand = best;
                        self.count = 1;
                    }
                    if self.count >= QUANT_CONFIRM {
                        self.note = Some(best);
                        self.count = 0;
                    }
                } else {
                    self.count = 0;
                }
            }
        }
    }
}

/// The setup values that design a core.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Setup {
    bands: usize,
    spacing: usize,
    low: f32,
    high: f32,
    width: f32,
}

/// Per-sample values shared by the cores.
#[derive(Clone, Copy, Debug, Default)]
struct Frame {
    gg: f64,
    gate_open: bool,
    fx: f64,
    att: f64,
    rel: f64,
    unvoiced: f64,
    noise: f64,
    sib: f64,
    pending: f64,
    car_gain: f64,
    nz_on: bool,
    flatten: f64,
}

/// One bank set: analysis, synthesis (one or two carrier sides), noise bank, followers, sibilance.
struct Core {
    d: GammaDesign,
    setup: Setup,
    n: usize,
    ana: ComplexBank,
    syn: [ComplexBank; 2],
    nz: ComplexBank,
    nz_from: usize,
    nz_scale: Vec<f64>,
    gain: Vec<f64>,
    fc: Vec<f64>,
    env: Vec<f64>,
    slow: Vec<f64>,
    held: Vec<f64>,
    e: Vec<f64>,
    car: [Vec<f64>; 2],
    /// per carrier side, per tick: (1 / C̄ clamped and faded, flatten's floor, C̄)
    flat: [(f64, f64, f64); 2],
    amp: Vec<f64>,
    mk: Vec<f64>,
    o: Vec<f64>,
    nzv: Vec<f64>,
    /// remap: (i0, i1, w0, w1) per synthesis band, tilt included
    mw: Vec<(usize, usize, f64, f64)>,
    mw_key: (f32, f32),
    mw_prev: Vec<(usize, usize, f64, f64)>,
    mw_on: bool,
    mw_t: f64,
    a_key: f64,
    a_cs: (f64, f64),
    cbar: [f64; 2],
    hi_from: usize,
    lo_to: usize,
    vhi: f64,
    vlo: f64,
    uv: bool,
    u: f64,
    sib: [Svf; 2],
    sr: f64,
}

impl Core {
    fn new() -> Self {
        let v = || vec![0.0; MAX_BANDS];
        Self {
            d: GammaDesign::new(MAX_BANDS),
            setup: Setup { bands: 0, spacing: 0, low: 0.0, high: 0.0, width: 0.0 },
            n: 0,
            ana: ComplexBank::new(),
            syn: [ComplexBank::new(), ComplexBank::new()],
            nz: ComplexBank::new(),
            nz_from: 0,
            nz_scale: v(),
            gain: v(),
            fc: v(),
            env: v(),
            slow: v(),
            held: v(),
            e: v(),
            car: [v(), v()],
            flat: [(0.0, 0.0, 0.0); 2],
            amp: v(),
            mk: v(),
            o: v(),
            nzv: v(),
            mw: vec![(0, 0, 1.0, 0.0); MAX_BANDS],
            mw_key: (f32::NAN, f32::NAN),
            mw_prev: vec![(0, 0, 1.0, 0.0); MAX_BANDS],
            mw_on: false,
            mw_t: 1.0,
            a_key: 0.0,
            a_cs: (1.0, 0.0),
            cbar: [0.0; 2],
            hi_from: MAX_BANDS,
            lo_to: 0,
            vhi: 0.0,
            vlo: 0.0,
            uv: false,
            u: 0.0,
            sib: [Svf::new(), Svf::new()],
            sr: 48000.0,
        }
    }

    fn prepare(&mut self) {
        self.ana.prepare(MAX_BANDS);
        for s in self.syn.iter_mut() {
            s.prepare(MAX_BANDS);
        }
        self.nz.prepare(MAX_BANDS);
    }

    /// Designs every bank for `s` (bounded, no allocation) and clears the state.
    fn design(&mut self, s: Setup, sr: f64) {
        self.setup = s;
        self.sr = sr;
        let spacing = if s.spacing == 1 { Spacing::Log } else { Spacing::Erb };
        let w = (s.width as f64 / 100.0).clamp(0.5, 2.0);
        let high = (s.high as f64).min(0.45 * sr);
        self.d.design(s.bands, s.low as f64, high, spacing, w.min(1.0), sr);
        let n = self.d.len();
        self.n = n;
        self.d.fill(&mut self.ana, 0, w);
        for c in 0..2 {
            let syn = &mut self.syn[c];
            self.d.fill(syn, 0, w.min(1.0));
        }
        self.nz_from = (0..n).find(|&k| self.d.centre(k) >= NOISE_FROM_HZ).unwrap_or(n);
        self.d.fill(&mut self.nz, self.nz_from, w.min(1.0));
        let trim = 10f64.powf(LEVEL_TRIM_DB / 20.0);
        for k in 0..n {
            self.fc[k] = self.d.centre(k);
            self.gain[k] = self.d.trim(k) * self.d.sum_gain() * trim;
        }
        for k in self.nz_from..n {
            let np = noise_power(self.nz.spec(k - self.nz_from), sr);
            // a unit envelope (2 |z| of white noise at RMS 1), brought to the level of a flattened noise
            // carrier band (whose divisor rides the envelope's peaks: −2.7 dB, measured)
            self.nz_scale[k] = NOISE_BAND_TRIM / (4.0 * WHITE_RMS * WHITE_RMS * np).sqrt();
        }
        self.hi_from = (0..n).find(|&k| self.fc[k] >= VUV_HI_HZ).unwrap_or(n);
        self.lo_to = (0..n).find(|&k| self.fc[k] > VUV_LO_HZ).unwrap_or(n);
        let fhp = high as f32;
        for f in self.sib.iter_mut() {
            f.set(SvfMode::HighPass, fhp, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr as f32);
        }
        self.mw_key = (f32::NAN, f32::NAN);
        self.mw_on = false;
        self.mw_t = 1.0;
        self.reset();
    }

    fn reset(&mut self) {
        self.ana.reset();
        for s in self.syn.iter_mut() {
            s.reset();
        }
        self.nz.reset();
        for v in [&mut self.env, &mut self.slow, &mut self.held, &mut self.e] {
            v.iter_mut().for_each(|x| *x = 0.0);
        }
        for c in 0..2 {
            self.car[c].iter_mut().for_each(|x| *x = 0.0);
        }
        self.flat = [(0.0, 0.0, 0.0); 2];
        self.nzv.iter_mut().for_each(|x| *x = 0.0);
        self.cbar = [0.0; 2];
        self.vhi = 0.0;
        self.vlo = 0.0;
        self.uv = false;
        self.u = 0.0;
        for f in self.sib.iter_mut() {
            f.reset();
        }
    }

    /// The second carrier side starts as a copy of the first (they had the same input so far).
    fn split(&mut self) {
        let (a, b) = self.syn.split_at_mut(1);
        b[0].copy_state_from(&a[0]);
        let n = self.n;
        let (c0, c1) = self.car.split_at_mut(1);
        c1[0][..n].copy_from_slice(&c0[0][..n]);
        self.flat[1] = self.flat[0];
        self.cbar[1] = self.cbar[0];
    }

    /// Freeze: the slow copy becomes the held envelope.
    fn capture(&mut self) {
        let n = self.n;
        self.held[..n].copy_from_slice(&self.slow[..n]);
    }

    /// The control tick: C̄, 1 / C̄ (clamped, faded under −70 dBFS) and flatten's floor; the remap weights.
    fn tick(&mut self, ncar: usize, formant: f32, tilt: f32, cbar_c: f64) {
        let n = self.n;
        if self.mw_t >= 1.0 {
            self.mw_on = false;
        }
        if (formant, tilt) != self.mw_key {
            // the new weights are reached over one tick (no step in a band's gain)
            if self.mw_key.0.is_finite() {
                self.mw_prev[..n].copy_from_slice(&self.mw[..n]);
                self.mw_on = true;
                self.mw_t = 0.0;
            }
            self.mw_key = (formant, tilt);
            let ratio = (-(formant as f64) / 12.0).exp2();
            let te = tilt as f64 / 6.02;
            for k in 0..n {
                let j = if formant == 0.0 { k as f64 } else { self.d.index_of(self.fc[k] * ratio) };
                let t = if tilt == 0.0 { 1.0 } else { (self.fc[k] / 1000.0).powf(te) };
                self.mw[k] = if j <= 0.0 {
                    (0, 0, t * 10f64.powf(-0.3 * (-j)), 0.0)
                } else if j >= (n - 1) as f64 {
                    (n - 1, n - 1, t * 10f64.powf(-0.3 * (j - (n - 1) as f64)), 0.0)
                } else {
                    let i0 = j.floor() as usize;
                    let w1 = j - i0 as f64;
                    (i0, (i0 + 1).min(n - 1), t * (1.0 - w1), t * w1)
                };
            }
        }
        for c in 0..ncar {
            let car = &self.car[c][..n];
            let ms = car.iter().map(|v| v * v).sum::<f64>() / n.max(1) as f64;
            self.cbar[c] = ms.sqrt() + (self.cbar[c] - ms.sqrt()) * cbar_c;
            let cb = self.cbar[c];
            let db = 20.0 * cb.max(1e-12).log10();
            let present = ((db + 76.0) / 6.0).clamp(0.0, 1.0);
            let inv = if cb > 0.0 { (1.0 / cb).min(INV_MAX) } else { 0.0 } * present;
            self.flat[c] = (inv, cb * FLAT_FLOOR, cb);
        }
    }

    /// One sample: the modulator `m`, the carrier sides, the noise sample. Returns the band sums per
    /// carrier side (even, odd bands) and the sibilance path.
    #[inline]
    fn process(&mut self, m: f64, carrier: [f64; 2], ncar: usize, white: f64, fr: &Frame, k: &Coefs) -> ([f64; 2], [f64; 2], f64) {
        let n = self.n;
        self.ana.process(m);
        {
            let re = self.ana.re();
            let im = self.ana.im();
            let amp = &mut self.amp[..n];
            let env = &mut self.env[..n];
            let slow = &mut self.slow[..n];
            let held = &self.held[..n];
            let e = &mut self.e[..n];
            for b in 0..n {
                let a = 2.0 * (re[b] * re[b] + im[b] * im[b]).sqrt();
                amp[b] = a;
                let c = if a > env[b] { fr.att } else { fr.rel };
                env[b] = a + (env[b] - a) * c;
                let live = env[b] * fr.gg;
                slow[b] = live + (slow[b] - live) * k.slow;
                e[b] = live + (held[b] - live) * fr.fx;
            }
        }
        let ehi: f64 = self.amp[self.hi_from.min(n)..n].iter().map(|a| a * a).sum();
        let elo: f64 = self.amp[..self.lo_to.min(n)].iter().map(|a| a * a).sum();
        let x1 = self.sib[0].process(m as f32);
        let xs = self.sib[1].process(x1) as f64;
        let ehi = ehi + 2.0 * xs * xs;
        self.vhi = ehi + (self.vhi - ehi) * if ehi > self.vhi { k.v_att } else { k.v_rel };
        self.vlo = elo + (self.vlo - elo) * if elo > self.vlo { k.v_att } else { k.v_rel };
        if !self.uv && self.vhi > self.vlo * 1.412_537_5 {
            self.uv = true;
        } else if self.uv && self.vhi < self.vlo * 0.707_945_8 {
            self.uv = false;
        }
        let ut = if self.uv && fr.gate_open { 1.0 } else { 0.0 };
        self.u = if ut > self.u { (self.u + k.u_up).min(ut) } else { (self.u - k.u_dn).max(ut) };
        let a = fr.unvoiced * self.u.max(fr.pending);
        if a != self.a_key {
            self.a_key = a;
            self.a_cs = if a <= 0.0 { (1.0, 0.0) } else { ((FRAC_PI_2 * a).cos(), (FRAC_PI_2 * a).sin()) };
        }
        let (ca, sa) = self.a_cs;
        let cg = ca * fr.car_gain;
        let ng = sa + fr.noise;
        // the modulator envelope each synthesis band reads (remap, tilt, trims, G_Σ)
        {
            let e = &self.e[..n];
            let mk = &mut self.mk[..n];
            if self.mw_on {
                self.mw_t = (self.mw_t + k.tick_step).min(1.0);
                let t = self.mw_t;
                for b in 0..n {
                    let (i0, i1, w0, w1) = self.mw[b];
                    let (j0, j1, v0, v1) = self.mw_prev[b];
                    let cur = e[i0] * w0 + e[i1] * w1;
                    let old = e[j0] * v0 + e[j1] * v1;
                    mk[b] = (old + (cur - old) * t) * self.gain[b];
                }
            } else {
                for b in 0..n {
                    let (i0, i1, w0, w1) = self.mw[b];
                    mk[b] = (e[i0] * w0 + e[i1] * w1) * self.gain[b];
                }
            }
        }
        let nz_on = fr.nz_on && ng > 0.0;
        if nz_on {
            self.nz.process(white);
            let nre = self.nz.re();
            let from = self.nz_from;
            let nv = &mut self.nzv[from..n];
            let sc = &self.nz_scale[from..n];
            for b in 0..n - from {
                nv[b] = ng * 2.0 * nre[b] * sc[b];
            }
        }
        let flatten = fr.flatten;
        let mut even = [0.0; 2];
        let mut odd = [0.0; 2];
        for c in 0..ncar {
            self.syn[c].process(carrier[c]);
            let re = self.syn[c].re();
            let im = self.syn[c].im();
            let car = &mut self.car[c][..n];
            let o = &mut self.o[..n];
            let (inv, floor, cb) = self.flat[c];
            for b in 0..n {
                let a = 2.0 * (re[b] * re[b] + im[b] * im[b]).sqrt();
                let cc = if a > car[b] { k.c_att } else { k.c_rel };
                car[b] = a + (car[b] - a) * cc;
                // the denominator never falls under the band's own envelope: no overshoot while
                // C_k rises after a note change
                let den = a.max(car[b]).max(floor).max(1e-30);
                o[b] = 2.0 * re[b] * inv * (cb / den);
            }
            if flatten < 1.0 {
                // (C̄ / den)^f = (C̄ / den) · (C̄ / den)^(f − 1)
                let f1 = flatten - 1.0;
                for b in 0..n {
                    let a = 2.0 * (re[b] * re[b] + im[b] * im[b]).sqrt();
                    let den = a.max(car[b]).max(floor).max(1e-30);
                    o[b] *= fast_pow2(f1 * fast_log2(cb / den));
                }
            }
            let mk = &self.mk[..n];
            let nv = &self.nzv[..n];
            let (mut se, mut so) = (0.0, 0.0);
            let from = if nz_on { self.nz_from } else { n };
            let mut b = 0;
            while b < n {
                let ve = mk[b] * (cg * o[b] + if b >= from { nv[b] } else { 0.0 });
                se += ve;
                if b + 1 < n {
                    let b1 = b + 1;
                    so += mk[b1] * (cg * o[b1] + if b1 >= from { nv[b1] } else { 0.0 });
                }
                b += 2;
            }
            even[c] = se;
            odd[c] = so;
        }
        if ncar == 1 {
            even[1] = even[0];
            odd[1] = odd[0];
        }
        let sib = xs * fr.sib * (0.25 + 0.75 * self.u) * fr.gg;
        (even, odd, sib)
    }
}

/// log2 of a positive finite value (≈ 1e−5 absolute): exponent + an atanh series on the mantissa.
#[inline]
fn fast_log2(x: f64) -> f64 {
    let bits = x.to_bits();
    let e = ((bits >> 52) & 0x7ff) as i64 - 1023;
    let m = f64::from_bits((bits & 0x000f_ffff_ffff_ffff) | 0x3ff0_0000_0000_0000);
    let t = (m - 1.0) / (m + 1.0);
    let t2 = t * t;
    e as f64 + t * (2.885_390_081_777_927 + t2 * (0.961_796_693_925_975_6 + t2 * (0.577_078_016_355_585_4 + t2 * 0.412_198_583_111_132_4)))
}

/// 2^y (≈ 2e−6 relative), clamped to ±1000.
#[inline]
fn fast_pow2(y: f64) -> f64 {
    let y = y.clamp(-1000.0, 1000.0);
    let i = y.floor();
    let f = (y - i) * std::f64::consts::LN_2;
    let p = 1.0 + f * (1.0 + f * (0.5 + f * (1.0 / 6.0 + f * (1.0 / 24.0 + f * (1.0 / 120.0 + f * (1.0 / 720.0))))));
    p * f64::from_bits(((i as i64 + 1023) as u64) << 52)
}

/// Coefficients of the fixed-time followers.
#[derive(Clone, Copy, Debug, Default)]
struct Coefs {
    slow: f64,
    v_att: f64,
    v_rel: f64,
    u_up: f64,
    u_dn: f64,
    c_att: f64,
    c_rel: f64,
    tick_step: f64,
}

/// One oscillator voice: a supersaw whose centre saw is the plain waves' phasor.
#[derive(Clone, Debug)]
struct Voice {
    ss: Supersaw,
    semi: f64,
    gain: f64,
    target: f64,
}

/// golem's channel vocoder.
pub struct Vocoder {
    v: [f32; NP],
    sr: f32,
    nch: usize,
    ctl: u32,
    clock: u64,
    cores: [Core; 2],
    act: usize,
    fade: BankFade,
    // smoothers
    formant: ExpSmoother,
    env_rel: ExpSmoother,
    sib: ExpSmoother,
    mix: ExpSmoother,
    flatten: ExpSmoother,
    noise: ExpSmoother,
    tilt: ExpSmoother,
    env_att: OnePole,
    thresh: OnePole,
    unvoiced: OnePole,
    detune: OnePole,
    spread: OnePole,
    output: OnePole,
    octave: OnePole,
    freeze: LinearRamp,
    source_fade: LinearRamp,
    src_cur: usize,
    src_prev: usize,
    wave_fade: LinearRamp,
    wave_cur: usize,
    wave_prev: usize,
    bypass: LinearRamp,
    // per-tick values
    k: Coefs,
    att: f64,
    rel: f64,
    // gate
    smax: SlidingMax,
    gate_open: bool,
    gg: f64,
    // tracking
    det: [PitchDetector; 2],
    f_det: f32,
    octave_pending: u32,
    since_voiced: u64,
    pending: bool,
    root: f64,
    root_target: f64,
    chrom: Quant,
    keyq: Quant,
    pc: u8,
    jump: bool,
    car_gain: f64,
    // carrier
    voices: Vec<Voice>,
    white_src: White,
    white_nz: White,
    split: bool,
    // mix cache
    mix_key: f32,
    mix_cs: (f64, f64),
    spread_key: f32,
    spread_g: [f64; 4],
    th_key: f32,
    th_lin: f64,
    trim_key: f64,
    trim_lin: f64,
    // meters
    f0_meter: f32,
}

impl Vocoder {
    pub fn new() -> Self {
        let v: [f32; NP] = std::array::from_fn(|i| PARAMS[i].default);
        let voices = (0..VOICES).map(|i| Voice { ss: Supersaw::new(0x5EED + i as u64), semi: 57.0, gain: 0.0, target: 0.0 }).collect();
        Self {
            v,
            sr: 48000.0,
            nch: 1,
            ctl: 32,
            clock: 0,
            cores: [Core::new(), Core::new()],
            act: 0,
            fade: BankFade::default(),
            formant: ExpSmoother::new(-12.0, 12.0, v[p::FORMANT]),
            env_rel: ExpSmoother::new(20.0, 2000.0, v[p::ENV_REL]),
            sib: ExpSmoother::new(0.0, 100.0, v[p::SIB]),
            mix: ExpSmoother::new(0.0, 100.0, v[p::MIX]),
            flatten: ExpSmoother::new(0.0, 100.0, v[p::FLATTEN]),
            noise: ExpSmoother::new(0.0, 100.0, v[p::NOISE]),
            tilt: ExpSmoother::new(-6.0, 6.0, v[p::TILT]),
            env_att: OnePole::new(20.0, v[p::ENV_ATT], 1e-4),
            thresh: OnePole::new(20.0, v[p::THRESH], 1e-4),
            unvoiced: OnePole::new(20.0, v[p::UNVOICED], 1e-4),
            detune: OnePole::new(20.0, v[p::DETUNE], 1e-4),
            spread: OnePole::new(20.0, v[p::SPREAD], 1e-4),
            output: OnePole::new(20.0, v[p::OUTPUT], 1e-4),
            octave: OnePole::new(20.0, v[p::OCTAVE] * 12.0, 1e-4),
            freeze: LinearRamp::new(FREEZE_MS, 0.0),
            source_fade: LinearRamp::new(SOURCE_MS, 1.0),
            src_cur: 0,
            src_prev: 0,
            wave_fade: LinearRamp::new(WAVE_MS, 1.0),
            wave_cur: 0,
            wave_prev: 0,
            bypass: LinearRamp::new(10.0, 1.0),
            k: Coefs::default(),
            att: 0.0,
            rel: 0.0,
            smax: SlidingMax::new(),
            gate_open: false,
            gg: 0.0,
            det: [PitchDetector::default(), PitchDetector::default()],
            f_det: 0.0,
            octave_pending: 0,
            since_voiced: u64::MAX / 2,
            pending: true,
            root: 57.0,
            root_target: 57.0,
            chrom: Quant::default(),
            keyq: Quant::default(),
            pc: 9,
            jump: false,
            car_gain: PENDING_GAIN,
            voices,
            white_src: White::new(0xC0FFEE),
            white_nz: White::new(0xBEEF),
            split: false,
            mix_key: f32::NAN,
            mix_cs: (0.0, 1.0),
            spread_key: f32::NAN,
            spread_g: [1.0; 4],
            th_key: f32::NAN,
            th_lin: 0.0,
            trim_key: 0.0,
            trim_lin: 1.0,
            f0_meter: 0.0,
        }
    }

    fn setup(&self) -> Setup {
        let v = &self.v;
        Setup {
            bands: (v[p::BANDS].round() as i32).clamp(8, 40) as usize,
            spacing: idx(v[p::SPACING], 2),
            low: v[p::LOW].clamp(50.0, 500.0),
            high: v[p::HIGH].clamp(2000.0, 12000.0),
            width: v[p::WIDTH].clamp(50.0, 200.0),
        }
    }

    fn register(&self) -> usize {
        idx(self.v[p::REGISTER], 4)
    }

    /// The tracked fundamental (Hz, 0 = none) and the gate (1 = open), as the meters publish them.
    pub fn f0(&self) -> f32 {
        self.f0_meter
    }

    pub fn gate_is_open(&self) -> bool {
        self.gate_open
    }

    /// Test witnesses: the active core's V/UV value, the pending state, the carrier's root (MIDI).
    pub fn unvoiced_u(&self) -> f64 {
        self.cores[self.act].u
    }

    pub fn is_pending(&self) -> bool {
        self.pending
    }

    pub fn root_midi(&self) -> f64 {
        self.root
    }

    /// The voices' current notes (MIDI) and gains.
    pub fn voice_notes(&self) -> Vec<(f64, f64)> {
        self.voices.iter().map(|v| (v.semi, v.target)).collect()
    }

    /// The modulator envelope each synthesis band reads this sample (remap, tilt, trims, G_Σ; test
    /// witness).
    pub fn band_env(&self) -> &[f64] {
        let c = &self.cores[self.act];
        &c.mk[..c.n]
    }

    /// The active design (test witness).
    pub fn design(&self) -> &GammaDesign {
        &self.cores[self.act].d
    }

    fn uses(&self, s: usize) -> bool {
        self.src_cur == s || (!self.source_fade.is_settled() && self.src_prev == s)
    }

    /// The chord's intervals above the root for pitch class `pc`.
    fn intervals(&self, pc: u8) -> ([f64; VOICES], usize) {
        let key = idx(self.v[p::KEY], 12) as u8;
        let scale = idx(self.v[p::SCALE], 10);
        let h = |s: f32| harmony_interval(pc, key, scale, s) as f64;
        let mut iv = [0.0; VOICES];
        let n = match idx(self.v[p::CHORD], 6) {
            0 => 1,
            1 => {
                iv[1] = -12.0;
                2
            }
            2 => {
                iv[1] = h(7.0);
                iv[2] = -12.0;
                3
            }
            3 => {
                iv[1] = h(4.0);
                iv[2] = h(7.0);
                3
            }
            4 => {
                iv[1] = h(4.0);
                iv[2] = h(7.0);
                iv[3] = h(11.0);
                4
            }
            _ => {
                iv[1] = h(4.0);
                iv[2] = h(7.0);
                iv[3] = h(11.0);
                iv[4] = h(14.0);
                5
            }
        };
        (iv, n)
    }

    /// One YIN analysis → octave guard, phrase / pending state, quantize, the root's target.
    fn on_analysis(&mut self, d: usize) {
        let reg = self.register();
        let (lo, hi) = REGISTERS[reg];
        let det = &self.det[d];
        let hop = det.rate() as f64 / 32.0;
        let frames_confirm = ((OCTAVE_CONFIRM_MS as f64 * 0.001) * hop).ceil() as u32;
        let Some(f) = det.hz().filter(|f| *f >= lo && *f <= hi) else {
            return;
        };
        let mut f_new = f;
        if self.f_det > 0.0 && !self.pending {
            let r = f / self.f_det;
            if ((r - 2.0).abs() < 0.12 || (r - 0.5).abs() < 0.03) && self.octave_pending < frames_confirm {
                self.octave_pending += 1;
                f_new = self.f_det;
            } else {
                self.octave_pending = 0;
            }
        }
        self.f_det = f_new;
        self.since_voiced = 0;
        let m = midi_of(f_new);
        let key = idx(self.v[p::KEY], 12) as i32;
        let sc = SCALES[idx(self.v[p::SCALE], 10)];
        self.chrom.update(m, |_| true);
        self.keyq.update(m, |n| sc.contains(&(n - key).rem_euclid(12)));
        let q = idx(self.v[p::QUANTIZE], 3);
        let note = if q == 2 { self.keyq.note } else { self.chrom.note }.unwrap_or(m.round() as i32);
        self.pc = note.rem_euclid(12) as u8;
        self.root_target = match q {
            0 => m as f64,
            _ => note as f64,
        };
        if self.pending {
            // the first voiced frame of a phrase: on the note at once, full level in 5 ms
            self.pending = false;
            self.jump = true;
        }
    }

    /// The control tick.
    fn control(&mut self) {
        let n = self.ctl as f64;
        let sr = self.sr;
        let att = (self.env_att.value() as f64).clamp(1.0, 100.0);
        let rel = (self.env_rel.value() as f64).clamp(20.0, 2000.0);
        self.att = one_pole_coef(att * 0.001 * sr as f64);
        self.rel = one_pole_coef(rel * 0.001 * sr as f64);
        // the gate's window: one period of the register's lowest note
        let reg = self.register();
        self.smax.set_window((sr / REGISTERS[reg].0).ceil() as usize);
        // a setup change designs the other core and crossfades
        let s = self.setup();
        if !self.fade.is_running() && s != self.cores[self.act].setup {
            let other = 1 - self.act;
            let (cb, flat) = (self.cores[self.act].cbar, self.cores[self.act].flat);
            let c = &mut self.cores[other];
            c.design(s, sr as f64);
            // the carrier is the same: its level statistics carry over (no 200 ms settling)
            c.cbar = cb;
            c.flat = flat;
            if self.split {
                c.split();
            }
            self.act = other;
            self.fade.start();
        }
        // the carrier's notes
        let tracked = self.src_cur == TRACKED;
        let oct = self.octave.value() as f64;
        let (root, pc) = if tracked || (self.src_cur != DRONE && self.src_prev == TRACKED) {
            let q = idx(self.v[p::QUANTIZE], 3);
            let glide = self.v[p::GLIDE].clamp(0.0, 200.0) as f64;
            let tau = if q == 0 { 10.0 + glide } else { glide };
            if self.jump || tau <= 0.0 {
                self.root = self.root_target;
            } else {
                let c = one_pole_coef(tau * 0.001 * sr as f64 / n);
                self.root = self.root_target + (self.root - self.root_target) * c;
            }
            (self.root, self.pc)
        } else {
            let key = idx(self.v[p::KEY], 12) as i32;
            let sc = SCALES[idx(self.v[p::SCALE], 10)];
            let deg = (self.v[p::DEGREE].round() as i32).clamp(1, 7) - 1;
            let len = sc.len() as i32;
            let semi = sc[deg.rem_euclid(len) as usize] + 12 * deg.div_euclid(len);
            ((48 + key + semi) as f64, (key + semi).rem_euclid(12) as u8)
        };
        let (iv, nv) = self.intervals(pc);
        let vc = one_pole_coef(0.005 * sr as f64 / n);
        let detune = self.detune.value() as f64;
        let gscale = 1.0 / (nv as f64).sqrt();
        let jump = self.jump;
        let drone = !tracked;
        let srd = sr as f64;
        for (i, vo) in self.voices.iter_mut().enumerate() {
            let target = root + iv[i] + oct;
            vo.semi = if drone && !jump { target + (vo.semi - target) * vc } else { target };
            vo.target = if i < nv { gscale } else { 0.0 };
            let hz = hz_of(vo.semi).min(0.45 * srd);
            vo.ss.set_freq(hz, detune, srd);
        }
        self.jump = false;
        // cores
        let formant = self.formant.value().clamp(-12.0, 12.0);
        let tilt = self.tilt.value().clamp(-6.0, 6.0);
        let cbar_c = one_pole_coef(CBAR_MS as f64 * 0.001 * sr as f64 / n);
        let ncar = if self.split { 2 } else { 1 };
        let act = self.act;
        let running = self.fade.is_running();
        for c in 0..2 {
            if c == act || running {
                self.cores[c].tick(ncar, formant, tilt, cbar_c);
            }
        }
        self.f0_meter = if self.src_cur == TRACKED && !self.pending { self.f_det } else { 0.0 };
    }

    /// The oscillator bank: (mono, left, right).
    #[inline]
    fn oscillators(&mut self) -> (f64, f64, f64) {
        let wf = self.wave_fade.next() as f64;
        let (wc, wp) = (self.wave_cur, self.wave_prev);
        let fading = wf < 1.0;
        let step = 1.0 / (VOICE_FADE_MS as f64 * 0.001 * self.sr as f64);
        let (mut m, mut l, mut r) = (0.0, 0.0, 0.0);
        let sup = wc == SUPERSAW || (fading && wp == SUPERSAW);
        for vo in self.voices.iter_mut() {
            vo.gain = if vo.target > vo.gain { (vo.gain + step).min(vo.target) } else { (vo.gain - step).max(vo.target) };
            if vo.gain <= 0.0 && vo.target <= 0.0 {
                continue;
            }
            let wave = |w: usize, vo: &Voice| -> (f64, f64, f64) {
                let c = &vo.ss.saws[3];
                match w {
                    0 => {
                        let x = saw(c);
                        (x, x, x)
                    }
                    1 => {
                        let x = pulse(c);
                        (x, x, x)
                    }
                    2 => {
                        let x = buzz(c);
                        (x, x, x)
                    }
                    _ => {
                        let (mut e, mut o) = (0.0, 0.0);
                        for (i, s) in vo.ss.saws.iter().enumerate() {
                            if i % 2 == 0 {
                                e += saw(s);
                            } else {
                                o += saw(s);
                            }
                        }
                        ((e + o) * (1.0 / 7f64.sqrt()), e * 0.5, o * (1.0 / 3f64.sqrt()))
                    }
                }
            };
            let mut y = wave(wc, vo);
            if fading {
                let yp = wave(wp, vo);
                y = (yp.0 + (y.0 - yp.0) * wf, yp.1 + (y.1 - yp.1) * wf, yp.2 + (y.2 - yp.2) * wf);
            }
            if sup {
                for s in vo.ss.saws.iter_mut() {
                    s.advance();
                }
            } else {
                vo.ss.saws[3].advance();
            }
            m += vo.gain * y.0;
            l += vo.gain * y.1;
            r += vo.gain * y.2;
        }
        (m, l, r)
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(2);
        let n = buffers.first().map_or(0, |b| b.len());
        if nch == 0 {
            return;
        }
        let v = self.v;
        self.bypass.set_target(if v[p::BYPASS] >= 0.5 { 0.0 } else { 1.0 });
        self.freeze.set_target(if v[p::FREEZE] >= 0.5 { 1.0 } else { 0.0 });
        // the carrier needs two sides: a stereo supersaw, or Self on a stereo main with L ≠ R
        if nch == 2 && !self.split {
            let sup = (self.uses(TRACKED) || self.uses(DRONE)) && (self.wave_cur == SUPERSAW || self.wave_prev == SUPERSAW);
            let selfdiff = self.uses(SELF) && buffers[0].iter().zip(buffers[1].iter()).any(|(a, b)| a != b);
            if sup || selfdiff {
                self.split = true;
                for c in self.cores.iter_mut() {
                    c.split();
                }
            }
        }
        let ncar = if self.split { 2 } else { 1 };
        let reg = self.register();
        let det_i = if reg == 3 { 1 } else { 0 };
        let phrase = (PHRASE_MS as f64 * 0.001 * self.sr as f64) as u64;
        let pend_step = 1.0 / (VOICE_FADE_MS as f64 * 0.001 * self.sr as f64);
        let unv_on = v[p::UNVOICED] > 0.0 || v[p::NOISE] > 0.0;

        for i in 0..n {
            if self.clock.is_multiple_of(self.ctl as u64) {
                self.control();
            }
            self.clock += 1;
            // smoothers
            self.formant.next();
            self.env_rel.next();
            let flatten = (self.flatten.next() as f64 * 0.01).clamp(0.0, 1.0);
            self.tilt.next();
            let sib = self.sib.next() as f64 * 0.01;
            let mix = self.mix.next();
            let noise = self.noise.next() as f64 * 0.01;
            self.env_att.next();
            let thresh = self.thresh.next();
            let unvoiced = self.unvoiced.next() as f64 * 0.01;
            self.detune.next();
            let spread = self.spread.next();
            let out_db = self.output.next();
            self.octave.next();
            let fx = self.freeze.next() as f64;

            let mut x = [0.0f64; 2];
            for c in 0..nch {
                let s = buffers[c][i];
                x[c] = if s.is_finite() { s as f64 } else { 0.0 };
            }
            if nch == 1 {
                x[1] = x[0];
            }
            let m = if nch == 2 { 0.5 * (x[0] + x[1]) } else { x[0] };

            // gate
            let lvl = self.smax.next(m as f32) as f64;
            if thresh != self.th_key {
                self.th_key = thresh;
                self.th_lin = 10f64.powf(thresh as f64 / 20.0);
            }
            let th = self.th_lin;
            if thresh <= -90.0 || (!self.gate_open && lvl > th) {
                self.gate_open = true;
            } else if self.gate_open && lvl < th * 0.501_187_2 {
                self.gate_open = false;
            }
            let gt = if self.gate_open { 1.0 } else { 0.0 };
            self.gg = gt + (self.gg - gt) * if gt > self.gg { self.att } else { self.rel };

            // tracking
            let tr = self.uses(TRACKED);
            if tr {
                let gated = (m * self.gg) as f32;
                if self.det[det_i].push(gated) {
                    self.on_analysis(det_i);
                }
                self.since_voiced = self.since_voiced.saturating_add(1);
                if self.since_voiced > phrase {
                    self.pending = true;
                }
            }
            let w_tr = if self.src_cur == TRACKED {
                if self.source_fade.is_settled() {
                    1.0
                } else {
                    self.source_fade.value() as f64
                }
            } else if self.src_prev == TRACKED && !self.source_fade.is_settled() {
                1.0 - self.source_fade.value() as f64
            } else {
                0.0
            };
            let pt = if self.pending { PENDING_GAIN } else { 1.0 };
            self.car_gain = if pt > self.car_gain { (self.car_gain + pend_step * 0.75).min(pt) } else { (self.car_gain - pend_step * 0.75).max(pt) };

            // carrier
            let sf = self.source_fade.next() as f64;
            let need_osc = self.uses(TRACKED) || self.uses(DRONE);
            let osc = if need_osc { self.oscillators() } else { (0.0, 0.0, 0.0) };
            let white = if self.uses(NOISE) { self.white_src.next_f64() } else { 0.0 };
            let sig = |s: usize, side: usize| -> f64 {
                match s {
                    TRACKED | DRONE => {
                        if ncar == 1 {
                            osc.0
                        } else if side == 0 {
                            osc.1
                        } else {
                            osc.2
                        }
                    }
                    NOISE => white,
                    _ => {
                        if ncar == 1 {
                            m
                        } else {
                            x[side]
                        }
                    }
                }
            };
            let mut car = [0.0; 2];
            for side in 0..ncar {
                let a = sig(self.src_cur, side);
                car[side] = if sf >= 1.0 { a } else { sig(self.src_prev, side) * (1.0 - sf) + a * sf };
            }
            let not_noise = self.src_cur != NOISE;
            // the oscillators' tilt trim at low `flatten`, by their share of the carrier
            let w_osc = {
                let osc_now = matches!(self.src_cur, TRACKED | DRONE);
                let osc_prev = matches!(self.src_prev, TRACKED | DRONE);
                if osc_now == osc_prev {
                    if osc_now { 1.0 } else { 0.0 }
                } else if osc_now {
                    sf
                } else {
                    1.0 - sf
                }
            };
            let wf = self.wave_fade.value() as f64;
            let tdb = (WAVE_TRIM_DB[self.wave_prev] + (WAVE_TRIM_DB[self.wave_cur] - WAVE_TRIM_DB[self.wave_prev]) * wf) * (1.0 - flatten) * w_osc;
            if tdb != self.trim_key {
                self.trim_key = tdb;
                self.trim_lin = 10f64.powf(tdb / 20.0);
            }
            let wave_trim = self.trim_lin;
            let fr = Frame {
                gg: self.gg,
                gate_open: self.gate_open,
                fx,
                att: self.att,
                rel: self.rel,
                unvoiced: if not_noise { unvoiced } else { 0.0 },
                noise: if not_noise { noise } else { 0.0 },
                sib,
                // the pending share follows the carrier's 5 ms level ramp (no step in the noise)
                pending: w_tr * (1.0 - self.car_gain) / (1.0 - PENDING_GAIN),
                car_gain: (1.0 - w_tr * (1.0 - self.car_gain)) * wave_trim,
                nz_on: unv_on && not_noise,
                flatten,
            };
            let wn = if fr.nz_on { self.white_nz.next_f64() } else { 0.0 };
            let k = self.k;
            let g_new = self.fade.next() as f64;
            let (mut ev, mut od, mut sb) = self.cores[self.act].process(m, car, ncar, wn, &fr, &k);
            if g_new < 1.0 {
                let (e2, o2, s2) = self.cores[1 - self.act].process(m, car, ncar, wn, &fr, &k);
                for c in 0..2 {
                    ev[c] = e2[c] + (ev[c] - e2[c]) * g_new;
                    od[c] = o2[c] + (od[c] - o2[c]) * g_new;
                }
                sb = s2 + (sb - s2) * g_new;
            }

            // spread, mix, output, bypass
            let mut wet = [ev[0] + od[0] + sb, ev[1] + od[1] + sb];
            if spread > 0.0 && nch == 2 {
                if spread != self.spread_key {
                    self.spread_key = spread;
                    let q = (spread as f64 / 100.0).clamp(0.0, 1.0);
                    let (a, b) = (FRAC_PI_4 * (1.0 - q), FRAC_PI_4 * (1.0 + q));
                    self.spread_g = [SQRT_2 * a.cos(), SQRT_2 * b.cos(), SQRT_2 * a.sin(), SQRT_2 * b.sin()];
                }
                let g = self.spread_g;
                wet = [g[0] * ev[0] + g[1] * od[0] + sb, g[2] * ev[1] + g[3] * od[1] + sb];
            }
            if mix != self.mix_key {
                self.mix_key = mix;
                let mm = (mix as f64 / 100.0).clamp(0.0, 1.0);
                self.mix_cs = if mm >= 1.0 { (0.0, 1.0) } else { ((FRAC_PI_2 * mm).cos(), (FRAC_PI_2 * mm).sin()) };
            }
            let (cd, cw) = self.mix_cs;
            let og = if out_db == 0.0 { 1.0 } else { 10f64.powf(out_db as f64 / 20.0) };
            let b = self.bypass.next() as f64;
            for c in 0..nch {
                let y = (x[c] * cd + wet[c] * cw) * og;
                let y = if b >= 1.0 { y } else { x[c] + (y - x[c]) * b };
                buffers[c][i] = y as f32;
            }
        }
    }
}

impl Default for Vocoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Vocoder {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate;
        self.nch = channels.clamp(1, 2);
        self.ctl = (sample_rate / TICK_HZ).round().max(1.0) as u32;
        for c in self.cores.iter_mut() {
            c.prepare();
        }
        self.fade.prepare(sample_rate);
        for s in [&mut self.formant, &mut self.env_rel, &mut self.sib, &mut self.mix, &mut self.flatten, &mut self.noise, &mut self.tilt] {
            s.prepare(sample_rate);
        }
        for s in [&mut self.env_att, &mut self.thresh, &mut self.unvoiced, &mut self.detune, &mut self.spread, &mut self.output, &mut self.octave] {
            s.prepare(sample_rate);
        }
        for r in [&mut self.freeze, &mut self.source_fade, &mut self.wave_fade, &mut self.bypass] {
            r.prepare(sample_rate);
        }
        self.smax.prepare((sample_rate / REGISTERS[0].0).ceil() as usize + 2);
        // the voice detector covers the three voice ranges (narrowed by set_range); the instrument
        // detector is gfx-pitch's (harmonic lock)
        self.det[0].prepare_range(sample_rate, REGISTERS[0].0, REGISTERS[2].1, false);
        self.det[1].prepare_range(sample_rate, REGISTERS[3].0, REGISTERS[3].1, true);
        let sr = sample_rate as f64;
        let c = |ms: f64| one_pole_coef(ms * 0.001 * sr);
        self.k = Coefs {
            slow: c(SLOW_MS as f64),
            v_att: c(2.0),
            v_rel: c(10.0),
            u_up: 1.0 / (0.003 * sr),
            u_dn: 1.0 / (0.020 * sr),
            c_att: c(CAR_ATT_MS as f64),
            c_rel: c(CAR_REL_MS as f64),
            tick_step: 1.0 / self.ctl as f64,
        };
        self.reset();
    }

    fn reset(&mut self) {
        let v = self.v;
        let cl = |i: usize| v[i].clamp(PARAMS[i].min, PARAMS[i].max);
        self.formant.settle(cl(p::FORMANT));
        self.env_rel.settle(cl(p::ENV_REL));
        self.sib.settle(cl(p::SIB));
        self.mix.settle(cl(p::MIX));
        self.flatten.settle(cl(p::FLATTEN));
        self.noise.settle(cl(p::NOISE));
        self.tilt.settle(cl(p::TILT));
        for s in [&mut self.formant, &mut self.env_rel, &mut self.sib, &mut self.mix, &mut self.flatten, &mut self.noise, &mut self.tilt] {
            s.reset();
        }
        self.env_att.set_immediate(cl(p::ENV_ATT));
        self.thresh.set_immediate(cl(p::THRESH));
        self.unvoiced.set_immediate(cl(p::UNVOICED));
        self.detune.set_immediate(cl(p::DETUNE));
        self.spread.set_immediate(cl(p::SPREAD));
        self.output.set_immediate(cl(p::OUTPUT));
        self.octave.set_immediate(v[p::OCTAVE].round().clamp(-2.0, 2.0) * 12.0);
        self.freeze.set_immediate(if v[p::FREEZE] >= 0.5 { 1.0 } else { 0.0 });
        self.src_cur = idx(v[p::SOURCE], 4);
        self.src_prev = self.src_cur;
        self.source_fade.set_immediate(1.0);
        self.wave_cur = idx(v[p::WAVE], 4);
        self.wave_prev = self.wave_cur;
        self.wave_fade.set_immediate(1.0);
        self.bypass.set_immediate(if v[p::BYPASS] >= 0.5 { 0.0 } else { 1.0 });
        self.split = false;
        let s = self.setup();
        self.act = 0;
        self.cores[0].design(s, self.sr as f64);
        self.fade.reset();
        self.smax.reset();
        self.gate_open = false;
        self.gg = 0.0;
        let reg = self.register();
        let (lo, hi) = REGISTERS[reg];
        if reg < 3 {
            self.det[0].set_range(lo, hi);
        }
        for d in self.det.iter_mut() {
            d.reset();
        }
        self.f_det = 0.0;
        self.octave_pending = 0;
        self.since_voiced = u64::MAX / 2;
        self.pending = true;
        self.root = 57.0;
        self.root_target = 57.0;
        self.chrom = Quant::default();
        self.keyq = Quant::default();
        self.pc = 9;
        self.jump = false;
        self.car_gain = PENDING_GAIN;
        for vo in self.voices.iter_mut() {
            vo.ss.reset();
            vo.semi = 57.0;
            vo.gain = 0.0;
            vo.target = 0.0;
        }
        self.white_src.reset();
        self.white_nz.reset();
        self.mix_key = f32::NAN;
        self.spread_key = f32::NAN;
        self.th_key = f32::NAN;
        self.trim_key = 0.0;
        self.trim_lin = 1.0;
        self.clock = 0;
        self.f0_meter = 0.0;
        // the voices start at their chord's level (no fade-in from silence after a reset)
        self.control();
        for vo in self.voices.iter_mut() {
            vo.gain = vo.target;
        }
    }

    fn set_param(&mut self, id: &str, value: f32) {
        let Some(i) = PARAMS.iter().position(|pi| pi.id == id) else {
            return;
        };
        let old = self.v[i];
        self.v[i] = value;
        let pi = &PARAMS[i];
        let x = value.clamp(pi.min, pi.max);
        let swap = |cur: &mut usize, prev: &mut usize, fade: &mut LinearRamp, new: usize| {
            if new != *cur {
                *prev = if fade.is_settled() || fade.value() >= 0.5 { *cur } else { *prev };
                *cur = new;
                fade.set_immediate(0.0);
                fade.set_target(1.0);
            }
        };
        let now = self.clock;
        match i {
            p::FORMANT if value != old => self.formant.set_target(x, now),
            p::ENV_REL if value != old => self.env_rel.set_target(x, now),
            p::SIB if value != old => self.sib.set_target(x, now),
            p::MIX if value != old => self.mix.set_target(x, now),
            p::FLATTEN if value != old => self.flatten.set_target(x, now),
            p::NOISE if value != old => self.noise.set_target(x, now),
            p::TILT if value != old => self.tilt.set_target(x, now),
            p::ENV_ATT => self.env_att.set_target(x),
            p::THRESH => self.thresh.set_target(x),
            p::UNVOICED => self.unvoiced.set_target(x),
            p::DETUNE => self.detune.set_target(x),
            p::SPREAD => self.spread.set_target(x),
            p::OUTPUT => self.output.set_target(x),
            p::OCTAVE => self.octave.set_target(x.round() * 12.0),
            p::FREEZE => {
                if value >= 0.5 && old < 0.5 {
                    for c in self.cores.iter_mut() {
                        c.capture();
                    }
                }
            }
            p::SOURCE => swap(&mut self.src_cur, &mut self.src_prev, &mut self.source_fade, idx(value, 4)),
            p::WAVE => swap(&mut self.wave_cur, &mut self.wave_prev, &mut self.wave_fade, idx(value, 4)),
            p::REGISTER => {
                let reg = idx(value, 4);
                if reg < 3 {
                    let (lo, hi) = REGISTERS[reg];
                    self.det[0].set_range(lo, hi);
                }
            }
            _ => {}
        }
    }

    fn meter(&self, id: &str) -> Option<f32> {
        match id {
            "f0" => Some(self.f0_meter),
            "state" => Some(if self.gate_open { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_block(buffers);
    }
}
