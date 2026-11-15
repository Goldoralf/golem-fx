// SPDX-License-Identifier: GPL-3.0-or-later
//! The 15 dirt types (README §2.4): their laws of the band's drive d (dB, 0 – 48), their memoryless
//! curves with antiderivatives (gfx-dsp `shapers`, plus the carbon button and the sagging rail), and
//! one slot's running state at the inner rate. Every type adds half an inner sample of delay (ADAA1,
//! the two-tap average, the area-sampled hold), as gfx-grit's modules.
//!
//! build.rs includes this file (with voice.rs) to measure the vocal make-up and small-signal tables
//! through the same code that runs in the plugin.

#![allow(dead_code, clippy::needless_range_loop)]

use gfx_dsp::shapers::{self, Curve};
use gfx_dsp::svf::{Svf, SvfMode};
use std::f64::consts::{PI, TAU};

/// The types, in the enum's permanent order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    Off,
    Soft,
    Hard,
    Tube,
    Fuzz,
    Rectify,
    Fold,
    SineFold,
    Crush,
    Decimate,
    Sputter,
    Ring,
    Carbon,
    Starve,
    Rasp,
}

pub const TYPES: [Type; 15] = [
    Type::Off,
    Type::Soft,
    Type::Hard,
    Type::Tube,
    Type::Fuzz,
    Type::Rectify,
    Type::Fold,
    Type::SineFold,
    Type::Crush,
    Type::Decimate,
    Type::Sputter,
    Type::Ring,
    Type::Carbon,
    Type::Starve,
    Type::Rasp,
];

/// Option labels (enum order, final; append only).
pub const TYPE_NAMES: [&str; 15] = ["Off", "Soft", "Hard", "Tube", "Fuzz", "Rectify", "Fold", "Sine Fold", "Crush", "Decimate", "Sputter", "Ring", "Carbon", "Starve", "Rasp"];

/// Points of the make-up and small-signal tables: d = 0, 1, … 48 dB.
pub const TABLE_POINTS: usize = 49;
pub const MAX_DRIVE_DB: f64 = 48.0;
/// Make-up limits (dB); Sputter's make-up is capped lower (a gated voice is not lifted back up).
pub const MAKEUP_MIN_DB: f64 = -30.0;
pub const MAKEUP_MAX_DB: f64 = 12.0;
pub const SPUTTER_CAP_DB: f64 = 6.0;

/// The reference vowel's peak scale: a sine at −18 dBFS RMS peaks here (the carbon button's and the
/// rail's unit).
pub const A_REF: f64 = 0.125_892_541_179_416_7 * std::f64::consts::SQRT_2;
/// Carbon: h·p = 0.1 · 10^(d / 40) at A_REF, guarded at 0.6 (gfx-radio §2.2's curve, ADAA here).
pub const CARBON_HP: f64 = 0.1;
pub const CARBON_GUARD: f64 = 0.6;
/// Starve: the rail at rest (−6 dBFS) and how far it sags (90 % at full depth on a loud syllable),
/// over a 20 ms envelope of |x| normalised to the reference vowel's mean |x|.
pub const STARVE_RAIL: f64 = 0.5;
pub const STARVE_SAG: f64 = 0.9;
pub const STARVE_FLOOR: f64 = 0.03;
pub const STARVE_REF: f64 = 0.1;
pub const STARVE_MS: f64 = 20.0;
/// Rasp: noise 2 – 6 kHz at −12 dB re the band's 5 ms RMS, times r = d / 48.
pub const RASP_DB: f64 = -12.0;
pub const RASP_MS: f64 = 5.0;
pub const RASP_LO: f64 = 2000.0;
pub const RASP_HI: f64 = 6000.0;
/// Fuzz's inter-stage high-pass (Hz), gfx-grit's.
pub const FUZZ_HP_HZ: f64 = 40.0;
/// Droop correction C(z) = (1 + a) / (1 + a z⁻¹) after the band sum: one stage, for the one inner
/// average every path carries (the types are parallel; gfx-grit's two slots in series take the square).
pub const DROOP_A: f64 = 0.8;

impl Type {
    pub fn from_index(i: usize) -> Type {
        TYPES[i.min(TYPES.len() - 1)]
    }

    /// Whether the type carries a vocal make-up table (Crush, Decimate, Ring and Off are unity).
    pub fn compensated(self) -> bool {
        !matches!(self, Type::Off | Type::Crush | Type::Decimate | Type::Ring)
    }

    /// Types with memory: in a blend they comb or beat (README §2.4); bench bound ±1.5 dB.
    pub fn has_memory(self) -> bool {
        matches!(self, Type::Fuzz | Type::Decimate | Type::Starve | Type::Ring)
    }

    /// The cost class: Fuzz runs two ADAA stages per inner sample.
    pub fn stages(self) -> usize {
        match self {
            Type::Off => 0,
            Type::Fuzz => 2,
            _ => 1,
        }
    }
}

/// Crush depth (bits): 16 → 4 over the drive (the README's 16 → 2 gates a −18 dBFS voice to silence
/// from 3 bits down, and aliases at +32 dB re the harmonics there: shortened as Fold's law was).
#[inline]
pub fn crush_bits(d: f64) -> f64 {
    16.0 - 12.0 * d / MAX_DRIVE_DB
}

/// Fuzz's drive into its first stage (dB): 0.7 d, 0 – 33.6 dB (at 48 dB it aliases at −38 dB on the
/// reference vowel, −45.0 dB at 36 dB: shortened as Fold's law was).
#[inline]
pub fn fuzz_db(d: f64) -> f64 {
    0.7 * d
}

/// Decimate hold rate (Hz): 20 kHz → 312.5 Hz over the drive.
#[inline]
pub fn hold_rate(d: f64) -> f64 {
    20000.0 * 2f64.powf(-6.0 * d / MAX_DRIVE_DB)
}

/// Sputter dead zone (dBFS): −66 → −6.
#[inline]
pub fn sputter_db(d: f64) -> f64 {
    -66.0 + 1.25 * d
}

/// A memoryless law with a closed-form antiderivative.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Law {
    Lin,
    Curve(Curve),
    /// The carbon button 1 / (1 − u) − 1 with u = h x clamped to ±0.6, divided by h (unity slope at 0).
    Carbon(f64),
    /// A hard rail at ±c.
    Rail(f64),
}

#[inline]
fn carbon_f(u: f64) -> f64 {
    let u = u.clamp(-CARBON_GUARD, CARBON_GUARD);
    1.0 / (1.0 - u) - 1.0
}

#[inline]
fn carbon_af(u: f64) -> f64 {
    // ∫ 1 / (1 − u) − 1 du = −ln(1 − u) − u inside the guard, then linear
    let g = CARBON_GUARD;
    if u > g {
        -(1.0 - g).ln() - g + carbon_f(g) * (u - g)
    } else if u < -g {
        -(1.0 + g).ln() + g + carbon_f(-g) * (u + g)
    } else {
        -(1.0 - u).ln() - u
    }
}

impl Law {
    #[inline]
    pub fn f(&self, x: f64) -> f64 {
        match *self {
            Law::Lin => x,
            Law::Curve(c) => c.f(x),
            Law::Carbon(h) => carbon_f(h * x) / h,
            Law::Rail(c) => x.clamp(-c, c),
        }
    }

    #[inline]
    pub fn af(&self, x: f64) -> f64 {
        match *self {
            Law::Lin => 0.5 * x * x,
            Law::Curve(c) => c.af(x),
            Law::Carbon(h) => carbon_af(h * x) / (h * h),
            Law::Rail(c) => {
                let a = x.abs();
                if a <= c {
                    0.5 * x * x
                } else {
                    c * a - 0.5 * c * c
                }
            }
        }
    }
}

/// ADAA1 of a [`Law`] whose parameters may move (gfx-grit's rule): F at the previous input is kept
/// while the law stays the same, recomputed when it changes, so a moving drive never spikes.
#[derive(Clone, Copy, Debug)]
pub struct Adaa {
    x1: f64,
    f1: f64,
    last: Law,
}

impl Default for Adaa {
    fn default() -> Self {
        Self { x1: 0.0, f1: 0.0, last: Law::Lin }
    }
}

impl Adaa {
    #[inline]
    pub fn process(&mut self, c: &Law, x: f64) -> f64 {
        if *c != self.last {
            self.last = *c;
            self.f1 = c.af(self.x1);
        }
        let fx = c.af(x);
        let d = x - self.x1;
        let y = if d.abs() < 1e-5 * (1.0 + x.abs()) { c.f(0.5 * (x + self.x1)) } else { (fx - self.f1) / d };
        self.x1 = x;
        self.f1 = fx;
        y
    }
}

/// The two-tap average: half a sample of delay (ADAA1's small-signal response).
#[derive(Clone, Copy, Debug, Default)]
pub struct Half {
    x1: f64,
}

impl Half {
    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let y = 0.5 * (x + self.x1);
        self.x1 = x;
        y
    }
}

/// The inner-rate context (rate-dependent constants).
#[derive(Clone, Copy, Debug)]
pub struct Inner {
    pub fs: f64,
    pub fuzz_hp: f64,
    pub starve_c: f64,
    pub rasp_c: f64,
    /// Rasp's band-pass noise normalisation (unit RMS)
    pub rasp_norm: f64,
}

impl Inner {
    pub fn new(fs: f64) -> Self {
        let bw = RASP_HI - RASP_LO;
        Self {
            fs,
            fuzz_hp: gfx_dsp::util::one_pole_coef(fs / (TAU * FUZZ_HP_HZ)),
            starve_c: gfx_dsp::util::one_pole_coef(STARVE_MS * 0.001 * fs),
            rasp_c: gfx_dsp::util::one_pole_coef(RASP_MS * 0.001 * fs),
            // a unity-peak band-pass passes π B / 2 of a white spectrum (one-sided, per Hz); the
            // white is uniform (variance 1 / 3) over fs / 2
            rasp_norm: 1.0 / (gfx_dsp::noise::WHITE_RMS * (PI * bw / fs).sqrt()),
        }
    }
}

/// A slot's parameters at one drive (rate-independent except `inc`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotParams {
    pub ty: Type,
    pub law: Law,
    /// Make-up (linear); 1 for the unity types.
    pub makeup: f64,
    /// Ring: carrier Hz; Decimate: hold rate Hz.
    pub freq: f64,
    /// r = d / 48: Ring's depth, Starve's sag depth, Rasp's noise share.
    pub depth: f64,
}

impl SlotParams {
    pub fn off() -> Self {
        Self { ty: Type::Off, law: Law::Lin, makeup: 1.0, freq: 0.0, depth: 0.0 }
    }

    /// The type `ty` at drive `d` (dB), with the make-up `makeup` (linear) and Ring's carrier.
    pub fn new(ty: Type, d: f64, makeup: f64, ring_hz: f64) -> Self {
        let d = d.clamp(0.0, MAX_DRIVE_DB);
        let g = 10f64.powf(d / 20.0);
        let r = d / MAX_DRIVE_DB;
        let mut p = Self { ty, law: Law::Lin, makeup: if ty.compensated() { makeup } else { 1.0 }, freq: 0.0, depth: r };
        match ty {
            Type::Off => {}
            Type::Soft => p.law = Law::Curve(Curve::Tanh(g)),
            Type::Hard => p.law = Law::Curve(Curve::Clip(g)),
            Type::Tube | Type::Rasp => p.law = Law::Curve(Curve::Tube(g)),
            Type::Fuzz => p.law = Law::Curve(Curve::BiasClip(shapers::FUZZ_G1 * 10f64.powf(fuzz_db(d) / 20.0), shapers::FUZZ_B1)),
            Type::Rectify => p.law = Law::Curve(Curve::Rectify(r, 1.0)),
            Type::Fold => p.law = Law::Curve(Curve::Fold(10f64.powf(d / 40.0))),
            Type::SineFold => p.law = Law::Curve(Curve::SineFold(10f64.powf(d / 40.0))),
            Type::Crush => p.law = Law::Curve(Curve::Crush(2f64.powf(1.0 - crush_bits(d)))),
            Type::Decimate => p.freq = hold_rate(d),
            Type::Sputter => p.law = Law::Curve(Curve::ZeroCross(10f64.powf(sputter_db(d) / 20.0))),
            Type::Ring => p.freq = ring_hz,
            Type::Carbon => p.law = Law::Carbon(CARBON_HP * 10f64.powf(d / 40.0) / A_REF),
            Type::Starve => p.law = Law::Rail(STARVE_RAIL),
        }
        p
    }
}

/// One slot's running state (both channels) at the inner rate.
#[derive(Clone, Debug)]
pub struct SlotState {
    adaa: [[Adaa; 2]; 2],
    half: [Half; 2],
    hp: [(f64, f64); 2],
    fz: [f64; 2],
    /// Ring and Decimate phase (cycles), shared by the channels
    pub phase: f64,
    hold: [f64; 2],
    prev: [f64; 2],
    /// Starve's |x| envelope and Rasp's x² envelope, linked (both channels)
    pub env: f64,
    noise: gfx_dsp::noise::White,
    noise_bp: Svf,
    noise_fs: f64,
}

/// Rasp's seed (the same on both channels: no fake width).
const RASP_SEED: u64 = 0x5EED_0A5B;

impl Default for SlotState {
    fn default() -> Self {
        Self {
            adaa: Default::default(),
            half: Default::default(),
            hp: [(0.0, 0.0); 2],
            fz: [0.0; 2],
            phase: 0.0,
            hold: [0.0; 2],
            prev: [0.0; 2],
            env: 0.0,
            noise: gfx_dsp::noise::White::new(RASP_SEED),
            noise_bp: Svf::new(),
            noise_fs: 0.0,
        }
    }
}

impl SlotState {
    pub fn reset(&mut self) {
        let fs = self.noise_fs;
        *self = Self::default();
        self.noise_fs = fs;
        if fs > 0.0 {
            self.set_noise(fs);
        }
    }

    fn set_noise(&mut self, fs: f64) {
        let fc = (RASP_LO * RASP_HI).sqrt();
        self.noise_bp.set(SvfMode::BandPass, fc as f32, (fc / (RASP_HI - RASP_LO)) as f32, 0.0, fs as f32);
        self.noise_fs = fs;
    }

    /// Clears the filter states and the ADAA history, keeping the cheap running states (Ring's and
    /// Decimate's phase, Starve's and Rasp's envelope, the noise): a slot that wakes from a zero weight.
    pub fn wake(&mut self) {
        self.adaa = Default::default();
        self.half = Default::default();
        self.hp = [(0.0, 0.0); 2];
        self.fz = [0.0; 2];
        self.hold = [0.0; 2];
        self.prev = [0.0; 2];
    }

    /// The cheap states of a slot that does not run (weight 0): phases and envelopes move on.
    #[inline]
    pub fn idle(&mut self, p: &SlotParams, x: &[f64; 2], nch: usize, ctx: &Inner) {
        match p.ty {
            Type::Ring => {
                self.phase += p.freq / ctx.fs;
                if self.phase >= 1.0 {
                    self.phase -= 1.0;
                }
            }
            Type::Decimate => {
                let next = self.phase + (p.freq / ctx.fs).min(0.999);
                self.phase = if next >= 1.0 { next - 1.0 } else { next };
            }
            Type::Starve => {
                let a = (0..nch).map(|c| x[c].abs()).sum::<f64>() / nch as f64;
                self.env = a + (self.env - a) * ctx.starve_c;
            }
            Type::Rasp => {
                let a = (0..nch).map(|c| x[c] * x[c]).sum::<f64>() / nch as f64;
                self.env = a + (self.env - a) * ctx.rasp_c;
            }
            _ => {}
        }
    }

    /// One inner sample of both channels, in place; the output carries the make-up.
    #[inline]
    pub fn tick(&mut self, p: &SlotParams, x: &mut [f64; 2], nch: usize, ctx: &Inner) {
        match p.ty {
            Type::Soft | Type::Hard | Type::Tube | Type::Rectify | Type::Fold | Type::SineFold | Type::Crush | Type::Sputter | Type::Carbon => {
                for ch in 0..nch {
                    x[ch] = self.adaa[ch][0].process(&p.law, x[ch]) * p.makeup;
                }
            }
            Type::Fuzz => {
                let c2 = Law::Curve(Curve::BiasClip(shapers::FUZZ_G2, shapers::FUZZ_B2));
                for ch in 0..nch {
                    let y1 = self.adaa[ch][0].process(&p.law, x[ch]);
                    let (hx, hy) = &mut self.hp[ch];
                    *hy = ctx.fuzz_hp * (*hy + y1 - *hx);
                    *hx = y1;
                    let y2 = self.adaa[ch][1].process(&c2, *hy);
                    // the second stage's extra average, corrected as in gfx-grit
                    let c = &mut self.fz[ch];
                    *c = (1.0 + DROOP_A) * y2 - DROOP_A * *c;
                    x[ch] = *c * p.makeup;
                }
            }
            Type::Starve => {
                let a = (0..nch).map(|c| x[c].abs()).sum::<f64>() / nch as f64;
                self.env = a + (self.env - a) * ctx.starve_c;
                let sag = (self.env / STARVE_REF).min(1.0);
                let c = (STARVE_RAIL * (1.0 - STARVE_SAG * p.depth * sag)).max(STARVE_FLOOR * STARVE_RAIL);
                let law = Law::Rail(c);
                for ch in 0..nch {
                    x[ch] = self.adaa[ch][0].process(&law, x[ch]) * p.makeup;
                }
            }
            Type::Rasp => {
                let a = (0..nch).map(|c| x[c] * x[c]).sum::<f64>() / nch as f64;
                self.env = a + (self.env - a) * ctx.rasp_c;
                if self.noise_fs != ctx.fs {
                    self.set_noise(ctx.fs);
                }
                let w = self.noise.next();
                let n = self.noise_bp.process(w) as f64 * ctx.rasp_norm;
                let k = n * self.env.sqrt() * p.depth * 10f64.powf(RASP_DB / 20.0);
                for ch in 0..nch {
                    x[ch] = self.adaa[ch][0].process(&p.law, x[ch]) * p.makeup + k;
                }
            }
            Type::Decimate => {
                let inc = (p.freq / ctx.fs).min(0.999);
                let next = self.phase + inc;
                if next >= 1.0 {
                    // the edge falls at t into this inner interval: the new level is read there, and
                    // this sample takes the area mean of the old and new levels (gfx-grit's engine)
                    let t = ((1.0 - self.phase) / inc).clamp(0.0, 1.0);
                    for ch in 0..nch {
                        let new = self.prev[ch] + t * (x[ch] - self.prev[ch]);
                        self.prev[ch] = x[ch];
                        x[ch] = t * self.hold[ch] + (1.0 - t) * new;
                        self.hold[ch] = new;
                    }
                    self.phase = next - 1.0;
                } else {
                    self.prev[..nch].copy_from_slice(&x[..nch]);
                    x[..nch].copy_from_slice(&self.hold[..nch]);
                    self.phase = next;
                }
            }
            Type::Ring => {
                let c = (TAU * self.phase).cos();
                let r = p.depth;
                for ch in 0..nch {
                    let h = self.half[ch].process(x[ch]);
                    x[ch] = h * (1.0 - r + r * c);
                }
                self.phase += p.freq / ctx.fs;
                if self.phase >= 1.0 {
                    self.phase -= 1.0;
                }
            }
            Type::Off => {
                for ch in 0..nch {
                    x[ch] = self.half[ch].process(x[ch]);
                }
            }
        }
    }
}

/// One type alone through the dirt's skeleton at 2× (up, the type, the 5 Hz DC block, the droop
/// correction, down), from rest, mono: the path the build-time tables measure. f32 audio as in the
/// plugin.
pub fn render_type(p: &SlotParams, x: &[f64], base_fs: f64) -> Vec<f64> {
    use gfx_dsp::oversample::Oversampler2x;
    let fs = 2.0 * base_fs;
    let ctx = Inner::new(fs);
    let mut os = Oversampler2x::new();
    let mut st = SlotState::default();
    let mut dc = Svf::new();
    dc.set(SvfMode::HighPass, 5.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, fs as f32);
    let mut droop = 0.0f64;
    let g = 1.0 + DROOP_A;
    x.iter()
        .map(|&v| {
            let u = os.up(v as f32);
            let mut o = [0.0f32; 2];
            for k in 0..2 {
                let mut s = [u[k] as f64, 0.0];
                st.tick(p, &mut s, 1, &ctx);
                let w = dc.process(s[0] as f32) as f64;
                droop = g * w - DROOP_A * droop;
                o[k] = droop as f32;
            }
            os.down(o) as f64
        })
        .collect()
}

/// RMS (dB) of `x[from..]`.
pub fn rms_db(x: &[f64], from: usize) -> f64 {
    let s = &x[from.min(x.len())..];
    10.0 * (s.iter().map(|v| v * v).sum::<f64>() / s.len().max(1) as f64).max(1e-30).log10()
}
