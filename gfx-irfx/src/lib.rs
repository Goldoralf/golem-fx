// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-irfx — golem's stereo IR effect (design: ../gfx-cab/README.md §2.8).
//!
//! Stands for golem's `ir-fx` block (ids `wet`, `dry` kept): springs, echoes and reverb boxes captured
//! as impulse responses. The same engine as gfx-cab (gfx-dsp `convolve`, `ir`, `ir_host`; the slot
//! plumbing, the cuts and the host side are gfx-dsp `ir_engine` and `cuts`), with one IR, a class
//! limit of 20 s and the effect's extras.
//!
//! - **Channels**: a 1-channel file plays on both sides; 2 channels L → ch 1, R → ch 2 (a mono DI on
//!   both inputs gives a true mono-to-stereo spring); a 4-channel file plays channels 1 and 4 (true
//!   stereo deferred). On 1 / 1 the two are averaged.
//! - **Preparation**: no onset trim, no minimum phase, no alignment (an effect's pre-delay and phase
//!   are its sound); `length` / `fade`, `reverse`, Loudness at 0 dB over 60 Hz – 12 kHz, so `wet` is
//!   the effect's level against the dry on a sustained signal.
//! - **Mix**: `predelay` on the kernel input (Lagrange 3, two 25 ms one-poles), the wet's `width`
//!   (mid / side, exact at 100 %), its low and high cuts, × `wet`, + `dry` × input.
//! - **`trails`** on: `bypass` mutes the kernel's input (10 ms) and brings the dry to unity, so the
//!   tail rings out; off: `bypass` crossfades to the input in 10 ms.
//!
//! Reported latency 0. Real-time safe, deterministic, the same output at every host block size.

#![allow(clippy::needless_range_loop)]

#[cfg(feature = "plugin")]
mod plugin;

#[cfg(test)]
mod tests;

use gfx_dsp::cuts::Cuts;
use gfx_dsp::ir_engine::{read_frac, IrCore, Want, SEG};
use gfx_dsp::delay::DelayLine;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::ir::{Align, ChannelMap, Normalize, Settings};
use gfx_dsp::ir_host::SlotSource;
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::{Effect, ParamInfo, ParamKind};

pub const NP: usize = 15;

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::float("wet", "Wet", -60.0, 6.0, -6.0).with_exp(),
    ParamInfo::float("dry", "Dry", -60.0, 6.0, 0.0),
    ParamInfo::float("lowcut", "Low Cut", 10.0, 1000.0, 10.0),
    ParamInfo::float("highcut", "High Cut", 1000.0, 22000.0, 22000.0).with_exp(),
    ParamInfo { id: "ir_a", name: "IR", min: 0.0, max: 65535.0, default: 0.0, kind: ParamKind::Int, exp: false },
    ParamInfo::float("predelay", "Predelay", 0.0, 250.0, 0.0),
    ParamInfo::float("length", "Length", 10.0, 20000.0, 20000.0),
    ParamInfo::float("fade", "Fade", 0.0, 100.0, 10.0),
    ParamInfo::enumeration("normalize", "Normalize", 3, 2),
    ParamInfo::boolean("reverse", "Reverse", false),
    ParamInfo::float("width", "Width", 0.0, 200.0, 100.0),
    ParamInfo::enumeration("lowcut_slope", "Low Cut Slope", 2, 0),
    ParamInfo::enumeration("highcut_slope", "High Cut Slope", 2, 0),
    ParamInfo::boolean("trails", "Trails", true),
    ParamInfo::boolean("bypass", "Bypass", false),
];

pub mod p {
    pub const WET: usize = 0;
    pub const DRY: usize = 1;
    pub const LOWCUT: usize = 2;
    pub const HIGHCUT: usize = 3;
    pub const IR_A: usize = 4;
    pub const PREDELAY: usize = 5;
    pub const LENGTH: usize = 6;
    pub const FADE: usize = 7;
    pub const NORMALIZE: usize = 8;
    pub const REVERSE: usize = 9;
    pub const WIDTH: usize = 10;
    pub const LOWCUT_SLOPE: usize = 11;
    pub const HIGHCUT_SLOPE: usize = 12;
    pub const TRAILS: usize = 13;
    pub const BYPASS: usize = 14;
}

/// Enum labels (order final; append only).
pub const NORMALIZES: [&str; 3] = ["Off", "Peak", "Loudness"];
pub const SLOPES: [&str; 2] = ["12 dB", "24 dB"];

/// Class limit (s), Loudness band (Hz) and reference (dB).
pub const MAX_S: f64 = 20.0;
pub const BAND: (f64, f64) = (60.0, 12000.0);
pub const REF_DB: f64 = 0.0;
const PREDELAY_MAX_MS: f32 = 250.0;

/// dB → gain, −60 = off.
#[inline]
fn level(db: f32) -> f32 {
    if db <= -60.0 {
        0.0
    } else if db == 0.0 {
        1.0
    } else {
        10f32.powf(db / 20.0)
    }
}

/// The bypass crossfade, exact at both ends.
#[inline]
fn mix(y: f32, x: f32, k: f32) -> f32 {
    if k == 0.0 {
        y
    } else if k == 1.0 {
        x
    } else {
        y + k * (x - y)
    }
}

pub struct IrFx {
    v: [f32; NP],
    sr: f32,
    clock: u64,
    core: IrCore,
    wet: ExpSmoother,
    dry: OnePole,
    width: OnePole,
    pre_s: [OnePole; 2],
    /// the kernel's input gate (trails) and the bypass crossfade
    gate: LinearRamp,
    bypass: LinearRamp,
    cuts: [Cuts; 2],
    pre: [DelayLine; 2],
    conv_in: [Vec<f32>; 2],
}

impl Default for IrFx {
    fn default() -> Self {
        Self::new()
    }
}

impl IrFx {
    pub fn new() -> Self {
        let v: [f32; NP] = std::array::from_fn(|i| PARAMS[i].default);
        let mut f = Self {
            v,
            sr: 48000.0,
            clock: 0,
            core: IrCore::new(1, MAX_S, None),
            wet: ExpSmoother::new(-60.0, 6.0, -6.0),
            dry: OnePole::new(20.0, 1.0, 1e-7),
            width: OnePole::new(20.0, 1.0, 1e-6),
            pre_s: [OnePole::new(25.0, 0.0, 1e-5), OnePole::new(25.0, 0.0, 1e-5)],
            gate: LinearRamp::new(10.0, 1.0),
            bypass: LinearRamp::new(10.0, 0.0),
            cuts: [Cuts::new(10.0, 1000.0, 1000.0, 22000.0), Cuts::new(10.0, 1000.0, 1000.0, 22000.0)],
            pre: Default::default(),
            conv_in: [vec![0.0; SEG], vec![0.0; SEG]],
        };
        f.core.set_want(f.want());
        f
    }

    pub fn core(&mut self) -> &mut IrCore {
        &mut self.core
    }

    pub fn core_ref(&self) -> &IrCore {
        &self.core
    }

    pub fn clock(&self) -> u64 {
        self.clock
    }

    /// Meter `loaded_a`.
    pub fn loaded(&self) -> f32 {
        self.core.loaded(0)
    }

    /// Registers an IR under a slot and prepares it now if asked for (gfx-bench only).
    pub fn load_ir(&mut self, slot: u32, channels: &[Vec<f32>], rate: f32) {
        self.core.load_ir(slot, channels, rate);
    }

    pub fn settings(&self) -> Settings {
        Settings {
            align: Align::Raw,
            normalize: match self.v[p::NORMALIZE].round() as i32 {
                0 => Normalize::Off,
                1 => Normalize::Peak,
                _ => Normalize::Loudness,
            },
            min_phase: false,
            reverse: self.v[p::REVERSE] >= 0.5,
            length_s: self.v[p::LENGTH] as f64 * 0.001,
            fade: self.v[p::FADE] as f64 * 0.01,
            max_s: MAX_S,
            band: BAND,
            ref_db: REF_DB,
            map: ChannelMap::Fx,
            out_channels: self.core.channels().max(1),
        }
    }

    pub fn want(&self) -> Want {
        let a = self.v[p::IR_A].round() as u32;
        Want { voices: [if a == 0 { SlotSource::None } else { SlotSource::Slot(a) }, SlotSource::None], settings: self.settings() }
    }

    fn switches(&mut self) {
        let byp = self.v[p::BYPASS] >= 0.5;
        let trails = self.v[p::TRAILS] >= 0.5;
        self.gate.set_target(if byp && trails { 0.0 } else { 1.0 });
        self.bypass.set_target(if byp && !trails { 1.0 } else { 0.0 });
        let d = if byp && trails { 1.0 } else { level(self.v[p::DRY]) };
        self.dry.set_target(d);
    }

    fn segment(&mut self, buffers: &mut [&mut [f32]], start: usize, n: usize) {
        let chans = buffers.len().min(self.core.channels()).min(2);
        let sr = self.sr;
        let t0 = self.clock;
        let mut wg = [0.0f32; SEG];
        let mut dg = [0.0f32; SEG];
        let mut wd = [0.0f32; SEG];
        let mut pd = [0.0f32; SEG];
        let mut gt = [0.0f32; SEG];
        let mut by = [0.0f32; SEG];
        for i in 0..n {
            wg[i] = level(self.wet.next());
            dg[i] = self.dry.next();
            wd[i] = self.width.next();
            let d1 = self.pre_s[0].next();
            self.pre_s[1].set_target(d1);
            pd[i] = self.pre_s[1].next();
            gt[i] = self.gate.next();
            by[i] = self.bypass.next();
        }
        for ch in 0..chans {
            let x = &buffers[ch][start..start + n];
            let ci = &mut self.conv_in[ch];
            for i in 0..n {
                self.pre[ch].push(x[i]);
                let v = if pd[i] > 0.0 { read_frac(&self.pre[ch], pd[i] as f64 * sr as f64 / 1000.0) } else { x[i] };
                ci[i] = if gt[i] == 1.0 { v } else { gt[i] * v };
            }
            self.core.convolve(ch, &self.conv_in[ch][..n]);
        }
        for i in 0..n {
            let (mut l, mut r) = (self.core.out[0][0][i], if chans == 2 { self.core.out[1][0][i] } else { 0.0 });
            if chans == 2 && wd[i] != 1.0 {
                let m = 0.5 * (l + r);
                let s = 0.5 * (l - r) * wd[i];
                l = m + s;
                r = m - s;
            }
            let t = t0 + i as u64;
            let wl = self.cuts[0].process(l, t);
            let xl = buffers[0][start + i];
            let yl = wg[i] * wl + dg[i] * xl;
            buffers[0][start + i] = mix(yl, xl, by[i]);
            if chans == 2 {
                let wr = self.cuts[1].process(r, t);
                let xr = buffers[1][start + i];
                let yr = wg[i] * wr + dg[i] * xr;
                buffers[1][start + i] = mix(yr, xr, by[i]);
            }
        }
        self.clock += n as u64;
    }
}

impl Effect for IrFx {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate;
        self.core.prepare(sample_rate, channels.clamp(1, 2));
        self.core.set_want(self.want());
        self.wet.prepare(sample_rate);
        self.pre_s.iter_mut().for_each(|o| o.prepare(sample_rate));
        for o in [&mut self.dry, &mut self.width] {
            o.prepare(sample_rate);
        }
        self.gate.prepare(sample_rate);
        self.bypass.prepare(sample_rate);
        for c in self.cuts.iter_mut() {
            c.prepare(sample_rate);
        }
        let dmax = (PREDELAY_MAX_MS * 0.001 * sample_rate).ceil() as usize + 8;
        self.pre.iter_mut().for_each(|d| d.prepare(dmax));
        self.reset();
    }

    fn reset(&mut self) {
        self.core.reset();
        self.wet.reset();
        self.switches();
        for o in [&mut self.dry, &mut self.width, &mut self.pre_s[0]] {
            o.settle();
        }
        let d = self.pre_s[0].value();
        self.pre_s[1].set_immediate(d);
        self.gate.settle();
        self.bypass.settle();
        for c in self.cuts.iter_mut() {
            c.reset();
        }
        self.pre.iter_mut().for_each(|d| d.reset());
        self.clock = 0;
    }

    fn set_param(&mut self, id: &str, value: f32) {
        let Some(i) = PARAMS.iter().position(|pi| pi.id == id) else { return };
        let pi = PARAMS[i];
        let value = if pi.kind == ParamKind::Int { value.round().clamp(pi.min, pi.max) } else { value.clamp(pi.min, pi.max) };
        self.v[i] = value;
        let now = self.clock;
        match i {
            p::WET => self.wet.set_target(value, now),
            p::LOWCUT => self.cuts.iter_mut().for_each(|c| c.low.set_freq(value, now)),
            p::HIGHCUT => self.cuts.iter_mut().for_each(|c| c.high.set_freq(value, now)),
            p::LOWCUT_SLOPE => self.cuts.iter_mut().for_each(|c| c.low.set_slope(value)),
            p::HIGHCUT_SLOPE => self.cuts.iter_mut().for_each(|c| c.high.set_slope(value)),
            p::WIDTH => self.width.set_target(value * 0.01),
            p::PREDELAY => self.pre_s[0].set_target(value),
            p::DRY | p::TRAILS | p::BYPASS => self.switches(),
            p::IR_A | p::LENGTH | p::FADE | p::NORMALIZE | p::REVERSE => self.core.set_want(self.want()),
            _ => {}
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        let n = buffers.first().map_or(0, |b| b.len());
        let mut start = 0;
        while start < n {
            let len = (n - start).min(SEG);
            self.segment(buffers, start, len);
            start += len;
        }
    }
}
