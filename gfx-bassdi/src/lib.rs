// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-bassdi — golem's bass preamp / DI (design: README.md).
//!
//! in × `input` → compressor (gfx-comp's engine, colour off; on the dirt or on both paths) → one
//! **engine** per model: the dirt (pre band-gains, 8× ADAA stages with an interstage low-pass, post
//! low-pass, loudness compensation) and the clean through the same all-passes, half-bands and half
//! samples (gfx-dsp `crossover` twins) → linear crossfade `mix` → `bass_keep` (LR4: clean, mono lows)
//! → subsonic high-pass at 0.6 × `low_note` → EQ → cab → `level`.
//!
//! Model changes run both engines (dirt and twins) and crossfade their mixes over 30 ms. Zero latency.

#![allow(clippy::needless_range_loop)]

use gfx_comp::{static_curve, Character, Comp, Settings as CompSettings};
use gfx_dsp::crossover::{Order, Split, Twin};
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::{Effect, ParamInfo};

pub mod engine;
pub mod post;
#[cfg(feature = "plugin")]
pub mod plugin;

use engine::{Ctl, Engine, Model, Voicing};
use post::{eq_targets, Cab, CabModel, Eq};

pub const NP: usize = 23;

pub static PARAMS: [ParamInfo; NP] = [
    // front panel
    ParamInfo::float("gain", "Drive", 0.0, 100.0, 35.0).with_exp(),
    ParamInfo::float("mix", "Blend", 0.0, 100.0, 50.0).with_exp(),
    ParamInfo::float("level", "Level", -24.0, 12.0, 0.0).with_exp(),
    ParamInfo::float("bass", "Bass", -12.0, 12.0, 0.0).with_exp(),
    ParamInfo::float("lo_mid", "Lo Mid", -12.0, 12.0, 0.0).with_exp(),
    ParamInfo::float("hi_mid", "Hi Mid", -12.0, 12.0, 0.0).with_exp(),
    ParamInfo::float("treble", "Treble", -12.0, 12.0, 0.0).with_exp(),
    ParamInfo::enumeration("model", "Model", 3, 0),
    ParamInfo::enumeration("cab", "Cab", 5, 1),
    // expert
    ParamInfo::float("grunt", "Grunt", -12.0, 12.0, 0.0).with_exp(),
    ParamInfo::float("attack", "Attack", -12.0, 12.0, 0.0).with_exp(),
    ParamInfo::float("dirt_level", "Dirt Level", -12.0, 12.0, 0.0),
    ParamInfo::float("lo_mid_freq", "Lo Mid Freq", 100.0, 1000.0, 500.0),
    ParamInfo::float("hi_mid_freq", "Hi Mid Freq", 500.0, 5000.0, 1500.0),
    ParamInfo::float("bass_keep", "Bass Keep", 0.0, 300.0, 0.0),
    ParamInfo::float("low_note", "Lowest Note", 20.0, 100.0, 41.2),
    ParamInfo::float("comp", "Comp", 0.0, 100.0, 0.0),
    ParamInfo::enumeration("comp_char", "Comp Type", 3, 0),
    ParamInfo::enumeration("comp_path", "Comp Path", 2, 0),
    ParamInfo::float("sag", "Sag", 0.0, 100.0, 50.0),
    ParamInfo::float("input", "Input", -12.0, 12.0, 0.0),
    ParamInfo::boolean("cab_lows", "Cab Lows", true),
    ParamInfo::boolean("bypass", "Bypass", false),
];

/// Parameter indices (PARAMS order).
pub mod p {
    pub const GAIN: usize = 0;
    pub const MIX: usize = 1;
    pub const LEVEL: usize = 2;
    pub const BASS: usize = 3;
    pub const LO_MID: usize = 4;
    pub const HI_MID: usize = 5;
    pub const TREBLE: usize = 6;
    pub const MODEL: usize = 7;
    pub const CAB: usize = 8;
    pub const GRUNT: usize = 9;
    pub const ATTACK: usize = 10;
    pub const DIRT_LEVEL: usize = 11;
    pub const LO_MID_FREQ: usize = 12;
    pub const HI_MID_FREQ: usize = 13;
    pub const BASS_KEEP: usize = 14;
    pub const LOW_NOTE: usize = 15;
    pub const COMP: usize = 16;
    pub const COMP_CHAR: usize = 17;
    pub const COMP_PATH: usize = 18;
    pub const SAG: usize = 19;
    pub const INPUT: usize = 20;
    pub const CAB_LOWS: usize = 21;
    pub const BYPASS: usize = 22;
}

/// The EXP-capable parameters (ExpSmoother), in this order.
const EXP_IDS: [usize; 9] = [p::GAIN, p::MIX, p::LEVEL, p::BASS, p::LO_MID, p::HI_MID, p::TREBLE, p::GRUNT, p::ATTACK];

const MAX_CH: usize = 2;
const CTL: usize = 32;
/// Subsonic high-pass at 0.6 × `low_note` (2nd-order Butterworth): the DC blocker too.
pub const SUBSONIC: f64 = 0.6;
/// The one-knob compressor's fixed settings (README §2.4) [A].
pub const COMP_KNEE: f32 = 6.0;
pub const COMP_ATTACK: f32 = 10.0;
pub const COMP_RELEASE: f32 = 120.0;
/// Reference level of the automatic make-up (dBFS, RMS).
pub const COMP_REF: f32 = -18.0;

/// The compressor's settings for `comp` c (0 – 1) and a character (README §2.4).
pub fn comp_settings(c: f32, character: Character) -> CompSettings {
    let thresh = -6.0 - 30.0 * c;
    let ratio = 1.0 + 7.0 * c;
    // automatic make-up: the static curve's reduction at the reference level
    let makeup = COMP_REF - static_curve(COMP_REF, thresh, ratio, COMP_KNEE);
    CompSettings {
        thresh,
        ratio,
        makeup,
        attack: COMP_ATTACK,
        release: COMP_RELEASE,
        character,
        knee: COMP_KNEE,
        auto_release: true,
        sc_hpf: 0.0,
        range: 60.0,
        drive: 0.0,
        mix: 100.0,
        link: 100.0,
        sc_listen: false,
    }
}

pub fn comp_character(i: u32) -> Character {
    match i {
        1 => Character::Opto,
        2 => Character::Clean,
        _ => Character::Fet,
    }
}

/// Loudness compensation (dB) per model over `gain` 0, 3.125, … 100 % (33 points), at `grunt`,
/// `attack`, `input` 0: what brings the dirt to the clean's loudness on a −18 dBFS-RMS bass DI.
/// Computed offline (`tests::print_loudness_tables`, K-weighted, synthetic plucked notes) and stored:
/// it never depends on the signal, so nothing pumps.
#[rustfmt::skip]
pub const LOUDNESS_DB: [[f32; 33]; 3] = [
    // Modern
    [-11.91, -12.65, -13.32, -13.93, -14.47, -14.95, -15.38, -15.76, -16.09, -16.37, -16.62, -16.83, -17.02, -17.18, -17.32, -17.44, -17.55, -17.64, -17.72, -17.79, -17.85, -17.91, -17.96, -18.00, -18.04, -18.07, -18.10, -18.12, -18.14, -18.16, -18.18, -18.19, -18.20],
    // Classic
    [-4.35, -5.24, -6.09, -6.89, -7.66, -8.37, -9.03, -9.64, -10.19, -10.72, -11.24, -11.75, -12.25, -12.72, -13.17, -13.59, -13.97, -14.31, -14.62, -14.89, -15.13, -15.33, -15.51, -15.67, -15.81, -15.93, -16.03, -16.12, -16.20, -16.26, -16.32, -16.38, -16.42],
    // Vintage
    [1.51, 0.61, -0.27, -1.15, -2.03, -2.89, -3.74, -4.58, -5.40, -6.20, -6.99, -7.77, -8.54, -9.28, -10.01, -10.70, -11.36, -11.99, -12.58, -13.12, -13.63, -14.09, -14.51, -14.90, -15.24, -15.55, -15.82, -16.06, -16.28, -16.46, -16.63, -16.77, -16.90],
];

pub fn loudness_db(m: Model, gain: f32) -> f32 {
    let t = &LOUDNESS_DB[m as usize];
    let x = (gain / 100.0).clamp(0.0, 1.0) * 32.0;
    let i = (x as usize).min(31);
    let f = x - i as f32;
    t[i] + (t[i + 1] - t[i]) * f
}

#[derive(Clone, Copy, Debug, Default)]
struct ChanOut {
    clean: f32,
    mix: f32,
}

pub struct BassDi {
    sr: f32,
    nch: usize,
    clock: u64,
    values: [f32; NP],
    exp: [ExpSmoother; 9],
    dirt_level: OnePole,
    ln_lo_mid_freq: OnePole,
    ln_hi_mid_freq: OnePole,
    ln_keep: OnePole,
    ln_low_note: OnePole,
    comp_amt: OnePole,
    sag: OnePole,
    input: OnePole,
    // structure
    engines: [Vec<Engine>; 2],
    active: usize,
    model_fade: LinearRamp,
    fading: bool,
    cabs: [Vec<Cab>; 2],
    cab_active: usize,
    cab_fade: LinearRamp,
    cab_fading: bool,
    cab_lows: LinearRamp,
    path: LinearRamp,
    keep_on: LinearRamp,
    byp: LinearRamp,
    // compressor
    comp: Comp,
    comp_on: bool,
    comp_set: Option<(f32, u32)>,
    gr: f32,
    // the shared chain
    keep_lo: Split,
    keep_hi: Vec<Split>,
    keep_pre_d: Vec<Split>,
    keep_pre_c: Vec<Twin>,
    sub: Vec<Svf>,
    eq: Vec<Eq>,
    eq_dirty: bool,
    cab_note: f64,
    sub_note: f32,
    keep_f: f32,
    keep_idle: bool,
    ctl: [Ctl; 2],
    ctl_key: [[f32; 6]; 2],
    /// Tests only: no loudness compensation (to measure the tables).
    #[doc(hidden)]
    pub raw_loudness: bool,
}

impl BassDi {
    pub fn new() -> Self {
        let values: [f32; NP] = std::array::from_fn(|i| PARAMS[i].default);
        let exp = EXP_IDS.map(|i| ExpSmoother::new(PARAMS[i].min, PARAMS[i].max, PARAMS[i].default));
        let ln = |i: usize| PARAMS[i].default.ln();
        let mut comp = Comp::new();
        comp.set_colour(false);
        comp.set(comp_settings(0.0, Character::Fet));
        let ctl = Ctl { low_db: 0.0, high_db: 0.0, g1: 1.0, sag: 1.0, out: 1.0 };
        Self {
            sr: 48000.0,
            nch: 1,
            clock: 0,
            values,
            exp,
            dirt_level: OnePole::new(20.0, 0.0, 1e-5),
            ln_lo_mid_freq: OnePole::new(30.0, ln(p::LO_MID_FREQ), 1e-6),
            ln_hi_mid_freq: OnePole::new(30.0, ln(p::HI_MID_FREQ), 1e-6),
            ln_keep: OnePole::new(30.0, 100f32.ln(), 1e-6),
            ln_low_note: OnePole::new(100.0, ln(p::LOW_NOTE), 1e-6),
            comp_amt: OnePole::new(20.0, 0.0, 1e-5),
            sag: OnePole::new(30.0, 50.0, 1e-4),
            input: OnePole::new(20.0, 0.0, 1e-5),
            engines: [Vec::new(), Vec::new()],
            active: 0,
            model_fade: LinearRamp::new(30.0, 1.0),
            fading: false,
            cabs: [Vec::new(), Vec::new()],
            cab_active: 0,
            cab_fade: LinearRamp::new(30.0, 1.0),
            cab_fading: false,
            cab_lows: LinearRamp::new(30.0, 1.0),
            path: LinearRamp::new(20.0, 0.0),
            keep_on: LinearRamp::new(20.0, 0.0),
            byp: LinearRamp::new(10.0, 0.0),
            comp,
            comp_on: false,
            comp_set: None,
            gr: 0.0,
            keep_lo: Split::new(Order::Lr4),
            keep_hi: Vec::new(),
            keep_pre_d: Vec::new(),
            keep_pre_c: Vec::new(),
            sub: Vec::new(),
            eq: Vec::new(),
            eq_dirty: true,
            cab_note: f64::NAN,
            sub_note: f32::NAN,
            keep_f: 100.0,
            keep_idle: false,
            ctl: [ctl; 2],
            ctl_key: [[f32::NAN; 6]; 2],
            raw_loudness: false,
        }
    }

    /// The compressor's gain reduction over the last block (dB, ≥ 0): the `gr` meter.
    pub fn gain_reduction_db(&self) -> f32 {
        self.gr
    }

    fn model(&self) -> Model {
        Model::from_index(self.values[p::MODEL] as u32)
    }

    fn cab_model(&self) -> CabModel {
        CabModel::from_index(self.values[p::CAB] as u32)
    }

    fn exp_value(&self, id: usize) -> f32 {
        let k = EXP_IDS.iter().position(|&i| i == id).unwrap();
        self.exp[k].value()
    }

    fn low_note(&self) -> f32 {
        self.ln_low_note.value().exp()
    }

    fn one_poles(&mut self) -> [&mut OnePole; 8] {
        [
            &mut self.dirt_level,
            &mut self.ln_lo_mid_freq,
            &mut self.ln_hi_mid_freq,
            &mut self.ln_keep,
            &mut self.ln_low_note,
            &mut self.comp_amt,
            &mut self.sag,
            &mut self.input,
        ]
    }

    fn ramps(&mut self) -> [&mut LinearRamp; 6] {
        [&mut self.model_fade, &mut self.cab_fade, &mut self.cab_lows, &mut self.path, &mut self.keep_on, &mut self.byp]
    }

    /// The engine controls of the engine in slot `s` (recomputed only when an input moved).
    fn engine_ctl(&mut self, s: usize) {
        let Some(e) = self.engines[s].first() else { return };
        let m = e.model;
        let gain = self.exp_value(p::GAIN);
        let key = [m as usize as f32, gain, self.exp_value(p::GRUNT), self.exp_value(p::ATTACK), self.dirt_level.value(), self.sag.value()];
        if key == self.ctl_key[s] {
            return;
        }
        self.ctl_key[s] = key;
        let v = Voicing::of(m);
        let loud = if self.raw_loudness { 0.0 } else { loudness_db(m, gain) };
        self.ctl[s] = Ctl {
            low_db: v.low_db + key[2] as f64,
            high_db: v.high_db + key[3] as f64,
            g1: 10f32.powf(v.stage1_db(gain as f64) as f32 / 20.0),
            sag: key[5] as f64 / 50.0,
            out: 10f32.powf((loud + key[4]) / 20.0),
        };
    }

    /// Control rate (every CTL samples): compressor, EQ and cab designs, structural changes.
    fn control(&mut self) {
        let sr = self.sr as f64;
        let l = self.low_note();
        // compressor: off (not run, bit-transparent) once `comp` has settled on 0
        let c = self.comp_amt.value();
        let ch = self.values[p::COMP_CHAR] as u32;
        if c == 0.0 && self.comp_amt.is_settled() {
            if self.comp_on {
                self.comp.set(comp_settings(0.0, comp_character(ch)));
                self.comp.reset();
                self.comp_on = false;
                self.comp_set = None;
            }
        } else {
            self.comp_on = true;
            if self.comp_set != Some((c, ch)) {
                self.comp.set(comp_settings(c / 100.0, comp_character(ch)));
                self.comp_set = Some((c, ch));
            }
        }
        // EQ
        if self.eq_dirty || [p::BASS, p::LO_MID, p::HI_MID, p::TREBLE].iter().any(|&i| !self.exp[EXP_IDS.iter().position(|&k| k == i).unwrap()].is_settled()) || !self.ln_lo_mid_freq.is_settled() || !self.ln_hi_mid_freq.is_settled() || !self.ln_low_note.is_settled() {
            let first = self.eq_dirty;
            self.eq_dirty = false;
            let t = eq_targets(
                l as f64,
                self.exp_value(p::BASS) as f64,
                (self.exp_value(p::LO_MID) as f64, self.ln_lo_mid_freq.value().exp() as f64),
                (self.exp_value(p::HI_MID) as f64, self.ln_hi_mid_freq.value().exp() as f64),
                self.exp_value(p::TREBLE) as f64,
            );
            for e in self.eq.iter_mut() {
                e.set(&t, sr, if first { 0 } else { CTL as u32 });
            }
        }
        // cab: a model change crossfades; a moving `low_note` re-designs (Smooth's high-pass follows it)
        let want = self.cab_model();
        if !self.cab_fading && self.cabs[self.cab_active].first().is_some_and(|c| c.model != want) {
            let b = 1 - self.cab_active;
            for c in self.cabs[b].iter_mut() {
                c.set(want, l as f64, sr, 0);
                c.reset();
            }
            self.cab_fade.set_immediate(0.0);
            self.cab_fade.set_target(1.0);
            self.cab_fading = true;
            self.cab_note = l as f64;
        } else if (l as f64) != self.cab_note {
            self.cab_note = l as f64;
            for c in self.cabs.iter_mut().flatten() {
                let m = c.model;
                c.set(m, l as f64, sr, CTL as u32);
            }
        }
        // model: run both engines, crossfade their mixes
        let want = self.model();
        if !self.fading && self.engines[self.active].first().is_some_and(|e| e.model != want) {
            let b = 1 - self.active;
            let lt = self.ln_low_note.target().exp();
            for e in self.engines[b].iter_mut() {
                e.set_model(want, l);
                e.set_low_note(lt);
                e.reset();
            }
            self.ctl_key[b] = [f32::NAN; 6];
            self.model_fade.set_immediate(0.0);
            self.model_fade.set_target(1.0);
            self.fading = true;
        }
    }

    fn set_index(&mut self, i: usize, v: f32) {
        let pi = PARAMS[i];
        let v = v.clamp(pi.min, pi.max);
        let v = match pi.kind {
            gfx_dsp::ParamKind::Enum(_) => v.round(),
            gfx_dsp::ParamKind::Bool => {
                if v >= 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            _ => v,
        };
        self.values[i] = v;
        if let Some(k) = EXP_IDS.iter().position(|&e| e == i) {
            self.exp[k].set_target(v, self.clock);
            return;
        }
        match i {
            p::DIRT_LEVEL => self.dirt_level.set_target(v),
            p::LO_MID_FREQ => self.ln_lo_mid_freq.set_target(v.ln()),
            p::HI_MID_FREQ => self.ln_hi_mid_freq.set_target(v.ln()),
            p::BASS_KEEP => {
                if v >= 1.0 {
                    self.ln_keep.set_target(v.max(10.0).ln());
                    if self.keep_on.value() == 0.0 {
                        // from off: the split starts on its frequency
                        self.ln_keep.settle();
                    }
                }
                self.keep_on.set_target(if v >= 1.0 { 1.0 } else { 0.0 });
            }
            p::LOW_NOTE => {
                self.ln_low_note.set_target(v.ln());
                for e in self.engines.iter_mut().flatten() {
                    e.set_low_note(v);
                }
            }
            p::COMP => self.comp_amt.set_target(v),
            p::COMP_PATH => self.path.set_target(v),
            p::SAG => self.sag.set_target(v),
            p::INPUT => self.input.set_target(v),
            p::CAB_LOWS => self.cab_lows.set_target(v),
            p::BYPASS => self.byp.set_target(v),
            _ => {}
        }
    }
}

impl Default for BassDi {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for BassDi {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate;
        self.nch = channels.clamp(1, MAX_CH);
        let n = self.nch;
        let l = self.ln_low_note.target().exp();
        let m = self.model();
        for slot in self.engines.iter_mut() {
            *slot = (0..n)
                .map(|_| {
                    let mut e = Engine::new(m);
                    e.prepare(sample_rate, l);
                    e
                })
                .collect();
        }
        let cm = self.cab_model();
        for slot in self.cabs.iter_mut() {
            *slot = (0..n)
                .map(|_| {
                    let mut c = Cab::new(cm);
                    c.set(cm, l as f64, sample_rate as f64, 0);
                    c
                })
                .collect();
        }
        self.keep_lo.prepare(sample_rate, 0.0);
        self.keep_hi = (0..n)
            .map(|_| {
                let mut s = Split::new(Order::Lr4);
                s.prepare(sample_rate, 0.0);
                s
            })
            .collect();
        self.keep_pre_d = self.keep_hi.clone();
        self.keep_pre_c = (0..n)
            .map(|_| {
                let mut t = Twin::new(Order::Lr4);
                t.prepare(sample_rate, 0.0);
                t
            })
            .collect();
        self.sub = vec![Svf::new(); n];
        self.eq = vec![Eq::default(); n];
        for e in self.exp.iter_mut() {
            e.prepare(sample_rate);
        }
        for s in self.one_poles() {
            s.prepare(sample_rate);
        }
        for r in self.ramps() {
            r.prepare(sample_rate);
        }
        self.comp.prepare(sample_rate, CTL, n);
        self.reset();
    }

    fn reset(&mut self) {
        self.clock = 0;
        for e in self.exp.iter_mut() {
            e.reset();
        }
        for s in self.one_poles() {
            s.settle();
        }
        for r in self.ramps() {
            r.settle();
        }
        self.model_fade.set_immediate(1.0);
        self.cab_fade.set_immediate(1.0);
        self.fading = false;
        self.cab_fading = false;
        let l = self.low_note();
        let (m, cm) = (self.model(), self.cab_model());
        let sr = self.sr as f64;
        for e in self.engines.iter_mut().flatten() {
            e.set_model(m, l);
            e.reset();
        }
        for c in self.cabs.iter_mut().flatten() {
            c.set(cm, l as f64, sr, 0);
            c.reset();
        }
        self.cab_note = l as f64;
        self.active = 0;
        self.cab_active = 0;
        self.keep_f = self.ln_keep.value().exp();
        self.keep_idle = true;
        self.keep_lo.set_freq_now(self.ln_keep.value().exp());
        self.keep_lo.reset();
        for s in self.keep_hi.iter_mut() {
            s.set_freq_now(self.ln_keep.value().exp());
            s.reset();
        }
        for s in self.sub.iter_mut() {
            s.set(SvfMode::HighPass, (SUBSONIC * l as f64) as f32, std::f32::consts::FRAC_1_SQRT_2, 0.0, self.sr);
            s.reset();
        }
        self.sub_note = l;
        self.eq_dirty = true;
        for e in self.eq.iter_mut() {
            *e = Eq::default();
        }
        // the compressor starts settled on its settings
        let c = self.comp_amt.value();
        let ch = self.values[p::COMP_CHAR] as u32;
        self.comp.set(comp_settings(c / 100.0, comp_character(ch)));
        self.comp.reset();
        self.comp_on = false;
        self.comp_set = None;
        self.gr = 0.0;
        self.ctl_key = [[f32::NAN; 6]; 2];
        self.control();
    }

    fn set_param(&mut self, id: &str, value: f32) {
        if let Some(i) = PARAMS.iter().position(|p| p.id == id) {
            self.set_index(i, value);
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        let n = buffers.first().map_or(0, |b| b.len());
        let nch = buffers.len().min(self.nch);
        let mut gr = 0.0f32;
        let mut i = 0;
        let mut xin = [[0.0f32; CTL]; MAX_CH];
        let mut xc = [[0.0f32; CTL]; MAX_CH];
        while i < n {
            let phase = (self.clock % CTL as u64) as usize;
            if phase == 0 {
                self.control();
            }
            let len = (CTL - phase).min(n - i);
            // input gain, then the compressor on a copy (block-wise; its clock is its own)
            for k in 0..len {
                let g = if self.input.is_settled() && self.input.value() == 0.0 {
                    1.0
                } else {
                    10f32.powf(self.input.next() / 20.0)
                };
                for c in 0..nch {
                    let v = buffers[c][i + k];
                    let x = if g == 1.0 { v } else { v * g };
                    xin[c][k] = x;
                    xc[c][k] = x;
                }
            }
            if self.comp_on {
                let (a, b) = xc.split_at_mut(1);
                if nch == 2 {
                    self.comp.process_block(&mut [&mut a[0][..len], &mut b[0][..len]]);
                } else {
                    self.comp.process_block(&mut [&mut a[0][..len]]);
                }
                gr = gr.max(self.comp.gain_reduction_db());
            }
            for k in 0..len {
                self.sample(buffers, i + k, nch, &xin, &xc, k);
            }
            i += len;
            self.clock += len as u64;
        }
        self.gr = gr;
    }
}

impl BassDi {
    #[inline]
    fn sample(&mut self, buffers: &mut [&mut [f32]], i: usize, nch: usize, xin: &[[f32; CTL]; MAX_CH], xc: &[[f32; CTL]; MAX_CH], k: usize) {
        for e in self.exp.iter_mut() {
            e.next();
        }
        self.dirt_level.next();
        self.ln_lo_mid_freq.next();
        self.ln_hi_mid_freq.next();
        let keep_f = if self.ln_keep.is_settled() { self.keep_f } else { self.ln_keep.next().exp() };
        self.keep_f = keep_f;
        let l = if self.ln_low_note.is_settled() { self.sub_note } else { self.ln_low_note.next().exp() };
        self.comp_amt.next();
        self.sag.next();
        let pw = self.path.next();
        let fade = self.model_fade.next();
        let cfade = self.cab_fade.next();
        let lows = self.cab_lows.next();
        let kw = self.keep_on.next();
        let byp = self.byp.next();
        let m = self.exp_value(p::MIX) / 100.0;
        let level = self.exp_value(p::LEVEL);
        let level = if level == 0.0 { 1.0 } else { 10f32.powf(level / 20.0) };
        self.engine_ctl(self.active);
        if self.fading {
            self.engine_ctl(1 - self.active);
        }
        if l != self.sub_note {
            self.sub_note = l;
            for s in self.sub.iter_mut() {
                s.set(SvfMode::HighPass, (SUBSONIC * l as f64) as f32, std::f32::consts::FRAC_1_SQRT_2, 0.0, self.sr);
            }
        }
        // bass_keep (crossfaded on / off by kw): before the engines the dirt is high-passed (LR4: the
        // lows never reach the clipper) and the clean runs through that split's twin; after them the
        // lows come from the clean, mono, and the highs from the mix (a second LR4 at the same
        // frequency). Both paths carry the same two all-passes: the blend stays coherent
        let keeping = kw > 0.0 || self.values[p::BASS_KEEP] >= 1.0;
        if keeping {
            if self.keep_idle {
                self.keep_idle = false;
                self.keep_lo.set_freq_now(keep_f);
                self.keep_lo.reset();
                for c in 0..self.keep_hi.len() {
                    self.keep_hi[c].set_freq_now(keep_f);
                    self.keep_hi[c].reset();
                    self.keep_pre_d[c].set_freq_now(keep_f);
                    self.keep_pre_d[c].reset();
                    self.keep_pre_c[c].set_freq_now(keep_f);
                    self.keep_pre_c[c].reset();
                }
            } else if keep_f != self.keep_lo.freq() {
                self.keep_lo.set_freq_now(keep_f);
                for c in 0..self.keep_hi.len() {
                    self.keep_hi[c].set_freq_now(keep_f);
                    self.keep_pre_d[c].set_freq_now(keep_f);
                    self.keep_pre_c[c].set_freq_now(keep_f);
                }
            }
        } else {
            self.keep_idle = true;
        }
        // engines: clean and mix per channel
        let mut out = [ChanOut::default(); MAX_CH];
        let (a, b) = (self.active, 1 - self.active);
        for c in 0..nch {
            let x = xin[c][k];
            let mut d_in = xc[c][k];
            let mut c_in = if pw == 0.0 { x } else { x + (d_in - x) * pw };
            if keeping {
                let dh = self.keep_pre_d[c].process(d_in as f64).high as f32;
                let ct = self.keep_pre_c[c].process(c_in as f64) as f32;
                d_in += (dh - d_in) * kw;
                c_in += (ct - c_in) * kw;
            }
            let (ca, da) = self.engines[a][c].process(c_in, d_in, &self.ctl[a]);
            let (mut cl, mut di) = (ca, da);
            if self.fading {
                let (cb, db) = self.engines[b][c].process(c_in, d_in, &self.ctl[b]);
                cl += (cb - ca) * fade;
                di += (db - da) * fade;
            }
            out[c] = ChanOut { clean: cl, mix: cl + (di - cl) * m };
        }
        if self.fading && fade >= 1.0 {
            self.fading = false;
            self.active = b;
            for e in self.engines[a].iter_mut() {
                e.reset();
            }
        }
        if keeping {
            let mono = out[..nch].iter().map(|o| o.clean).sum::<f32>() / nch as f32;
            let lo = self.keep_lo.process(mono as f64).low;
            for c in 0..nch {
                let hi = self.keep_hi[c].process(out[c].mix as f64).high;
                let kept = (lo + hi) as f32;
                out[c].mix += (kept - out[c].mix) * kw;
            }
        }
        let (ca, cb) = (self.cab_active, 1 - self.cab_active);
        for c in 0..nch {
            let mut y = self.sub[c].process(out[c].mix);
            y = self.eq[c].process(y);
            let ya = self.cabs[ca][c].process(y, lows);
            y = if self.cab_fading {
                let yb = self.cabs[cb][c].process(y, lows);
                ya + (yb - ya) * cfade
            } else {
                ya
            };
            y *= level;
            let dry = buffers[c][i];
            buffers[c][i] = if byp == 0.0 {
                y
            } else if byp >= 1.0 {
                dry
            } else {
                y + (dry - y) * byp
            };
        }
        if self.cab_fading && cfade >= 1.0 {
            self.cab_fading = false;
            self.cab_active = cb;
        }
    }
}

#[cfg(test)]
mod tests;
