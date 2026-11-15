// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-glitch — golem's spectral pedal for guitar textures and pads (design: README.md).
//!
//! - **Engine** (engine.rs): a symmetric STFT of 512 – 32,768 points, hop / 4, its frame work
//!   scheduled on sample counts (block-size independent, no CPU spike); the wet is late by size + hop.
//!   A size or window change starts a second engine and crossfades once it speaks.
//! - **Bins**: sustain (max-hold with a T60 and `damp`), the frequency map (pitch, spread, Move slots:
//!   peak regions moved as rigid units), cloud (random phases), three effect slots on frequency regions
//!   (slots.rs, 29 effects), density (a compander per bin).
//! - **Wet** (time domain): drift (a single-sideband shifter), low cut, tone, wet level, a soft limiter.
//! - **Dry** at unity and never delayed unless `align` (then the latency is reported).
//! - **Modulation**: an LFO and an envelope, one destination each.
//!
//! No footswitch: the pedal is set before playing (the maintainer, 2026-11-14). Real-time safe, deterministic.

#![allow(clippy::needless_range_loop)]

pub mod engine;
pub mod slots;

#[cfg(feature = "plugin")]
mod plugin;

#[cfg(test)]
mod tests;

use engine::{Engine, FrameSet, SlotSet};
use gfx_dsp::envelope::PeakFollower;
use gfx_dsp::lfo::Rng;
use gfx_dsp::smooth::OnePole;
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::transport::{Sync, TransportInfo};
use gfx_dsp::{Effect, ParamInfo};

pub const NP: usize = 40;

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::enumeration("fx_a", "Effect", 30, 16),
    ParamInfo::float("amt_a", "Amount", 0.0, 100.0, 40.0).with_exp(),
    ParamInfo::float("sustain", "Sustain", 0.0, 10000.0, 0.0).with_exp(),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 35.0).with_exp(),
    ParamInfo::enumeration("size", "Size", 7, 2),
    ParamInfo::enumeration("window", "Window", 3, 0),
    ParamInfo::float("trim", "Input", -24.0, 24.0, 0.0),
    ParamInfo::float("cloud", "Cloud", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("damp", "Damp", 0.0, 100.0, 30.0),
    ParamInfo::float("pitch", "Pitch", -24.0, 24.0, 0.0).with_exp(),
    ParamInfo::float("spread", "Spread", -100.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("drift", "Drift", -500.0, 500.0, 0.0).with_exp(),
    ParamInfo::float("lo_a", "Region A Low", 0.0, 10.0, 0.0).with_exp(),
    ParamInfo::float("hi_a", "Region A High", 0.0, 10.0, 10.0).with_exp(),
    ParamInfo::enumeration("fx_b", "Effect B", 30, 0),
    ParamInfo::float("amt_b", "Amount B", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("lo_b", "Region B Low", 0.0, 10.0, 0.0),
    ParamInfo::float("hi_b", "Region B High", 0.0, 10.0, 10.0),
    ParamInfo::enumeration("fx_c", "Effect C", 30, 0),
    ParamInfo::float("amt_c", "Amount C", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("lo_c", "Region C Low", 0.0, 10.0, 0.0),
    ParamInfo::float("hi_c", "Region C High", 0.0, 10.0, 10.0),
    ParamInfo::float("density", "Density", -100.0, 100.0, 0.0).with_exp(),
    ParamInfo::enumeration("lfo_shape", "LFO Shape", 7, 0),
    ParamInfo::float("rate", "LFO Rate", 0.02, 20.0, 0.5).with_exp(),
    ParamInfo::enumeration("sync", "LFO Sync", 17, 0),
    ParamInfo::float("lfo_depth", "LFO Depth", -100.0, 100.0, 0.0).with_exp(),
    ParamInfo::enumeration("lfo_dest", "LFO Destination", 15, 0),
    ParamInfo::float("env_sens", "Envelope Sensitivity", 0.0, 100.0, 50.0),
    ParamInfo::float("env_att", "Envelope Attack", 1.0, 100.0, 10.0),
    ParamInfo::float("env_rel", "Envelope Release", 20.0, 2000.0, 200.0),
    ParamInfo::float("env_depth", "Envelope Depth", -100.0, 100.0, 0.0).with_exp(),
    ParamInfo::enumeration("env_dest", "Envelope Destination", 15, 0),
    ParamInfo::float("wet", "Wet Level", -60.0, 6.0, 0.0).with_exp(),
    ParamInfo::float("hpf", "Low Cut", 20.0, 500.0, 60.0),
    ParamInfo::float("lpf", "Tone", 1000.0, 20000.0, 12000.0).with_exp(),
    ParamInfo::float("width", "Width", 0.0, 100.0, 70.0),
    ParamInfo::boolean("align", "Align Dry", false),
    ParamInfo::boolean("trails", "Trails", true),
    ParamInfo::boolean("bypass", "Bypass", false),
];

pub mod p {
    pub const FX_A: usize = 0;
    pub const AMT_A: usize = 1;
    pub const SUSTAIN: usize = 2;
    pub const MIX: usize = 3;
    pub const SIZE: usize = 4;
    pub const WINDOW: usize = 5;
    pub const TRIM: usize = 6;
    pub const CLOUD: usize = 7;
    pub const DAMP: usize = 8;
    pub const PITCH: usize = 9;
    pub const SPREAD: usize = 10;
    pub const DRIFT: usize = 11;
    pub const LO_A: usize = 12;
    pub const HI_A: usize = 13;
    pub const FX_B: usize = 14;
    pub const FX_C: usize = 18;
    pub const DENSITY: usize = 22;
    pub const LFO_SHAPE: usize = 23;
    pub const RATE: usize = 24;
    pub const SYNC: usize = 25;
    pub const LFO_DEPTH: usize = 26;
    pub const LFO_DEST: usize = 27;
    pub const ENV_SENS: usize = 28;
    pub const ENV_ATT: usize = 29;
    pub const ENV_REL: usize = 30;
    pub const ENV_DEPTH: usize = 31;
    pub const ENV_DEST: usize = 32;
    pub const WET: usize = 33;
    pub const HPF: usize = 34;
    pub const LPF: usize = 35;
    pub const WIDTH: usize = 36;
    pub const ALIGN: usize = 37;
    pub const TRAILS: usize = 38;
    pub const BYPASS: usize = 39;
}

/// Window option labels (enum order, final).
pub const WINDOWS: [&str; 3] = ["Smooth", "Raw", "Ringing"];
/// LFO shapes (enum order, final).
pub const SHAPES: [&str; 7] = ["Sine", "Triangle", "Saw Up", "Saw Down", "Square", "Random Steps", "Random Smooth"];
/// Modulation destinations (enum order, final; append only).
pub const DESTS: [&str; 15] = ["Off", "Amount A", "Amount B", "Amount C", "All Amounts", "Sustain", "Pitch", "Spread", "Drift", "Region A", "Region B", "Region C", "Density", "Tone", "Wet"];

mod dest {
    pub const AMT_A: usize = 1;
    // Amount B = 2, Amount C = 3: AMT_A + slot
    pub const ALL: usize = 4;
    pub const SUSTAIN: usize = 5;
    pub const PITCH: usize = 6;
    pub const SPREAD: usize = 7;
    pub const DRIFT: usize = 8;
    pub const REG_A: usize = 9;
    pub const DENSITY: usize = 12;
    pub const TONE: usize = 13;
    pub const WET: usize = 14;
}

#[inline]
fn db(x: f32) -> f32 {
    10f32.powf(x / 20.0)
}

/// The LFO: phase in f64, seeded random shapes, a 3 ms output smoother (no edge clicks).
#[derive(Clone, Debug)]
struct ModLfo {
    phase: f64,
    rng: Rng,
    from: f32,
    to: f32,
}

impl ModLfo {
    fn new() -> Self {
        Self { phase: 0.0, rng: Rng::new(0x01F0_0D5E_ED00), from: 0.0, to: 0.0 }
    }
    fn reset(&mut self) {
        *self = Self::new();
    }
    fn wrapped(&mut self) {
        self.from = self.to;
        self.to = self.rng.bipolar();
    }
    fn raw(&self, shape: usize) -> f32 {
        let p = self.phase as f32;
        match shape {
            0 => (2.0 * std::f32::consts::PI * p).sin(),
            1 => 1.0 - 4.0 * (p - 0.5).abs(),
            2 => 2.0 * p - 1.0,
            3 => 1.0 - 2.0 * p,
            4 => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            5 => self.to,
            _ => self.from + (self.to - self.from) * (0.5 - 0.5 * (std::f32::consts::PI * p).cos()),
        }
    }
}

/// A Hilbert pair of all-pass cascades (Niemitalo's 4 + 4 sections, < 1° from 20 Hz to 20 kHz at
/// 44.1 kHz): two outputs in quadrature, for the single-sideband shifter.
#[derive(Clone, Copy, Debug, Default)]
struct Hilbert {
    s: [[f32; 4]; 8],
    d: f32,
}

const HA: [f32; 4] = [0.479_400_86, 0.876_218_5, 0.976_597_6, 0.997_499_9];
const HB: [f32; 4] = [0.161_758_5, 0.733_028_7, 0.945_349_8, 0.990_599_2];

impl Hilbert {
    /// (in-phase, quadrature)
    #[inline]
    fn next(&mut self, x: f32) -> (f32, f32) {
        let mut a = x;
        for i in 0..4 {
            let c = HA[i];
            let [x1, x2, y1, y2] = [self.s[i][0], self.s[i][1], self.s[i][2], self.s[i][3]];
            let y = c * (a + y2) - x2;
            self.s[i] = [a, x1, y, y1];
            a = y;
        }
        let mut b = x;
        for i in 0..4 {
            let c = HB[i];
            let j = i + 4;
            let [x1, x2, y1, y2] = [self.s[j][0], self.s[j][1], self.s[j][2], self.s[j][3]];
            let y = c * (b + y2) - x2;
            self.s[j] = [b, x1, y, y1];
            b = y;
        }
        let i = self.d;
        self.d = a;
        (i, b)
    }
}

const DRY_RING: usize = 65536;

pub struct Glitch {
    v: [f32; NP],
    sr: f32,
    nch: usize,
    eng: [Engine; 2],
    cur: usize,
    /// crossfade to the other engine: samples to wait (its latency), then position / length
    xf_wait: usize,
    xf_pos: usize,
    xf_len: usize,
    xf_on: bool,
    dry: Vec<Vec<f32>>,
    dpos: usize,
    mix: OnePole,
    wet: OnePole,
    trim: OnePole,
    drift: OnePole,
    lpf: OnePole,
    hpf: OnePole,
    align: OnePole,
    byp: OnePole,
    /// 0 → 1: the single-sideband path engaged (a 30 ms crossfade, never at a zero crossing of `drift`)
    ssb_on: OnePole,
    hp: [[Svf; 2]; 2],
    lp: [[Svf; 2]; 2],
    hil: [Hilbert; 2],
    ssb_phase: f64,
    lfo: ModLfo,
    lfo_out: f32,
    env: PeakFollower,
    env_out: f32,
    transport: TransportInfo,
    guard: f32,
    filt_count: u32,
}

impl Default for Glitch {
    fn default() -> Self {
        Self::new()
    }
}

impl Glitch {
    pub fn new() -> Self {
        let mut v = [0.0; NP];
        for (i, p) in PARAMS.iter().enumerate() {
            v[i] = p.default;
        }
        Self {
            v,
            sr: 48000.0,
            nch: 1,
            eng: [Engine::new(), Engine::new()],
            cur: 0,
            xf_wait: 0,
            xf_pos: 0,
            xf_len: 1,
            xf_on: false,
            dry: Vec::new(),
            dpos: 0,
            mix: OnePole::new(20.0, 0.35, 1e-5),
            wet: OnePole::new(20.0, 1.0, 1e-5),
            trim: OnePole::new(20.0, 1.0, 1e-5),
            drift: OnePole::new(20.0, 0.0, 1e-3),
            lpf: OnePole::new(20.0, 12000.0, 0.1),
            hpf: OnePole::new(20.0, 60.0, 0.01),
            align: OnePole::new(20.0, 0.0, 1e-5),
            byp: OnePole::new(10.0, 0.0, 1e-5),
            ssb_on: OnePole::new(30.0, 0.0, 1e-5),
            hp: [[Svf::new(), Svf::new()], [Svf::new(), Svf::new()]],
            lp: [[Svf::new(), Svf::new()], [Svf::new(), Svf::new()]],
            hil: [Hilbert::default(); 2],
            ssb_phase: 0.0,
            lfo: ModLfo::new(),
            lfo_out: 0.0,
            env: PeakFollower::new(10.0, 200.0),
            env_out: 0.0,
            transport: TransportInfo::default(),
            guard: 1.0,
            filt_count: 0,
        }
    }

    fn target_cfg(&self) -> (usize, usize) {
        (self.v[p::SIZE].round().clamp(0.0, 6.0) as usize, self.v[p::WINDOW].round().clamp(0.0, 2.0) as usize)
    }

    /// The modulation offset for a destination: LFO (bipolar) and envelope (0 – 1), each × its depth.
    #[inline]
    fn modv(&self, d: usize) -> f32 {
        let mut m = 0.0;
        if self.v[p::LFO_DEST].round() as usize == d {
            m += self.v[p::LFO_DEPTH] * 0.01 * self.lfo_out;
        }
        if self.v[p::ENV_DEST].round() as usize == d {
            m += self.v[p::ENV_DEPTH] * 0.01 * self.env_out;
        }
        m
    }

    fn frame_set(&self) -> FrameSet {
        let v = &self.v;
        let all = self.modv(dest::ALL);
        let mut slots = [SlotSet::default(); 3];
        for s in 0..3 {
            let b = p::FX_A + if s == 0 { 0 } else { p::FX_B + 4 * (s - 1) - p::FX_A };
            let (fx, amt, lo, hi) = if s == 0 { (v[p::FX_A], v[p::AMT_A], v[p::LO_A], v[p::HI_A]) } else { (v[b], v[b + 1], v[b + 2], v[b + 3]) };
            let ma = self.modv(dest::AMT_A + s) + all;
            let mr = self.modv(dest::REG_A + s) * 5.0;
            let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
            slots[s] = SlotSet { fx: fx.round().clamp(0.0, 29.0) as u8, amt: (amt * 0.01 + ma).clamp(0.0, 1.0), lo: (lo + mr).clamp(0.0, 10.0), hi: (hi + mr).clamp(0.0, 10.0) };
        }
        FrameSet {
            sustain_s: ((v[p::SUSTAIN] + self.modv(dest::SUSTAIN) * 5000.0).clamp(0.0, 10000.0)) * 0.001,
            damp: v[p::DAMP] * 0.01,
            cloud: v[p::CLOUD] * 0.01,
            pitch_st: (v[p::PITCH] + self.modv(dest::PITCH) * 12.0).clamp(-24.0, 24.0),
            spread: (v[p::SPREAD] * 0.01 + self.modv(dest::SPREAD)).clamp(-1.0, 1.0),
            density: (v[p::DENSITY] * 0.01 + self.modv(dest::DENSITY)).clamp(-1.0, 1.0),
            width: v[p::WIDTH] * 0.01,
            slots,
        }
    }

    fn set_targets(&mut self) {
        let v = self.v;
        self.mix.set_target(v[p::MIX] * 0.01);
        self.wet.set_target(db(v[p::WET]));
        self.trim.set_target(db(v[p::TRIM]));
        self.drift.set_target(v[p::DRIFT]);
        self.lpf.set_target(v[p::LPF]);
        self.hpf.set_target(v[p::HPF]);
        self.align.set_target(if v[p::ALIGN] > 0.5 { 1.0 } else { 0.0 });
        self.byp.set_target(if v[p::BYPASS] > 0.5 { 1.0 } else { 0.0 });
        let drift_mod = (v[p::LFO_DEST].round() as usize == dest::DRIFT && v[p::LFO_DEPTH] != 0.0) || (v[p::ENV_DEST].round() as usize == dest::DRIFT && v[p::ENV_DEPTH] != 0.0);
        self.ssb_on.set_target(if v[p::DRIFT] != 0.0 || drift_mod { 1.0 } else { 0.0 });
        self.env.set_times(v[p::ENV_ATT].max(1.0), v[p::ENV_REL].max(20.0), self.sr);
    }

    fn set_filters(&mut self, tone_mod: f32) {
        let lpf = (self.lpf.value() * 2f32.powf(tone_mod * 3.0)).clamp(200.0, 20000.0);
        let hpf = self.hpf.value();
        for c in 0..2 {
            // 4th order: two 2-pole sections (Butterworth Qs)
            self.hp[c][0].set(SvfMode::HighPass, hpf, 0.541_196, 0.0, self.sr);
            self.hp[c][1].set(SvfMode::HighPass, hpf, 1.306_563, 0.0, self.sr);
            self.lp[c][0].set(SvfMode::LowPass, lpf, std::f32::consts::FRAC_1_SQRT_2, 0.0, self.sr);
            self.lp[c][1].set(SvfMode::LowPass, lpf.min(self.sr * 0.45), std::f32::consts::FRAC_1_SQRT_2, 0.0, self.sr);
        }
    }

    fn reset_audio(&mut self) {
        let cfg = self.target_cfg();
        self.cur = 0;
        self.eng[0].configure(cfg.0, cfg.1);
        self.eng[0].active = true;
        self.eng[1].active = false;
        self.xf_on = false;
        for d in &mut self.dry {
            d.fill(0.0);
        }
        self.dpos = 0;
        for c in 0..2 {
            for f in self.hp[c].iter_mut().chain(self.lp[c].iter_mut()) {
                f.reset();
            }
        }
        self.hil = [Hilbert::default(); 2];
        self.ssb_phase = 0.0;
        self.guard = 1.0;
    }

    /// Reported latency: the active engine's when `align` is on.
    pub fn latency_samples(&self) -> usize {
        if self.v[p::ALIGN] > 0.5 {
            self.eng[self.cur].latency()
        } else {
            0
        }
    }
}

impl Effect for Glitch {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate;
        self.nch = channels.clamp(1, 2);
        let cfg = self.target_cfg();
        for e in &mut self.eng {
            e.size_idx = cfg.0;
            e.window = cfg.1;
            e.prepare(sample_rate, self.nch);
        }
        self.dry = vec![vec![0.0; DRY_RING]; self.nch];
        for s in [&mut self.mix, &mut self.wet, &mut self.trim, &mut self.drift, &mut self.lpf, &mut self.hpf, &mut self.align, &mut self.byp, &mut self.ssb_on] {
            s.prepare(sample_rate);
        }
        self.env.prepare(sample_rate);
        self.reset();
    }

    fn reset(&mut self) {
        self.set_targets();
        for s in [&mut self.mix, &mut self.wet, &mut self.trim, &mut self.drift, &mut self.lpf, &mut self.hpf, &mut self.align, &mut self.byp, &mut self.ssb_on] {
            s.settle();
        }
        self.lfo.reset();
        self.lfo_out = 0.0;
        self.env.reset();
        self.env_out = 0.0;
        self.reset_audio();
        self.set_filters(0.0);
        self.filt_count = 0;
    }

    fn set_param(&mut self, id: &str, value: f32) {
        if let Some(i) = PARAMS.iter().position(|p| p.id == id) {
            let p = &PARAMS[i];
            self.v[i] = value.clamp(p.min, p.max);
            self.set_targets();
        }
    }

    fn set_transport(&mut self, transport: TransportInfo) {
        self.transport = transport;
    }

    fn latency(&self) -> usize {
        self.latency_samples()
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(self.nch);
        if nch == 0 {
            return;
        }
        let n = buffers[0].len();
        let sr = self.sr;
        let srd = sr as f64;
        let v = self.v;
        let shape = v[p::LFO_SHAPE].round() as usize;
        let sync = Sync::from_index(v[p::SYNC].round() as usize);
        let beats = sync.beats();
        let sens = v[p::ENV_SENS] * 0.01;
        let trails = v[p::TRAILS] > 0.5;
        let lfo_used = v[p::LFO_DEST] >= 0.5 && v[p::LFO_DEPTH].abs() > 1e-3;
        let env_used = v[p::ENV_DEST] >= 0.5 && v[p::ENV_DEPTH].abs() > 1e-3;
        let tone_dest = self.v[p::LFO_DEST].round() as usize == dest::TONE || self.v[p::ENV_DEST].round() as usize == dest::TONE;

        // a size / window change: start the other engine
        let cfg = self.target_cfg();
        if !self.xf_on && cfg != (self.eng[self.cur].size_idx, self.eng[self.cur].window) {
            let o = 1 - self.cur;
            self.eng[o].configure(cfg.0, cfg.1);
            self.eng[o].active = true;
            self.xf_on = true;
            // its first frames still hold the silence it started on: wait until they have played out
            self.xf_wait = self.eng[o].latency() + self.eng[o].n;
            self.xf_len = ((0.05 * sr) as usize).max(self.eng[o].n);
            self.xf_pos = 0;
        }

        for i in 0..n {
            // ---- modulation
            if lfo_used {
                let prev = self.lfo.phase;
                match beats.and_then(|b| self.transport.phase_at(i, b, 0.0, sr)) {
                    Some(ph) => self.lfo.phase = ph,
                    None => {
                        let hz = match beats {
                            Some(b) => self.transport.bpm() / 60.0 / b,
                            None => v[p::RATE] as f64,
                        };
                        self.lfo.phase = (self.lfo.phase + hz / srd).fract();
                    }
                }
                if self.lfo.phase < prev {
                    self.lfo.wrapped();
                }
                let target = self.lfo.raw(shape);
                self.lfo_out += (target - self.lfo_out) * (1.0 - (-1.0 / (0.003 * sr)).exp());
            }
            let mut lvl = 0.0f32;
            for c in 0..nch {
                lvl = lvl.max(buffers[c][i].abs());
            }
            if env_used {
                let e = self.env.next(lvl);
                let floor = -20.0 - 40.0 * sens;
                self.env_out = ((20.0 * e.max(1e-6).log10() - floor) / 30.0).clamp(0.0, 1.0);
            }

            // ---- smoothed controls
            let mix = self.mix.next();
            let trim = self.trim.next();
            let wet_g = self.wet.next() * (1.0 + self.modv(dest::WET)).clamp(0.0, 2.0);
            let drift = (self.drift.next() + self.modv(dest::DRIFT) * 500.0).clamp(-500.0, 500.0);
            self.lpf.next();
            self.hpf.next();
            let al = self.align.next();
            let b = self.byp.next();
            self.filt_count += 1;
            if self.filt_count >= 16 {
                self.filt_count = 0;
                if !self.lpf.is_settled() || !self.hpf.is_settled() || tone_dest {
                    let tm = self.modv(dest::TONE);
                    self.set_filters(tm);
                }
            }

            // ---- engines
            let feed = if trails { 1.0 - b } else { 1.0 };
            let mut x = [0.0f32; 2];
            for c in 0..nch {
                x[c] = buffers[c][i] * trim * feed;
            }
            if nch == 1 {
                x[1] = x[0];
            }
            let mut y = [0.0f32; 2];
            {
                let cur = self.cur;
                self.eng[cur].tick(&x, &mut y);
                if self.eng[cur].want_latch {
                    let fs = self.frame_set();
                    self.eng[cur].set = fs;
                    self.eng[cur].want_latch = false;
                }
            }
            if self.xf_on {
                let o = 1 - self.cur;
                let mut y2 = [0.0f32; 2];
                self.eng[o].tick(&x, &mut y2);
                if self.eng[o].want_latch {
                    let fs = self.frame_set();
                    self.eng[o].set = fs;
                    self.eng[o].want_latch = false;
                }
                if self.xf_wait > 0 {
                    self.xf_wait -= 1;
                } else {
                    let t = (self.xf_pos as f32 + 0.5) / self.xf_len as f32;
                    let (go, gn) = ((t * std::f32::consts::FRAC_PI_2).cos(), (t * std::f32::consts::FRAC_PI_2).sin());
                    for c in 0..2 {
                        y[c] = y[c] * go + y2[c] * gn;
                    }
                    self.xf_pos += 1;
                    if self.xf_pos >= self.xf_len {
                        self.eng[self.cur].active = false;
                        self.cur = o;
                        self.xf_on = false;
                    }
                }
            }

            // ---- wet chain and mix
            let gw = (2.0 * mix).min(1.0);
            let gd = (2.0 * (1.0 - mix)).min(1.0);
            let dry_g = gd + (1.0 - gd) * b;
            let wet_mix = if trails { gw } else { gw * (1.0 - b) };
            let sb = self.ssb_on.next();
            let shift = sb > 0.0;
            let (s, co) = if shift {
                self.ssb_phase = (self.ssb_phase + drift as f64 / srd).rem_euclid(1.0);
                let a = (self.ssb_phase * std::f64::consts::TAU) as f32;
                a.sin_cos()
            } else {
                (0.0, 1.0)
            };
            let lat = self.eng[self.cur].latency();
            let mut bad = false;
            let mut out = [0.0f32; 2];
            for c in 0..nch {
                let mut w = y[c];
                let (hi, hq) = self.hil[c].next(w);
                if shift {
                    let ssb = hi * co + hq * s;
                    w = w + (ssb - w) * sb;
                }
                w = self.hp[c][0].process(w);
                w = self.hp[c][1].process(w);
                w = self.lp[c][0].process(w);
                w = self.lp[c][1].process(w);
                w *= wet_g;
                // soft limiter: tanh knee from −6 dBFS, ceiling −1 dBFS
                let aw = w.abs();
                if aw > 0.5 {
                    let ceil = 0.891_25;
                    w = w.signum() * (0.5 + (ceil - 0.5) * ((aw - 0.5) / (ceil - 0.5)).tanh());
                }
                let xin = buffers[c][i];
                let d = &mut self.dry[c];
                d[self.dpos] = xin;
                let delayed = d[(self.dpos + DRY_RING - lat.min(DRY_RING - 1)) % DRY_RING];
                let dry = xin + (delayed - xin) * al;
                let o = dry * dry_g + w * wet_mix * self.guard;
                if !o.is_finite() {
                    bad = true;
                }
                out[c] = o;
            }
            self.dpos = (self.dpos + 1) % DRY_RING;
            if bad {
                // guard: reset the engines and the wet filters, fade the wet back in over 10 ms
                self.reset_audio();
                self.guard = 0.0;
                for c in 0..nch {
                    buffers[c][i] = 0.0;
                }
                continue;
            }
            if self.guard < 1.0 {
                self.guard = (self.guard + 1.0 / (0.01 * sr)).min(1.0);
            }
            for c in 0..nch {
                buffers[c][i] = out[c];
            }
        }
    }
}
