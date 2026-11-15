// SPDX-License-Identifier: GPL-3.0-or-later
//! The 25 dirt modules of a slot (README §2.4): their parameters at an effective Amount
//! ([`ModParams`], rate-independent, computed once per base sample and shared by both engines) and
//! their running state at the inner rate ([`ModState`], one per engine, slot and crossfade side).
//!
//! Every module adds exactly half an inner sample of delay: ADAA1 on the nonlinear ones, a two-tap
//! average ([`Half`]) in front of the linear ones and of Off, the area-sampled hold on Sample divide.
//! Fuzz is the one exception (two ADAA stages): its second average is corrected at once by one stage of
//! the engine's droop correction, which restores the magnitude and most of the phase.

use crate::curves::{self, Comp, Curve};
use crate::engine::DROOP_A;
use crate::COMP_TABLES;
use gfx_dsp::svf::{Svf, SvfMode};
use std::f64::consts::{PI, TAU};

/// The module types, in the catalogue's order; Tube and Fuzz appended (enums only grow at the end).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Off,
    Soft,
    Hard,
    Heavy,
    Asym,
    Rectify,
    Fold,
    SineFold,
    ZeroCross,
    BitCrush,
    SampleDivide,
    Dc,
    Width,
    Gain,
    Pan,
    Lowpass,
    Highpass,
    Peak,
    DryWet,
    StereoRectify,
    StereoDc,
    RingMod,
    Cabinet,
    Tube,
    Fuzz,
}

pub const KINDS: [Kind; 25] = [
    Kind::Off,
    Kind::Soft,
    Kind::Hard,
    Kind::Heavy,
    Kind::Asym,
    Kind::Rectify,
    Kind::Fold,
    Kind::SineFold,
    Kind::ZeroCross,
    Kind::BitCrush,
    Kind::SampleDivide,
    Kind::Dc,
    Kind::Width,
    Kind::Gain,
    Kind::Pan,
    Kind::Lowpass,
    Kind::Highpass,
    Kind::Peak,
    Kind::DryWet,
    Kind::StereoRectify,
    Kind::StereoDc,
    Kind::RingMod,
    Kind::Cabinet,
    Kind::Tube,
    Kind::Fuzz,
];

/// Option labels (enum order, final; the catalogue's 23 then Tube, Fuzz).
pub const MODULE_NAMES: [&str; 25] = [
    "Off",
    "Soft clip",
    "Hard clip",
    "Heavy",
    "Asym",
    "Rectify",
    "Fold",
    "Sine fold",
    "Zero cross",
    "Bit crush",
    "Sample divide",
    "DC",
    "Width",
    "Gain",
    "Pan",
    "Lowpass",
    "Highpass",
    "Peak",
    "Dry/wet",
    "Stereo rectify",
    "Stereo DC",
    "Ring mod",
    "Cabinet",
    "Tube",
    "Fuzz",
];

/// Cabinet voicing names, in the order of Prisma's roles (README §2.4).
pub const CABINETS: [&str; 6] = ["4×12 closed", "1×15 bass", "4×12 bright", "8″ small", "2×12 open", "1×12 combo"];

/// A cabinet voicing: a resonant high-pass (low resonance), a presence bell, two low-passes (24 dB /
/// oct roll-off). (hp Hz, hp Q, bell Hz, bell dB, bell Q, lp Hz, lp Q) [by ear: to voice on the bench].
const CAB_VOICINGS: [[f64; 7]; 6] = [
    [75.0, 1.3, 2200.0, 2.0, 1.4, 4200.0, 0.8],
    [42.0, 1.2, 800.0, 2.0, 0.8, 3000.0, 0.7],
    [80.0, 1.1, 3500.0, 4.0, 1.4, 6500.0, 0.8],
    [220.0, 1.0, 900.0, 4.0, 0.9, 4000.0, 0.9],
    [65.0, 0.9, 2800.0, 3.0, 1.2, 5500.0, 0.75],
    [90.0, 1.0, 2200.0, 3.0, 1.3, 5000.0, 0.8],
];

impl Kind {
    pub fn from_index(i: usize) -> Kind {
        KINDS[i.min(KINDS.len() - 1)]
    }

    /// Drive / Focus weight k_type (points of Amount at full Drive, README §2.5); 0 = untouched.
    pub fn weight(self) -> f64 {
        match self {
            Kind::Soft | Kind::Hard | Kind::Fold | Kind::SineFold | Kind::Tube | Kind::Fuzz | Kind::Heavy | Kind::Asym | Kind::Rectify => 50.0,
            Kind::ZeroCross | Kind::BitCrush | Kind::SampleDivide => 20.0,
            _ => 0.0,
        }
    }

    /// The compensation family (level-matched modules).
    pub fn comp(self) -> Option<Comp> {
        Some(match self {
            Kind::Soft => Comp::Soft,
            Kind::Hard => Comp::Hard,
            Kind::Heavy => Comp::Heavy,
            Kind::Asym => Comp::Asym,
            Kind::Rectify | Kind::StereoRectify => Comp::Rectify,
            Kind::Fold => Comp::Fold,
            Kind::SineFold => Comp::SineFold,
            Kind::ZeroCross => Comp::ZeroCross,
            Kind::Tube => Comp::Tube,
            Kind::Fuzz => Comp::Fuzz,
            _ => return None,
        })
    }
}

/// The make-up of a compensated family at Amount `a` (linear), interpolated in its table.
pub fn makeup(c: Comp, a: f64) -> f64 {
    let t = &COMP_TABLES[c as usize];
    let pos = (a.clamp(0.0, 100.0) / 100.0) * (curves::COMP_POINTS - 1) as f64;
    let i = (pos as usize).min(curves::COMP_POINTS - 2);
    let fr = pos - i as f64;
    let db = t[i] as f64 * (1.0 - fr) + t[i + 1] as f64 * fr;
    10f64.powf(db / 20.0)
}

/// The cabinet's voicing index at Amount `a` (Prisma's stepping, round(5 a / 100)).
pub fn cab_index(a: f64) -> usize {
    ((5.0 * a / 100.0).round().max(0.0) as usize).min(5)
}

/// Linear trim that brings a voicing's mean power over 100 Hz – 5 kHz to 0 dB (analog responses).
fn cab_trim(v: &[f64; 7]) -> f64 {
    let [hf, hq, bf, bdb, bq, lf, lq] = *v;
    let a = 10f64.powf(bdb / 40.0);
    let mut sum = 0.0;
    const N: usize = 48;
    for i in 0..N {
        let f = 100.0 * 50f64.powf(i as f64 / (N - 1) as f64);
        let h = |w: f64, q: f64| (1.0 - w * w).powi(2) + (w / q).powi(2);
        let wh = f / hf;
        let hp = wh.powi(4) / h(wh, hq);
        let wb = f / bf;
        let bell = ((1.0 - wb * wb).powi(2) + (wb * a / bq).powi(2)) / ((1.0 - wb * wb).powi(2) + (wb / (bq * a)).powi(2));
        let wl = f / lf;
        let lp = 1.0 / h(wl, lq);
        sum += hp * bell * lp * lp;
    }
    (sum / N as f64).sqrt().recip()
}

/// A module's parameters at its effective Amount (rate-independent).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModParams {
    pub kind: Kind,
    /// The slot (0 or 1): DC acts in slot 1 only.
    pub slot: usize,
    pub curve: Curve,
    /// Make-up (linear), or the cabinet's trim.
    pub makeup: f64,
    /// Generic value: DC offset, width factor, gain, pan position, dry/wet, stereo DC offset.
    pub val: f64,
    /// Hz: Lowpass / Highpass / Peak / Ring mod frequency, Sample divide hold rate.
    pub freq: f64,
    pub peak_db: f64,
    pub cab: usize,
}

impl ModParams {
    pub fn off(slot: usize) -> Self {
        Self { kind: Kind::Off, slot, curve: Curve::Lin, makeup: 1.0, val: 0.0, freq: 1000.0, peak_db: 0.0, cab: 0 }
    }

    /// `a`: the effective Amount (0 – 100, after Drive and Focus); `peak_db`: the slot's Peak gain.
    pub fn new(kind: Kind, a: f64, peak_db: f64, slot: usize) -> Self {
        let mut p = Self::off(slot);
        p.kind = kind;
        let a = a.clamp(0.0, 100.0);
        if let Some(c) = kind.comp() {
            p.makeup = makeup(c, a);
        }
        match kind {
            Kind::Off => {}
            Kind::Soft => p.curve = Curve::Tanh(curves::drive_gain(a)),
            Kind::Hard => p.curve = Curve::Clip(curves::drive_gain(a)),
            Kind::Heavy => p.curve = Curve::Heavy(curves::heavy_exp(a)),
            Kind::Asym => p.curve = Curve::Asym(curves::heavy_exp(a)),
            Kind::Rectify => p.curve = Curve::Rectify(a / 100.0, 1.0),
            Kind::StereoRectify => p.val = a / 100.0,
            Kind::Fold => p.curve = Curve::Fold(curves::drive_gain(a)),
            Kind::SineFold => p.curve = Curve::SineFold(curves::drive_gain(a)),
            Kind::ZeroCross => p.curve = Curve::ZeroCross(10f64.powf(curves::zc_threshold_db(a) / 20.0)),
            Kind::BitCrush => p.curve = Curve::Crush(2f64.powf(1.0 - curves::crush_bits(a))),
            Kind::SampleDivide => p.freq = curves::hold_rate(a),
            Kind::Dc => p.val = a / 100.0 - 0.5,
            Kind::Width => p.val = 2.0 * a / 100.0,
            Kind::Gain => p.val = 10f64.powf(curves::gain_db(a) / 20.0),
            Kind::Pan => p.val = a / 100.0,
            Kind::Lowpass => p.freq = curves::freq_down(a),
            Kind::Highpass | Kind::RingMod => p.freq = curves::freq_up(a),
            Kind::Peak => {
                p.freq = curves::freq_up(a);
                p.peak_db = peak_db;
            }
            Kind::DryWet => p.val = a / 100.0,
            Kind::StereoDc => p.val = a / 100.0,
            Kind::Cabinet => {
                p.cab = cab_index(a);
                p.makeup = cab_trim(&CAB_VOICINGS[p.cab]);
            }
            Kind::Tube => p.curve = Curve::Tube(curves::drive_gain(a)),
            Kind::Fuzz => p.curve = Curve::BiasClip(curves::FUZZ_G1 * curves::drive_gain(a), curves::FUZZ_B1),
        }
        p
    }
}

/// The inner-rate context of an engine.
#[derive(Clone, Copy, Debug)]
pub struct Inner {
    pub fs: f64,
    /// Fuzz's inter-stage high-pass (40 Hz one-pole) coefficient.
    pub fuzz_hp: f64,
}

impl Inner {
    pub fn new(fs: f64) -> Self {
        Self { fs, fuzz_hp: gfx_dsp::util::one_pole_coef(fs / (TAU * 40.0)) }
    }
}

/// The two-tap average: half a sample of delay, ADAA1's small-signal response.
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

/// ADAA1 of a [`Curve`] whose parameters may move: F at the previous input is kept while the curve
/// stays the same (one antiderivative per sample) and recomputed with the new parameters when it
/// changes, so a moving Amount never spikes. Falls back to f at the midpoint on tiny steps.
#[derive(Clone, Copy, Debug)]
pub struct Adaa {
    x1: f64,
    f1: f64,
    last: Curve,
}

impl Default for Adaa {
    fn default() -> Self {
        Self { x1: 0.0, f1: 0.0, last: Curve::Lin }
    }
}

impl Adaa {
    #[inline]
    pub fn process(&mut self, c: &Curve, x: f64) -> f64 {
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

/// One module's running state (both channels) at the inner rate.
#[derive(Clone, Debug)]
pub struct ModState {
    adaa: [[Adaa; 2]; 2],
    half: [Half; 2],
    svf: [[Svf; 4]; 2],
    /// (kind, freq, peak dB, cabinet) the filters were last set for
    svf_key: (Kind, f64, f64, usize),
    hp: [(f64, f64); 2],
    /// Fuzz's correction of its second average: one stage of the engine's droop correction
    fz: [f64; 2],
    /// Ring mod and Sample divide phase (cycles), shared by the channels
    pub phase: f64,
    hold: [f64; 2],
    prev: [f64; 2],
}

impl Default for ModState {
    fn default() -> Self {
        Self {
            adaa: Default::default(),
            half: Default::default(),
            svf: Default::default(),
            svf_key: (Kind::Off, f64::NAN, f64::NAN, usize::MAX),
            hp: [(0.0, 0.0); 2],
            fz: [0.0; 2],
            phase: 0.0,
            hold: [0.0; 2],
            prev: [0.0; 2],
        }
    }
}

impl ModState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn set_filters(&mut self, p: &ModParams, fs: f32) {
        let key = (p.kind, p.freq, p.peak_db, p.cab);
        if key == self.svf_key {
            return;
        }
        self.svf_key = key;
        for s in self.svf.iter_mut() {
            match p.kind {
                Kind::Lowpass => s[0].set(SvfMode::LowPass, p.freq as f32, 1.2, 0.0, fs),
                Kind::Highpass => s[0].set(SvfMode::HighPass, p.freq as f32, 1.1, 0.0, fs),
                Kind::Peak => s[0].set(SvfMode::Bell, p.freq as f32, 1.5, p.peak_db as f32, fs),
                Kind::Cabinet => {
                    let [hf, hq, bf, bdb, bq, lf, lq] = CAB_VOICINGS[p.cab].map(|v| v as f32);
                    s[0].set(SvfMode::HighPass, hf, hq, 0.0, fs);
                    s[1].set(SvfMode::Bell, bf, bq, bdb, fs);
                    s[2].set(SvfMode::LowPass, lf, lq, 0.0, fs);
                    s[3].set(SvfMode::LowPass, lf, lq, 0.0, fs);
                }
                _ => {}
            }
        }
    }

    /// One inner sample of both channels, in place. `dry`: the band's dry aligned with this slot's
    /// output (for Dry/wet).
    #[inline]
    pub fn tick(&mut self, p: &ModParams, x: &mut [f64; 2], dry: &[f64; 2], nch: usize, ctx: &Inner) {
        let stereo = nch == 2;
        match p.kind {
            Kind::Soft | Kind::Hard | Kind::Heavy | Kind::Asym | Kind::Rectify | Kind::Fold | Kind::SineFold | Kind::ZeroCross | Kind::Tube | Kind::BitCrush => {
                for ch in 0..nch {
                    x[ch] = self.adaa[ch][0].process(&p.curve, x[ch]) * p.makeup;
                }
            }
            Kind::StereoRectify if stereo => {
                for ch in 0..2 {
                    let c = Curve::Rectify(p.val, if ch == 0 { 1.0 } else { -1.0 });
                    x[ch] = self.adaa[ch][0].process(&c, x[ch]) * p.makeup;
                }
            }
            Kind::Fuzz => {
                let c2 = Curve::BiasClip(curves::FUZZ_G2, curves::FUZZ_B2);
                for ch in 0..nch {
                    let y1 = self.adaa[ch][0].process(&p.curve, x[ch]);
                    let (hx, hy) = &mut self.hp[ch];
                    *hy = ctx.fuzz_hp * (*hy + y1 - *hx);
                    *hx = y1;
                    let y2 = self.adaa[ch][1].process(&c2, *hy);
                    // the second stage's extra average, corrected as the engine corrects the
                    // others: (1 + a) / (1 + a z⁻¹) restores its magnitude and takes back most of
                    // its phase (0.09 rad left at 20 kHz, 2×)
                    let c = &mut self.fz[ch];
                    *c = (1.0 + DROOP_A) * y2 - DROOP_A * *c;
                    x[ch] = *c * p.makeup;
                }
            }
            Kind::SampleDivide => {
                let inc = (p.freq / ctx.fs).min(0.999);
                let next = self.phase + inc;
                if next >= 1.0 {
                    // the edge falls at t (0 – 1) into this inner interval: the new level is read
                    // there, and this sample takes the area mean of the old and new levels
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
            Kind::Dc => {
                let off = if p.slot == 0 { p.val } else { 0.0 };
                for ch in 0..nch {
                    x[ch] = self.half[ch].process(x[ch]) + off;
                }
            }
            Kind::StereoDc if stereo => {
                for ch in 0..2 {
                    x[ch] = self.half[ch].process(x[ch]) + p.val * (ch as f64 - 0.5);
                }
            }
            Kind::Width if stereo => {
                let l = self.half[0].process(x[0]);
                let r = self.half[1].process(x[1]);
                let m = 0.5 * (l + r);
                let s = 0.5 * (l - r) * p.val;
                x[0] = m + s;
                x[1] = m - s;
            }
            Kind::Pan if stereo => {
                x[0] = self.half[0].process(x[0]) * (2.0 - 2.0 * p.val).min(1.0);
                x[1] = self.half[1].process(x[1]) * (2.0 * p.val).min(1.0);
            }
            Kind::Gain => {
                for ch in 0..nch {
                    x[ch] = self.half[ch].process(x[ch]) * p.val;
                }
            }
            Kind::DryWet => {
                for ch in 0..nch {
                    x[ch] = self.half[ch].process(x[ch]) * (1.0 - p.val) + dry[ch] * p.val;
                }
            }
            Kind::RingMod => {
                let c = (TAU * self.phase).cos();
                for ch in 0..nch {
                    x[ch] = self.half[ch].process(x[ch]) * c;
                }
                self.phase += p.freq / ctx.fs;
                if self.phase >= 1.0 {
                    self.phase -= 1.0;
                }
            }
            Kind::Lowpass | Kind::Highpass | Kind::Peak => {
                self.set_filters(p, ctx.fs as f32);
                for ch in 0..nch {
                    x[ch] = self.svf[ch][0].process(self.half[ch].process(x[ch]) as f32) as f64;
                }
            }
            Kind::Cabinet => {
                self.set_filters(p, ctx.fs as f32);
                for ch in 0..nch {
                    let s = &mut self.svf[ch];
                    let mut v = self.half[ch].process(x[ch]) as f32;
                    for f in s.iter_mut() {
                        v = f.process(v);
                    }
                    x[ch] = v as f64 * p.makeup;
                }
            }
            // Off, and the stereo modules in mono: inert
            _ => {
                for ch in 0..nch {
                    x[ch] = self.half[ch].process(x[ch]);
                }
            }
        }
    }
}

/// A slot's parameters for one base sample: the module playing, and the one fading out after a type
/// (or cabinet voicing) change.
#[derive(Clone, Copy, Debug)]
pub struct SlotFrame {
    pub cur: ModParams,
    pub old: ModParams,
    /// Weight of `cur` (raised cosine, 0 → 1 over 10 ms); 1 when no fade runs.
    pub fade: f64,
    pub fading: bool,
    /// Bumped on every type change: an engine seeing a new value moves its state to the fading
    /// side and starts the new module from rest.
    pub gen: u32,
}

impl SlotFrame {
    pub fn new(slot: usize) -> Self {
        Self { cur: ModParams::off(slot), old: ModParams::off(slot), fade: 1.0, fading: false, gen: 0 }
    }
}

/// A slot's state in one engine: the playing module and the fading one.
#[derive(Clone, Debug, Default)]
pub struct SlotState {
    pub cur: ModState,
    pub old: ModState,
    gen: u32,
}

impl SlotState {
    pub fn reset(&mut self, gen: u32) {
        self.cur.reset();
        self.old.reset();
        self.gen = gen;
    }

    #[inline]
    pub fn tick(&mut self, f: &SlotFrame, x: &mut [f64; 2], dry: &[f64; 2], nch: usize, ctx: &Inner) {
        if f.gen != self.gen {
            self.gen = f.gen;
            std::mem::swap(&mut self.cur, &mut self.old);
            self.cur.reset();
        }
        if f.fading {
            let mut z = *x;
            self.old.tick(&f.old, &mut z, dry, nch, ctx);
            self.cur.tick(&f.cur, x, dry, nch, ctx);
            for ch in 0..nch {
                x[ch] = f.fade * x[ch] + (1.0 - f.fade) * z[ch];
            }
        } else {
            self.cur.tick(&f.cur, x, dry, nch, ctx);
        }
    }
}

/// The raised-cosine shape of a 0 – 1 ramp.
#[inline]
pub fn raised_cos(r: f64) -> f64 {
    0.5 - 0.5 * (PI * r.clamp(0.0, 1.0)).cos()
}
