// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-emet — golem's high-headroom 808-family overdrive (design: README.md).
//!
//! The 808 identity out = in + clip(in · Zf / Zs), run at 8× with an exactly-integrable soft clipper
//! (ADAA1), whose knee sits at the MOSFET pair's ≈ 2.7 V instead of a Tube Screamer's 0.6 V; then the
//! 808 tone network, simulated from its netlist (gfx-dsp's `circuit`); then the level.

use gfx_dsp::adaa::HalfSample;
use gfx_dsp::circuit::{Circuit, Element, Netlist};
use gfx_dsp::oversample::Oversampler2x;
use gfx_dsp::smooth::OnePole;
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
pub mod plugin;

pub static PARAMS: [ParamInfo; 8] = [
    ParamInfo::float("gain", "Gain", 0.0, 100.0, 35.0).with_exp(),
    ParamInfo::float("tone_stack", "Tone", 0.0, 100.0, 50.0).with_exp(),
    ParamInfo::float("level", "Level", -24.0, 12.0, 0.0).with_exp(),
    ParamInfo::float("headroom", "Headroom", -18.0, 6.0, 0.0),
    ParamInfo::float("asym", "Asymmetry", 0.0, 100.0, 0.0),
    ParamInfo::float("tight", "Tight", 20.0, 1000.0, 20.0),
    ParamInfo::float("input", "Input", -12.0, 12.0, 0.0),
    ParamInfo::float("bass_keep", "Bass Keep", 0.0, 500.0, 0.0),
];

// the clipping stage (clone values)
pub const R_DRIVE: f64 = 1.0e6;
pub const R_FIXED: f64 = 10e3;
pub const R_SHUNT: f64 = 4.7e3;
pub const C_SHUNT: f64 = 47e-9;
pub const C_FB: f64 = 50e-12;
/// The MOSFET pair's knee, in volts (0 dBFS = 1 V at the input).
pub const VC: f64 = 2.7;
pub const OS: usize = 8;
const MAX_CH: usize = 2;
const CTL: u32 = 32;

/// The drive pot's audio taper: the fraction of its resistance at position `t` (0 – 1); 10 % at half.
pub fn a_taper(t: f64) -> f64 {
    (10f64.powf(2.0 * t.clamp(0.0, 1.0)) - 1.0) / 99.0
}

/// The feedback resistance at `gain` (0 – 100 %).
pub fn rf(gain: f64) -> f64 {
    R_DRIVE * a_taper(gain / 100.0) + R_FIXED
}

/// Small-signal analog gain of the clipping stage, 1 + Zf / Zs, at `freq` (no `tight`).
pub fn stage_db(gain: f64, freq: f64) -> f64 {
    let w = 2.0 * std::f64::consts::PI * freq;
    let r = rf(gain);
    // Zf / Zs = jω Cs Rf / ((1 + jω Rf Cf)(1 + jω Rs Cs))
    let (nr, ni) = (0.0, w * C_SHUNT * r);
    let (d1r, d1i) = (1.0, w * r * C_FB);
    let (d2r, d2i) = (1.0, w * R_SHUNT * C_SHUNT);
    let (dr, di) = (d1r * d2r - d1i * d2i, d1r * d2i + d1i * d2r);
    let den = dr * dr + di * di;
    let (hr, hi) = ((nr * dr + ni * di) / den, (ni * dr - nr * di) / den);
    20.0 * (1.0 + hr).hypot(hi).log10()
}

/// The 808 tone network at position `t` (0 – 1; linear stand-in for the W taper).
/// Nodes: 1 in (the clipping stage), 2 X (1 kΩ / 220 nF, op-amp +), 3 wiper, 4 op-amp −, 5 C6 / R9,
/// 6 out. Elements 3 and 4 are the pot's two halves.
pub fn tone_network(t: f64) -> Netlist {
    use Element::*;
    let (rx, ry) = tone_halves(t);
    Netlist {
        nodes: 7,
        elements: vec![
            R(1, 2, 1000.0),
            C(2, 0, 220e-9),
            R(2, 0, 10e3), // the bias resistor to Vref
            R(2, 3, rx),
            R(3, 4, ry),
            C(3, 5, 220e-9),
            R(5, 0, 220.0),
            R(4, 6, 1000.0),
            OpAmp(2, 4, 6),
        ],
        input: 1,
        output: 6,
    }
}

fn tone_halves(t: f64) -> (f64, f64) {
    let t = t.clamp(0.0, 1.0);
    (20e3 * t + 1.0, 20e3 * (1.0 - t) + 1.0)
}

/// First-order bilinear section (prewarped).
#[derive(Clone, Copy, Debug, Default)]
struct First {
    b0: f64,
    b1: f64,
    a1: f64,
    x1: f64,
    y1: f64,
}

impl First {
    fn low_pass(&mut self, f: f64, sr: f64) {
        let k = (std::f64::consts::PI * f.min(0.45 * sr) / sr).tan();
        let d = 1.0 + k;
        self.b0 = k / d;
        self.b1 = k / d;
        self.a1 = (k - 1.0) / d;
    }
    fn high_pass(&mut self, f: f64, sr: f64) {
        let k = (std::f64::consts::PI * f.min(0.45 * sr) / sr).tan();
        let d = 1.0 + k;
        self.b0 = 1.0 / d;
        self.b1 = -1.0 / d;
        self.a1 = (k - 1.0) / d;
    }
    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 - self.a1 * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
    fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

/// The soft clipper f(x) = x / sqrt(1 + (x / V)²), V = `vp` above zero and `vn` below, with its
/// antiderivative (exact, continuous, zero at zero) for ADAA1.
#[derive(Clone, Copy, Debug)]
pub struct Clipper {
    pub vp: f64,
    pub vn: f64,
}

impl Clipper {
    #[inline]
    pub fn f(&self, x: f64) -> f64 {
        let v = if x >= 0.0 { self.vp } else { self.vn };
        x / (1.0 + (x / v) * (x / v)).sqrt()
    }
    #[inline]
    pub fn antiderivative(&self, x: f64) -> f64 {
        let v = if x >= 0.0 { self.vp } else { self.vn };
        v * ((v * v + x * x).sqrt() - v)
    }
}

/// 8× up / down: three cascaded half-band stages.
#[derive(Clone, Debug, Default)]
struct Os8 {
    up: [Oversampler2x; 3],
    down: [Oversampler2x; 3],
}

impl Os8 {
    fn reset(&mut self) {
        for o in self.up.iter_mut().chain(self.down.iter_mut()) {
            o.reset();
        }
    }
    #[inline]
    fn up(&mut self, x: f32) -> [f32; OS] {
        let a = self.up[0].up(x);
        let mut b = [0.0f32; 4];
        for k in 0..2 {
            let u = self.up[1].up(a[k]);
            b[2 * k] = u[0];
            b[2 * k + 1] = u[1];
        }
        let mut c = [0.0f32; OS];
        for k in 0..4 {
            let u = self.up[2].up(b[k]);
            c[2 * k] = u[0];
            c[2 * k + 1] = u[1];
        }
        c
    }
    #[inline]
    fn down(&mut self, c: [f32; OS]) -> f32 {
        let mut b = [0.0f32; 4];
        for k in 0..4 {
            b[k] = self.down[2].down([c[2 * k], c[2 * k + 1]]);
        }
        let mut a = [0.0f32; 2];
        for k in 0..2 {
            a[k] = self.down[1].down([b[2 * k], b[2 * k + 1]]);
        }
        self.down[0].down(a)
    }
}

/// One channel's state.
#[derive(Clone, Debug)]
struct Chan {
    os: Os8,
    os_clean: Os8,
    leg_hp: First,
    leg_tight: First,
    leg_lp: First,
    x1: f64,
    f1: f64,
    dry_half: HalfSample,
    clean_half: HalfSample,
    split_lo: [Svf; 2],
    split_hi: [Svf; 2],
    dc: First,
    tone: Circuit,
}

pub struct Emet {
    sr: f64,
    ch: Vec<Chan>,
    gain: OnePole,
    tone: OnePole,
    level: OnePole,
    headroom: OnePole,
    asym: OnePole,
    tight: OnePole,
    input: OnePole,
    bass_keep: OnePole,
    tone_set: f64,
    split_set: f64,
    clip: Clipper,
    leg_gain: f64,
    in_gain: f64,
    out_gain: f64,
    ctl: u32,
    /// Tests only: skip the tone network (the clipping stage alone).
    #[doc(hidden)]
    pub stage_only: bool,
}

impl Emet {
    pub fn new() -> Self {
        Self {
            sr: 48000.0,
            ch: Vec::new(),
            gain: OnePole::new(30.0, 35.0, 1e-3),
            tone: OnePole::new(30.0, 50.0, 1e-3),
            level: OnePole::new(20.0, 0.0, 1e-4),
            headroom: OnePole::new(30.0, 0.0, 1e-4),
            asym: OnePole::new(30.0, 0.0, 1e-3),
            tight: OnePole::new(30.0, 20.0, 1e-2),
            input: OnePole::new(20.0, 0.0, 1e-4),
            bass_keep: OnePole::new(30.0, 0.0, 1e-2),
            tone_set: f64::NAN,
            split_set: f64::NAN,
            clip: Clipper { vp: VC, vn: VC },
            leg_gain: 1.0,
            in_gain: 1.0,
            out_gain: 1.0,
            ctl: 0,
            stage_only: false,
        }
    }

    fn smoothers(&mut self) -> [&mut OnePole; 8] {
        [
            &mut self.gain,
            &mut self.tone,
            &mut self.level,
            &mut self.headroom,
            &mut self.asym,
            &mut self.tight,
            &mut self.input,
            &mut self.bass_keep,
        ]
    }

    /// Recomputes everything that follows the controls (at control rate).
    fn control(&mut self) {
        let fs8 = self.sr * OS as f64;
        let r = rf(self.gain.value() as f64);
        self.leg_gain = r / R_SHUNT;
        let f_hp = 1.0 / (2.0 * std::f64::consts::PI * R_SHUNT * C_SHUNT);
        let f_lp = 1.0 / (2.0 * std::f64::consts::PI * r * C_FB);
        let f_tight = self.tight.value() as f64;
        let v = VC * 10f64.powf(self.headroom.value() as f64 / 20.0);
        self.clip = Clipper { vp: v, vn: v * (1.0 - 0.5 * self.asym.value() as f64 / 100.0) };
        self.in_gain = 10f64.powf(self.input.value() as f64 / 20.0);
        self.out_gain = 10f64.powf(self.level.value() as f64 / 20.0);
        let t = self.tone.value() as f64 / 100.0;
        let retone = (t - self.tone_set).abs() > 1e-6 || self.tone_set.is_nan();
        let bk = self.bass_keep.value() as f64;
        let resplit = (bk - self.split_set).abs() > 0.05 || self.split_set.is_nan();
        let sr = self.sr as f32;
        for c in &mut self.ch {
            c.leg_hp.high_pass(f_hp, fs8);
            c.leg_lp.low_pass(f_lp, fs8);
            c.leg_tight.high_pass(f_tight, fs8);
            if retone {
                let (rx, ry) = tone_halves(t);
                c.tone.set_resistor(3, rx);
                c.tone.set_resistor(4, ry);
            }
            if resplit && bk > 0.5 {
                for k in 0..2 {
                    c.split_lo[k].set(SvfMode::LowPass, bk as f32, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
                    c.split_hi[k].set(SvfMode::HighPass, bk as f32, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
                }
            }
        }
        if retone {
            self.tone_set = t;
        }
        if resplit {
            self.split_set = bk;
        }
    }

    #[inline]
    fn drive_one(clip: &Clipper, leg_gain: f64, tight_on: bool, c: &mut Chan, hi: f32, lo: Option<f32>) -> f32 {
        let up = c.os.up(hi);
        let mut out = [0.0f32; OS];
        for k in 0..OS {
            let x = up[k] as f64;
            // the gain leg: Zf / Zs, then `tight`
            let leg = c.leg_lp.process(c.leg_hp.process(x) * leg_gain);
            // `tight` at its bottom (20 Hz) is off: bypassed, so it shifts no phase in the lows
            let leg = if tight_on { c.leg_tight.process(leg) } else { leg };
            // ADAA1 of the clipper
            let fx = clip.antiderivative(leg);
            let d = leg - c.x1;
            let y = if d.abs() > 1e-6 * (1.0 + leg.abs()) {
                (fx - c.f1) / d
            } else {
                clip.f(0.5 * (leg + c.x1))
            };
            c.x1 = leg;
            c.f1 = fx;
            // the 808 sum: the input (delayed half a sample like the ADAA) plus the clipped leg
            out[k] = c.dry_half.process(x as f32) + y as f32;
        }
        let mut v = c.os.down(out);
        if let Some(l) = lo {
            // the clean band through the same filters: no comb at the crossover
            let u = c.os_clean.up(l);
            let mut o = [0.0f32; OS];
            for k in 0..OS {
                o[k] = c.clean_half.process(u[k]);
            }
            v += c.os_clean.down(o);
        }
        v
    }
}

impl Default for Emet {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Emet {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate as f64;
        let n = channels.clamp(1, MAX_CH);
        let mut dc = First::default();
        dc.high_pass(5.0, self.sr);
        self.ch = (0..n)
            .map(|_| Chan {
                os: Os8::default(),
                os_clean: Os8::default(),
                leg_hp: First::default(),
                leg_tight: First::default(),
                leg_lp: First::default(),
                x1: 0.0,
                f1: 0.0,
                dry_half: HalfSample::default(),
                clean_half: HalfSample::default(),
                split_lo: [Svf::new(), Svf::new()],
                split_hi: [Svf::new(), Svf::new()],
                dc,
                tone: Circuit::new(tone_network(0.5), self.sr),
            })
            .collect();
        for s in self.smoothers() {
            s.prepare(sample_rate);
        }
        self.tone_set = f64::NAN;
        self.split_set = f64::NAN;
        self.control();
    }

    fn reset(&mut self) {
        for s in self.smoothers() {
            s.settle();
        }
        for c in &mut self.ch {
            c.os.reset();
            c.os_clean.reset();
            c.leg_hp.reset();
            c.leg_tight.reset();
            c.leg_lp.reset();
            c.x1 = 0.0;
            c.f1 = 0.0;
            c.dry_half.reset();
            c.clean_half.reset();
            for k in 0..2 {
                c.split_lo[k].reset();
                c.split_hi[k].reset();
            }
            c.dc.reset();
            c.tone.reset();
        }
        self.tone_set = f64::NAN;
        self.split_set = f64::NAN;
        self.ctl = 0;
        self.control();
    }

    fn set_param(&mut self, id: &str, value: f32) {
        let s = match id {
            "gain" => &mut self.gain,
            "tone_stack" => &mut self.tone,
            "level" => &mut self.level,
            "headroom" => &mut self.headroom,
            "asym" => &mut self.asym,
            "tight" => &mut self.tight,
            "input" => &mut self.input,
            "bass_keep" => &mut self.bass_keep,
            _ => return,
        };
        let p = PARAMS.iter().find(|p| p.id == id).unwrap();
        s.set_target(value.clamp(p.min, p.max));
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        let n = buffers.first().map_or(0, |b| b.len());
        let nch = buffers.len().min(self.ch.len());
        for i in 0..n {
            // every sample while a control moves (no zipper; the tone network is rebuilt), every
            // CTL samples once everything has settled
            let moving = self.smoothers().iter().any(|s| !s.is_settled());
            if moving || self.ctl == 0 {
                self.control();
            }
            self.ctl = (self.ctl + 1) % CTL;
            for s in self.smoothers() {
                s.next();
            }
            let split = self.split_set > 0.5;
            let clip = self.clip;
            let leg_gain = self.leg_gain;
            let tight_on = self.tight.value() > 20.5;
            for c in 0..nch {
                let x = buffers[c][i] * self.in_gain as f32;
                let ch = &mut self.ch[c];
                let (hi, lo) = if split {
                    // Linkwitz-Riley 4 (two Butterworth pairs): the bands sum in phase, flat
                    let l0 = ch.split_lo[0].process(x);
                    let lo = ch.split_lo[1].process(l0);
                    let h0 = ch.split_hi[0].process(x);
                    let hi = ch.split_hi[1].process(h0);
                    (hi, Some(lo))
                } else {
                    (x, None)
                };
                let mut y = Self::drive_one(&clip, leg_gain, tight_on, ch, hi, lo) as f64;
                if !self.stage_only {
                    y = ch.tone.process(y);
                }
                y = ch.dc.process(y) * self.out_gain;
                buffers[c][i] = y as f32;
            }
        }
    }
}

#[cfg(test)]
mod tests;
