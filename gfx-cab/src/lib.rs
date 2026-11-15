// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-cab — golem's impulse-response loader (design: README.md).
//!
//! Stands for golem's `cab` block (id `level` kept). One or two IRs blended, zero latency, files
//! reached through slot parameters and a golem-written index (§2.1; plumbing in gfx-dsp `ir_host` and `ir_engine`).
//!
//! - **Engine** (gfx-dsp `convolve`): a 64-tap direct head and non-uniform partitions up to 8,192
//!   whose work is spread over the callbacks; one input FDL per channel shared by A, B and the kernels
//!   crossfading in or out (per level, 30 ms). Class limit 2 s.
//! - **Preparation** (gfx-dsp `ir`, off the audio thread): resample, onset trim and auto-polarity,
//!   length and fade, minimum phase, B aligned to A by correlation, Loudness normalisation at +14 dB.
//! - **Slot 0 of `ir_a`** is the built-in voicing (builtin.rs); slot 0 of `ir_b` is none.
//! - **Mix** (§2.5): g_A = level_a · (1 − blend), g_B = level_b · blend · ±1 (A alone at level_a when
//!   B is none), `delay_b` on the late voice (Lagrange 3; each voice's delay through two 25 ms one-poles, so a read
//!   head never jumps in speed, also across 0), the low and high cuts on the sum,
//!   × `level`, + `dry`; `bass_keep` sends the lows of the dry instead of the cab (LR4); Split on 2 / 2
//!   plays L through A and R through B. `bypass` crossfades wet → dry in 10 ms.
//!
//! Reported latency 0, and it is exact: h[0] multiplies x[n]. Real-time safe, deterministic, the same
//! output at every host block size.

#![allow(clippy::needless_range_loop)]

pub mod builtin;

#[cfg(feature = "plugin")]
mod plugin;

#[cfg(test)]
mod tests;

use gfx_dsp::cuts::Cuts;
use gfx_dsp::ir_engine::{read_frac, IrCore, Want, SEG};
use gfx_dsp::crossover::{Order, Split};
use gfx_dsp::delay::DelayLine;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::ir::{Align, ChannelMap, Normalize, Settings};
use gfx_dsp::ir_host::SlotSource;
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::{Effect, ParamInfo, ParamKind};

pub const NP: usize = 21;

const fn int(id: &'static str, name: &'static str) -> ParamInfo {
    ParamInfo { id, name, min: 0.0, max: 65535.0, default: 0.0, kind: ParamKind::Int, exp: false }
}

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::float("level", "Level", -24.0, 12.0, 0.0).with_exp(),
    ParamInfo::float("blend", "Blend", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("lowcut", "Low Cut", 10.0, 500.0, 10.0),
    ParamInfo::float("highcut", "High Cut", 1000.0, 22000.0, 22000.0).with_exp(),
    ParamInfo::enumeration("align", "Align", 3, 1),
    int("ir_a", "IR A"),
    int("ir_b", "IR B"),
    ParamInfo::float("level_a", "Level A", -24.0, 12.0, 0.0),
    ParamInfo::float("level_b", "Level B", -24.0, 12.0, 0.0),
    ParamInfo::boolean("polarity_b", "Polarity B", false),
    ParamInfo::float("delay_b", "Delay B", -10.0, 10.0, 0.0),
    ParamInfo::float("length", "Length", 5.0, 2000.0, 2000.0),
    ParamInfo::float("fade", "Fade", 0.0, 100.0, 10.0),
    ParamInfo::enumeration("normalize", "Normalize", 3, 2),
    ParamInfo::boolean("min_phase", "Min Phase", false),
    ParamInfo::enumeration("lowcut_slope", "Low Cut Slope", 2, 0),
    ParamInfo::enumeration("highcut_slope", "High Cut Slope", 2, 0),
    ParamInfo::float("dry", "Dry", -60.0, 6.0, -60.0),
    ParamInfo::float("bass_keep", "Bass Keep", 0.0, 300.0, 0.0),
    ParamInfo::enumeration("stereo", "Stereo", 2, 0),
    ParamInfo::boolean("bypass", "Bypass", false),
];

pub mod p {
    pub const LEVEL: usize = 0;
    pub const BLEND: usize = 1;
    pub const LOWCUT: usize = 2;
    pub const HIGHCUT: usize = 3;
    pub const ALIGN: usize = 4;
    pub const IR_A: usize = 5;
    pub const IR_B: usize = 6;
    pub const LEVEL_A: usize = 7;
    pub const LEVEL_B: usize = 8;
    pub const POLARITY_B: usize = 9;
    pub const DELAY_B: usize = 10;
    pub const LENGTH: usize = 11;
    pub const FADE: usize = 12;
    pub const NORMALIZE: usize = 13;
    pub const MIN_PHASE: usize = 14;
    pub const LOWCUT_SLOPE: usize = 15;
    pub const HIGHCUT_SLOPE: usize = 16;
    pub const DRY: usize = 17;
    pub const BASS_KEEP: usize = 18;
    pub const STEREO: usize = 19;
    pub const BYPASS: usize = 20;
}

/// Enum labels (order final; append only).
pub const ALIGNS: [&str; 3] = ["Raw", "Onset", "Correlation"];
pub const NORMALIZES: [&str; 3] = ["Off", "Peak", "Loudness"];
pub const SLOPES: [&str; 2] = ["12 dB", "24 dB"];
pub const STEREOS: [&str; 2] = ["Linked", "Split"];

/// Class limit (s).
pub const MAX_S: f64 = 2.0;
/// `delay_b`'s range (ms).
const DELAY_MAX_MS: f32 = 10.0;
/// The crossover never goes below this (Hz): `bass_keep` 0 bypasses it instead.
const BASS_MIN_HZ: f32 = 5.0;

#[inline]
fn db_gain(db: f32) -> f32 {
    if db == 0.0 {
        1.0
    } else {
        10f32.powf(db / 20.0)
    }
}

/// `dry`: −60 dB = off.
#[inline]
fn dry_gain(db: f32) -> f32 {
    if db <= -60.0 {
        0.0
    } else {
        db_gain(db)
    }
}

pub fn align_of(v: f32) -> Align {
    match v.round() as i32 {
        0 => Align::Raw,
        2 => Align::Correlation,
        _ => Align::Onset,
    }
}

pub fn normalize_of(v: f32) -> Normalize {
    match v.round() as i32 {
        0 => Normalize::Off,
        1 => Normalize::Peak,
        _ => Normalize::Loudness,
    }
}

pub struct Cab {
    v: [f32; NP],
    sr: f32,
    clock: u64,
    core: IrCore,
    level: ExpSmoother,
    blend: ExpSmoother,
    level_a: OnePole,
    level_b: OnePole,
    pol_b: OnePole,
    /// delays of A and B (ms), two one-poles each
    dly_s: [[OnePole; 2]; 2],
    dry: OnePole,
    bass: OnePole,
    bass_on: LinearRamp,
    split: LinearRamp,
    b_on: LinearRamp,
    bypass: LinearRamp,
    cuts: [Cuts; 2],
    xo: [Split; 2],
    dly: [[DelayLine; 2]; 2],
    /// per channel: the conv input and the dry lows of the segment
    conv_in: [Vec<f32>; 2],
    lows: [Vec<f32>; 2],
}

impl Default for Cab {
    fn default() -> Self {
        Self::new()
    }
}

impl Cab {
    pub fn new() -> Self {
        let v: [f32; NP] = std::array::from_fn(|i| PARAMS[i].default);
        let mut c = Self {
            v,
            sr: 48000.0,
            clock: 0,
            core: IrCore::new(2, MAX_S, Some(builtin::render)),
            level: ExpSmoother::new(-24.0, 12.0, 0.0),
            blend: ExpSmoother::new(0.0, 100.0, 0.0),
            level_a: OnePole::new(20.0, 1.0, 1e-6),
            level_b: OnePole::new(20.0, 1.0, 1e-6),
            pol_b: OnePole::new(5.0, 1.0, 1e-6),
            dly_s: std::array::from_fn(|_| std::array::from_fn(|_| OnePole::new(25.0, 0.0, 1e-5))),
            dry: OnePole::new(20.0, 0.0, 1e-7),
            bass: OnePole::new(30.0, 0.0, 1e-3),
            bass_on: LinearRamp::new(30.0, 0.0),
            split: LinearRamp::new(20.0, 0.0),
            b_on: LinearRamp::new(20.0, 0.0),
            bypass: LinearRamp::new(10.0, 0.0),
            cuts: [Cuts::new(10.0, 500.0, 1000.0, 22000.0), Cuts::new(10.0, 500.0, 1000.0, 22000.0)],
            xo: [Split::new(Order::Lr4), Split::new(Order::Lr4)],
            dly: Default::default(),
            conv_in: [vec![0.0; SEG], vec![0.0; SEG]],
            lows: [vec![0.0; SEG], vec![0.0; SEG]],
        };
        c.core.set_want(c.want());
        c
    }

    /// The IR engine (plugin wrapper, tests).
    pub fn core(&mut self) -> &mut IrCore {
        &mut self.core
    }

    pub fn core_ref(&self) -> &IrCore {
        &self.core
    }

    /// The effect's clock (samples since reset).
    pub fn clock(&self) -> u64 {
        self.clock
    }

    /// Meters `loaded_a`, `loaded_b`.
    pub fn loaded(&self, v: usize) -> f32 {
        self.core.loaded(v)
    }

    /// Registers an IR under a slot and prepares it now if a voice asks for it (gfx-bench only).
    pub fn load_ir(&mut self, slot: u32, channels: &[Vec<f32>], rate: f32) {
        self.core.load_ir(slot, channels, rate);
    }

    /// The preparation settings of the parameters.
    pub fn settings(&self) -> Settings {
        Settings {
            align: align_of(self.v[p::ALIGN]),
            normalize: normalize_of(self.v[p::NORMALIZE]),
            min_phase: self.v[p::MIN_PHASE] >= 0.5,
            reverse: false,
            length_s: self.v[p::LENGTH] as f64 * 0.001,
            fade: self.v[p::FADE] as f64 * 0.01,
            max_s: MAX_S,
            band: builtin::BAND,
            ref_db: builtin::REF_DB,
            map: ChannelMap::Cab,
            out_channels: self.core.channels().max(1),
        }
    }

    /// What the parameters ask the engine to play.
    pub fn want(&self) -> Want {
        let a = self.v[p::IR_A].round() as u32;
        let b = self.v[p::IR_B].round() as u32;
        Want {
            voices: [if a == 0 { SlotSource::Builtin } else { SlotSource::Slot(a) }, if b == 0 { SlotSource::None } else { SlotSource::Slot(b) }],
            settings: self.settings(),
        }
    }

    fn segment(&mut self, buffers: &mut [&mut [f32]], start: usize, n: usize) {
        let chans = buffers.len().min(self.core.channels()).min(2);
        let sr = self.sr;
        let t0 = self.clock;
        // smoothers shared by the channels run once per sample: render their values first
        let mut lv = [0.0f32; SEG];
        let mut ga = [0.0f32; SEG];
        let mut gb = [0.0f32; SEG];
        let mut sa = [0.0f32; SEG];
        let mut sb = [0.0f32; SEG];
        let mut sw = [0.0f32; SEG];
        let mut dla = [0.0f32; SEG];
        let mut dlb = [0.0f32; SEG];
        let mut dr = [0.0f32; SEG];
        let mut bk = [0.0f32; SEG];
        let mut bo = [0.0f32; SEG];
        let mut by = [0.0f32; SEG];
        for i in 0..n {
            let l = self.level.next();
            lv[i] = db_gain(l);
            let bl = self.blend.next() * 0.01;
            let on = self.b_on.next();
            let la = self.level_a.next();
            let lb = self.level_b.next();
            let pol = self.pol_b.next();
            let eb = bl * on;
            ga[i] = la * (1.0 - eb);
            gb[i] = lb * eb * pol;
            // Split: both at unity at 50 %
            sa[i] = la * (2.0 * (1.0 - bl)).min(1.0);
            sb[i] = lb * pol * (2.0 * bl).min(1.0) * on;
            sw[i] = self.split.next();
            for (v, out) in [&mut dla, &mut dlb].into_iter().enumerate() {
                let d1 = self.dly_s[v][0].next();
                self.dly_s[v][1].set_target(d1);
                out[i] = self.dly_s[v][1].next();
            }
            dr[i] = self.dry.next();
            bk[i] = self.bass.next();
            bo[i] = self.bass_on.next();
            by[i] = self.bypass.next();
        }
        for ch in 0..chans {
            // input: the lows of the dry (bass_keep) and the conv input
            {
                let x = &buffers[ch][start..start + n];
                let ci = &mut self.conv_in[ch];
                let lo = &mut self.lows[ch];
                if bo[0] == 0.0 && bo[n - 1] == 0.0 && self.bass_on.is_settled() {
                    ci[..n].copy_from_slice(x);
                    lo[..n].iter_mut().for_each(|v| *v = 0.0);
                } else {
                    for i in 0..n {
                        if (t0 + i as u64).is_multiple_of(16) {
                            self.xo[ch].set_freq(bk[i].max(BASS_MIN_HZ));
                        }
                        let b = self.xo[ch].process(x[i] as f64);
                        let k = bo[i];
                        ci[i] = x[i] + k * (b.high as f32 - x[i]);
                        lo[i] = k * b.low as f32;
                    }
                }
            }
            self.core.convolve(ch, &self.conv_in[ch][..n]);
            for i in 0..n {
                let x = buffers[ch][start + i];
                let mut a = self.core.out[ch][0][i];
                let mut b = self.core.out[ch][1][i];
                self.dly[ch][0].push(a);
                self.dly[ch][1].push(b);
                if dla[i] > 0.0 {
                    a = read_frac(&self.dly[ch][0], dla[i] as f64 * sr as f64 / 1000.0);
                }
                if dlb[i] > 0.0 {
                    b = read_frac(&self.dly[ch][1], dlb[i] as f64 * sr as f64 / 1000.0);
                }
                let linked = ga[i] * a + gb[i] * b;
                let wet = if sw[i] == 0.0 {
                    linked
                } else {
                    let split = if ch == 0 { sa[i] * a } else { sb[i] * b };
                    linked + sw[i] * (split - linked)
                };
                let wet = self.cuts[ch].process(wet, t0 + i as u64);
                let y = lv[i] * wet + dr[i] * x + self.lows[ch][i];
                buffers[ch][start + i] = if by[i] == 0.0 {
                    y
                } else if by[i] == 1.0 {
                    x
                } else {
                    y + by[i] * (x - y)
                };
            }
        }
        self.clock += n as u64;
    }

    fn update_switches(&mut self) {
        let b_present = self.core.has_kernel(1);
        self.b_on.set_target(if b_present { 1.0 } else { 0.0 });
        let split = self.v[p::STEREO] >= 0.5 && b_present && self.core.channels() == 2;
        self.split.set_target(if split { 1.0 } else { 0.0 });
    }
}

impl Effect for Cab {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate;
        self.core.prepare(sample_rate, channels.clamp(1, 2));
        self.core.set_want(self.want());
        self.level.prepare(sample_rate);
        self.blend.prepare(sample_rate);
        for o in [&mut self.level_a, &mut self.level_b, &mut self.pol_b, &mut self.dry, &mut self.bass] {
            o.prepare(sample_rate);
        }
        self.dly_s.iter_mut().flatten().for_each(|o| o.prepare(sample_rate));
        for r in [&mut self.bass_on, &mut self.split, &mut self.b_on, &mut self.bypass] {
            r.prepare(sample_rate);
        }
        for c in self.cuts.iter_mut() {
            c.prepare(sample_rate);
        }
        for x in self.xo.iter_mut() {
            x.prepare(sample_rate, 30.0);
        }
        let dmax = (DELAY_MAX_MS * 0.001 * sample_rate).ceil() as usize + 8;
        self.dly.iter_mut().flatten().for_each(|d| d.prepare(dmax));
        self.reset();
    }

    fn reset(&mut self) {
        self.core.reset();
        self.level.reset();
        self.blend.reset();
        for o in [&mut self.level_a, &mut self.level_b, &mut self.pol_b, &mut self.dry, &mut self.bass] {
            o.settle();
        }
        for s in self.dly_s.iter_mut() {
            s[0].settle();
            let v = s[0].value();
            s[1].set_immediate(v);
        }
        self.update_switches();
        for r in [&mut self.bass_on, &mut self.split, &mut self.b_on, &mut self.bypass] {
            r.settle();
        }
        for c in self.cuts.iter_mut() {
            c.reset();
        }
        let bk = self.bass.value();
        for x in self.xo.iter_mut() {
            x.set_freq_now(bk.max(BASS_MIN_HZ));
            x.reset();
        }
        self.dly.iter_mut().flatten().for_each(|d| d.reset());
        self.clock = 0;
    }

    fn set_param(&mut self, id: &str, value: f32) {
        let Some(i) = PARAMS.iter().position(|pi| pi.id == id) else { return };
        let pi = PARAMS[i];
        let value = if pi.kind == ParamKind::Int { value.round().clamp(pi.min, pi.max) } else { value.clamp(pi.min, pi.max) };
        self.v[i] = value;
        let now = self.clock;
        match i {
            p::LEVEL => self.level.set_target(value, now),
            p::BLEND => self.blend.set_target(value, now),
            p::LOWCUT => self.cuts.iter_mut().for_each(|c| c.low.set_freq(value, now)),
            p::HIGHCUT => self.cuts.iter_mut().for_each(|c| c.high.set_freq(value, now)),
            p::LOWCUT_SLOPE => self.cuts.iter_mut().for_each(|c| c.low.set_slope(value)),
            p::HIGHCUT_SLOPE => self.cuts.iter_mut().for_each(|c| c.high.set_slope(value)),
            p::LEVEL_A => self.level_a.set_target(db_gain(value)),
            p::LEVEL_B => self.level_b.set_target(db_gain(value)),
            p::POLARITY_B => self.pol_b.set_target(if value >= 0.5 { -1.0 } else { 1.0 }),
            p::DELAY_B => {
                self.dly_s[0][0].set_target((-value).max(0.0));
                self.dly_s[1][0].set_target(value.max(0.0));
            }
            p::DRY => self.dry.set_target(dry_gain(value)),
            p::BASS_KEEP => {
                self.bass.set_target(value);
                self.bass_on.set_target(if value > 0.0 { 1.0 } else { 0.0 });
            }
            p::BYPASS => self.bypass.set_target(if value >= 0.5 { 1.0 } else { 0.0 }),
            p::STEREO => self.update_switches(),
            p::LENGTH | p::FADE => self.core.set_want(self.want()),
            p::ALIGN | p::IR_A | p::IR_B | p::NORMALIZE | p::MIN_PHASE => {
                self.core.set_want(self.want());
                self.update_switches();
            }
            _ => {}
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.update_switches();
        let n = buffers.first().map_or(0, |b| b.len());
        let mut start = 0;
        while start < n {
            let len = (n - start).min(SEG);
            self.segment(buffers, start, len);
            start += len;
        }
    }
}
