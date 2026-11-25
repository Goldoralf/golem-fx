// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-octaver — golem's octaver (design: README.md).
//!
//! Stands for golem's `octaver` block (ids `direct`, `octave1` = −1 oct, `octave2` = −2 oct, each 0 – 1;
//! `direct` = 1 is unity). Four modes, all with zero reported latency:
//! - **Vintage**: the Boss OC-2. Adaptive Schmitt trigger → ÷2 flip-flop → the filtered signal's polarity
//!   switched, so the sub keeps the player's envelope. ÷4 is clocked from the −1 voice's zero crossings.
//! - **Clean**: the half-angle identity cos(θ/2) = ±√((1 + cos θ) / 2) on the input through a low-pass
//!   that tracks the detected fundamental. A near-sine sub made from the player's own waveform, sample
//!   by sample. The bass player's mode.
//! - **Synth**: an oscillator phase-locked to the divider, sine → square, glide; muted when the period
//!   estimate is unstable (a confidence gate, never in the audio path).
//! - **Poly**: a non-decimated complex filterbank on the ERB scale.
//!   - Up: z² / |z| per band.
//!   - Down: per-band square root with branch tracking, only below `range`.
//!
//! Octave up: a full-wave rectifier (Vintage, Synth, oversampled 2×), the analytic signal z² / |z| (Clean).

#![allow(clippy::needless_range_loop)]

use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::oversample::Oversampler2x;
use gfx_dsp::pitch::Hilbert;
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::util::one_pole_coef;
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
pub mod plugin;

const MAX_CH: usize = 2;
const N_BANDS: usize = 28;

pub static PARAMS: [ParamInfo; 13] = [
    ParamInfo::float("direct", "Direct", 0.0, 1.0, 1.0).with_exp(),
    ParamInfo::float("octave1", "Octave -1", 0.0, 1.0, 0.6).with_exp(),
    ParamInfo::float("octave2", "Octave -2", 0.0, 1.0, 0.0).with_exp(),
    ParamInfo::float("octave_up", "Octave +1", 0.0, 1.0, 0.0).with_exp(),
    ParamInfo::enumeration("mode", "Mode", 4, 0),
    ParamInfo::enumeration("instrument", "Instrument", 2, 0),
    ParamInfo::float("sub_tone", "Sub Tone", 60.0, 2000.0, 470.0),
    ParamInfo::float("up_tone", "Up Tone", 500.0, 12000.0, 4000.0),
    ParamInfo::float("sensitivity", "Sensitivity", 0.0, 100.0, 50.0),
    ParamInfo::float("glide", "Glide", 0.0, 200.0, 0.0),
    ParamInfo::float("shape", "Shape", 0.0, 100.0, 0.0),
    ParamInfo::float("range", "Range", 40.0, 1000.0, 1000.0),
    ParamInfo::float("level", "Level", -24.0, 24.0, 0.0),
];

pub mod p {
    pub const DIRECT: usize = 0;
    pub const OCTAVE1: usize = 1;
    pub const OCTAVE2: usize = 2;
    pub const OCTAVE_UP: usize = 3;
    pub const MODE: usize = 4;
    pub const INSTRUMENT: usize = 5;
    pub const SUB_TONE: usize = 6;
    pub const UP_TONE: usize = 7;
    pub const SENSITIVITY: usize = 8;
    pub const GLIDE: usize = 9;
    pub const SHAPE: usize = 10;
    pub const RANGE: usize = 11;
    pub const LEVEL: usize = 12;
    pub const N: usize = 13;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub v: [f32; p::N],
}

impl Default for Settings {
    fn default() -> Self {
        Self { v: core::array::from_fn(|i| PARAMS[i].default) }
    }
}

impl Settings {
    pub fn with(mut self, i: usize, value: f32) -> Self {
        self.v[i] = value;
        self
    }
}

#[inline]
fn idx(v: f32, n: usize) -> usize {
    (v.round().max(0.0) as usize).min(n - 1)
}

/// One complex band of the Poly filterbank, with the sub's branch state.
#[derive(Clone, Copy, Default)]
struct Band {
    p_re: f64,
    p_im: f64,
    g: f64,
    z_re: f64,
    z_im: f64,
    w_re: f64,
    w_im: f64,
    f: f64,
}

#[derive(Clone, Default)]
struct Channel {
    smooth: [Svf; 2],
    env: f32,
    q: f32,
    ff1: f32,
    ff2: f32,
    sub1_prev: f32,
    // period tracking (Clean, Synth)
    since_edge: f64,
    period: f64,
    prev_period: f64,
    track: [Svf; 2],
    t_env: f32,
    /// the cycle's peak, held: the envelope is the last cycle's peak (no ripple inside a cycle)
    t_cyc: f32,
    t_held: f32,
    t_prev: f32,
    t_dir: f32,
    t_ff1: f32,
    t_ff2: f32,
    s1_prev: f32,
    s1_dir: f32,
    // synth
    osc_ph: f64,
    osc_hz: f64,
    gate: f32,
    // outputs' filters, dry alignment, octave up
    sub_lp: [Svf; 2],
    sub2_lp: [Svf; 2],
    up_lp: Svf,
    up_hp: Svf,
    os: Oversampler2x,
    up_dc: (f32, f32),
    dry_ap: [(f32, f32); 2],
    hil: Hilbert,
    bands: Vec<Band>,
}

pub struct Octaver {
    s: Settings,
    sr: f32,
    ch: [Channel; MAX_CH],
    direct: ExpSmoother,
    octave1: ExpSmoother,
    octave2: ExpSmoother,
    octave_up: ExpSmoother,
    sub_tone: OnePole,
    up_tone: OnePole,
    level: OnePole,
    mode: usize,
    pending: bool,
    gate: LinearRamp,
    clock: u64,
    ctrl: u32,
}

impl Octaver {
    pub fn new() -> Self {
        let s = Settings::default();
        let v = s.v;
        Self {
            s,
            sr: 48000.0,
            ch: Default::default(),
            direct: ExpSmoother::new(0.0, 1.0, v[p::DIRECT]),
            octave1: ExpSmoother::new(0.0, 1.0, v[p::OCTAVE1]),
            octave2: ExpSmoother::new(0.0, 1.0, v[p::OCTAVE2]),
            octave_up: ExpSmoother::new(0.0, 1.0, v[p::OCTAVE_UP]),
            sub_tone: OnePole::new(30.0, v[p::SUB_TONE], 1e-3),
            up_tone: OnePole::new(30.0, v[p::UP_TONE], 1e-3),
            level: OnePole::new(20.0, v[p::LEVEL], 1e-4),
            mode: 0,
            pending: false,
            gate: LinearRamp::new(20.0, 1.0),
            clock: 0,
            ctrl: 0,
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn set(&mut self, s: Settings) {
        let v = &s.v;
        let old = self.s.v;
        for (i, e) in [(p::DIRECT, &mut self.direct), (p::OCTAVE1, &mut self.octave1), (p::OCTAVE2, &mut self.octave2), (p::OCTAVE_UP, &mut self.octave_up)] {
            if v[i] != old[i] {
                e.set_target(v[i].clamp(0.0, 1.0), self.clock);
            }
        }
        self.sub_tone.set_target(v[p::SUB_TONE]);
        self.up_tone.set_target(v[p::UP_TONE]);
        self.level.set_target(v[p::LEVEL]);
        if idx(v[p::MODE], 4) != self.mode || idx(v[p::INSTRUMENT], 2) != idx(old[p::INSTRUMENT], 2) {
            self.pending = true;
            self.gate.set_target(0.0);
        }
        self.s = s;
    }

    fn control(&mut self) {
        let sr = self.sr;
        let bass = idx(self.s.v[p::INSTRUMENT], 2) == 1;
        let smooth_f = if bass { 300.0 } else { 800.0 };
        let st = self.sub_tone.next();
        let ut = self.up_tone.next().min(0.45 * sr);
        for c in self.ch.iter_mut() {
            for f in c.smooth.iter_mut() {
                f.set(SvfMode::LowPass, smooth_f, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            }
            for f in c.sub_lp.iter_mut() {
                f.set(SvfMode::LowPass, st, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            }
            for f in c.sub2_lp.iter_mut() {
                f.set(SvfMode::LowPass, 0.45 * st, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            }
            c.up_lp.set(SvfMode::LowPass, ut, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            c.up_hp.set(SvfMode::HighPass, 80.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            // the tracking low-pass of Clean: 1.5 × the detected fundamental
            let f0 = if c.period > 0.0 { sr as f64 / c.period } else { smooth_f as f64 * 0.5 };
            let ft = (1.5 * f0).clamp(30.0, 2000.0) as f32;
            for f in c.track.iter_mut() {
                f.set(SvfMode::LowPass, ft, 0.6, 0.0, sr);
            }
        }
    }

    fn build_bands(&mut self) {
        let sr = self.sr as f64;
        for c in self.ch.iter_mut() {
            c.bands = (0..N_BANDS)
                .map(|k| {
                    // 40 Hz … 5 kHz, log spaced; bandwidth 1 ERB
                    let f = 40.0 * (5000.0f64 / 40.0).powf(k as f64 / (N_BANDS - 1) as f64);
                    let erb = 24.7 * (4.37 * f / 1000.0 + 1.0);
                    let r = (-std::f64::consts::PI * erb / sr).exp();
                    let w = std::f64::consts::TAU * f / sr;
                    Band { p_re: r * w.cos(), p_im: r * w.sin(), g: 1.0 - r, f, ..Default::default() }
                })
                .collect();
        }
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(MAX_CH);
        let n = buffers.first().map_or(0, |b| b.len());
        let sr = self.sr;
        let v = self.s.v;
        let bass = idx(v[p::INSTRUMENT], 2) == 1;
        let sens = v[p::SENSITIVITY] * 0.01;
        let hyst = 0.05 + 0.35 * (1.0 - sens);
        let floor = 10f32.powf((-50.0 - 20.0 * sens) / 20.0);
        let env_rel = one_pole_coef((if bass { 0.1 } else { 0.05 }) * sr as f64) as f32;
        let env_att = one_pole_coef(0.001 * sr as f64) as f32;
        let env_glide = one_pole_coef(0.02 * sr as f64) as f32;
        let shape = v[p::SHAPE] * 0.01;
        let glide_k = if v[p::GLIDE] < 1.0 { 0.0 } else { (-1.0 / (v[p::GLIDE] as f64 * 0.001 * sr as f64)).exp() };
        let range = v[p::RANGE] as f64;
        let dc_k = one_pole_coef(sr as f64 / (std::f64::consts::TAU * 10.0)) as f32;
        // the dry's alignment corner is fixed between the two smoothing filters (300 / 800 Hz): switching
        // the instrument must not move the dry
        let ap_c = {
            let t = (std::f64::consts::PI * 550.0 / sr as f64).tan();
            ((t - 1.0) / (t + 1.0)) as f32
        };
        for i in 0..n {
            self.clock += 1;
            if self.ctrl == 0 {
                self.control();
            }
            self.ctrl = (self.ctrl + 1) % 16;
            let g = self.gate.next();
            if g == 0.0 && self.gate.is_settled() {
                if self.pending {
                    self.mode = idx(self.s.v[p::MODE], 4);
                    self.pending = false;
                }
                self.gate.set_target(1.0);
            }
            let gs = 0.5 - 0.5 * (std::f32::consts::PI * g).cos();
            let direct = self.direct.next();
            let o1 = self.octave1.next() * gs;
            let o2 = self.octave2.next() * gs;
            let up = self.octave_up.next() * gs;
            let lv = self.level.next();
            let og = if lv == 0.0 { 1.0 } else { 10f32.powf(lv / 20.0) };
            let mode = self.mode;
            for c in 0..nch {
                let x = buffers[c][i];
                let ch = &mut self.ch[c];
                // smoothing low-pass and envelope
                let s0 = ch.smooth[0].process(x);
                let xs = ch.smooth[1].process(s0);
                let a = xs.abs();
                let k = if a > ch.env { env_att } else { env_rel };
                ch.env = a + (ch.env - a) * k;
                let live = ch.env > floor;
                // adaptive Schmitt trigger on the smoothed signal: thresholds follow the envelope
                let th = hyst * ch.env;
                let mut rising = false;
                if ch.q <= 0.0 && xs > th && live {
                    ch.q = 1.0;
                    rising = true;
                } else if ch.q >= 0.0 && xs < -th {
                    ch.q = -1.0;
                }
                if ch.ff1 == 0.0 {
                    ch.ff1 = 1.0;
                    ch.ff2 = 1.0;
                }
                ch.since_edge += 1.0;
                if rising {
                    ch.ff1 = -ch.ff1;
                    ch.prev_period = ch.period;
                    ch.period = ch.since_edge;
                    ch.since_edge = 0.0;
                }
                // dry aligned to the smoothed path's phase (two all-passes)
                let mut dry = x;
                for st in ch.dry_ap.iter_mut() {
                    let y = ap_c * dry + st.0 - ap_c * st.1;
                    st.0 = dry;
                    st.1 = y;
                    dry = y;
                }
                let (mut sub1, mut sub2);
                let mut upv = 0.0f32;
                match mode {
                    1 => {
                        // Clean: the half-angle identity on a near-sinusoid
                        let t0 = ch.track[0].process(x);
                        let xt = ch.track[1].process(t0);
                        let at = xt.abs();
                        ch.t_cyc = ch.t_cyc.max(at);
                        // the envelope: last cycle's peak, gliding (20 ms); a louder sample lifts it at once
                        ch.t_env = ch.t_held + (ch.t_env - ch.t_held) * env_glide;
                        if at > ch.t_env {
                            ch.t_env = at;
                        }
                        let e = ch.t_env.max(1e-6);
                        // the sign flips at the troughs (θ = π), where the root is zero: continuous
                        let dir = (xt - ch.t_prev).signum();
                        if ch.t_dir < 0.0 && dir > 0.0 && xt < -0.5 * e {
                            ch.t_ff1 = -ch.t_ff1;
                            ch.t_held = ch.t_cyc;
                            ch.t_cyc = 0.0;
                        }
                        if ch.t_ff1 == 0.0 {
                            ch.t_ff1 = 1.0;
                            ch.t_ff2 = 1.0;
                        }
                        if dir != 0.0 {
                            ch.t_dir = dir;
                        }
                        ch.t_prev = xt;
                        let c1 = (xt / e).clamp(-1.0, 1.0);
                        sub1 = ch.t_ff1 * e * ((1.0 + c1) * 0.5).max(0.0).sqrt();
                        // −2: the same identity on the −1 voice
                        let d1 = (sub1 - ch.s1_prev).signum();
                        if ch.s1_dir < 0.0 && d1 > 0.0 && sub1 < -0.5 * e {
                            ch.t_ff2 = -ch.t_ff2;
                        }
                        if d1 != 0.0 {
                            ch.s1_dir = d1;
                        }
                        ch.s1_prev = sub1;
                        let c2 = (sub1 / e).clamp(-1.0, 1.0);
                        sub2 = ch.t_ff2 * e * ((1.0 + c2) * 0.5).max(0.0).sqrt();
                        if !live {
                            sub1 = 0.0;
                            sub2 = 0.0;
                        }
                        // octave up: z² / |z| of the analytic signal
                        let (re, im) = ch.hil.process(x);
                        let m = (re * re + im * im).sqrt().max(1e-9);
                        upv = ((re * re - im * im) / m) as f32;
                    }
                    2 => {
                        // Synth: an oscillator locked to the divider; muted when the period is unstable
                        if ch.period > 0.0 {
                            let target = sr as f64 / (2.0 * ch.period);
                            ch.osc_hz = target + (ch.osc_hz - target) * glide_k;
                        }
                        ch.osc_ph = (ch.osc_ph + ch.osc_hz / sr as f64).fract();
                        if rising && ch.ff1 > 0.0 {
                            // lock: pull the phase towards 0 at each divider edge
                            let e = if ch.osc_ph > 0.5 { ch.osc_ph - 1.0 } else { ch.osc_ph };
                            ch.osc_ph = (ch.osc_ph - 0.5 * e).rem_euclid(1.0);
                        }
                        let stable = ch.prev_period > 0.0 && (ch.period / ch.prev_period - 1.0).abs() < 0.2;
                        let gt = if live && stable { 1.0 } else { 0.0 };
                        ch.gate += (gt - ch.gate) * 0.002;
                        let sq = |ph: f64| {
                            let s = (std::f64::consts::TAU * ph).sin();
                            let k = 1.0 + 20.0 * shape as f64;
                            ((k * s).tanh() / k.tanh()) as f32
                        };
                        sub1 = sq(ch.osc_ph) * ch.env * ch.gate;
                        sub2 = sq(ch.osc_ph * 0.5 + if ch.ff2 > 0.0 { 0.0 } else { 0.5 }) * ch.env * ch.gate;
                        if rising && ch.ff1 > 0.0 {
                            ch.ff2 = -ch.ff2;
                        }
                    }
                    3 => {
                        // Poly: per band, up = z² / |z|, down = √z with branch tracking below `range`
                        let (mut s1, mut uu) = (0.0f64, 0.0f64);
                        let xd = x as f64;
                        for b in ch.bands.iter_mut() {
                            let zr = b.p_re * b.z_re - b.p_im * b.z_im + b.g * xd;
                            let zi = b.p_re * b.z_im + b.p_im * b.z_re;
                            b.z_re = zr;
                            b.z_im = zi;
                            let m = (zr * zr + zi * zi).sqrt();
                            if m < 1e-9 {
                                continue;
                            }
                            uu += (zr * zr - zi * zi) / m;
                            if b.f < range {
                                // the root of the unit phasor, times |z|; the branch closest to the last one
                                let ang = zi.atan2(zr) * 0.5;
                                let (mut wr, mut wi) = (m * ang.cos(), m * ang.sin());
                                if wr * b.w_re + wi * b.w_im < 0.0 {
                                    wr = -wr;
                                    wi = -wi;
                                }
                                b.w_re = wr;
                                b.w_im = wi;
                                s1 += wr;
                            }
                        }
                        // each band carries half of a real component: × 2, and the bank overlaps ≈ 2.4×
                        sub1 = (s1 * 2.0 / 2.4) as f32;
                        upv = (uu * 2.0 / 2.4) as f32;
                        sub2 = 0.0;
                    }
                    _ => {
                        // Vintage: the filtered signal, its polarity switched by the flip-flops
                        sub1 = ch.ff1 * xs;
                        if ch.sub1_prev < 0.0 && sub1 >= 0.0 {
                            ch.ff2 = -ch.ff2;
                        }
                        ch.sub1_prev = sub1;
                        sub2 = ch.ff2 * sub1;
                        if !live {
                            sub1 *= ch.env / floor;
                            sub2 *= ch.env / floor;
                        }
                    }
                }
                if mode != 1 && mode != 3 {
                    // octave up: full-wave rectifier, oversampled 2×, DC-blocked
                    let h = ch.up_hp.process(x);
                    let [a0, a1] = ch.os.up(h);
                    let r = ch.os.down([a0.abs(), a1.abs()]);
                    let d = r - ch.up_dc.0 + dc_k * ch.up_dc.1;
                    ch.up_dc = (r, d);
                    upv = 2.0 * d;
                }
                let s1a = ch.sub_lp[0].process(sub1);
                let s1f = ch.sub_lp[1].process(s1a);
                let s2a = ch.sub2_lp[0].process(sub2);
                let s2f = ch.sub2_lp[1].process(s2a);
                let upf = ch.up_lp.process(upv);
                buffers[c][i] = (direct * dry + o1 * s1f + o2 * s2f + up * upf) * og;
            }
        }
    }
}

impl Default for Octaver {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Octaver {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        for e in [&mut self.direct, &mut self.octave1, &mut self.octave2, &mut self.octave_up] {
            e.prepare(sample_rate);
        }
        self.sub_tone.prepare(sample_rate / 16.0);
        self.up_tone.prepare(sample_rate / 16.0);
        self.gate.prepare(sample_rate);
        self.level.prepare(sample_rate);
        self.build_bands();
        self.control();
    }

    fn reset(&mut self) {
        let v = self.s.v;
        for (i, e) in [(p::DIRECT, &mut self.direct), (p::OCTAVE1, &mut self.octave1), (p::OCTAVE2, &mut self.octave2), (p::OCTAVE_UP, &mut self.octave_up)] {
            e.settle(v[i].clamp(0.0, 1.0));
        }
        self.sub_tone.settle();
        self.up_tone.settle();
        self.level.settle();
        self.mode = idx(v[p::MODE], 4);
        self.pending = false;
        self.gate.set_immediate(1.0);
        for c in self.ch.iter_mut() {
            let bands = std::mem::take(&mut c.bands);
            *c = Channel::default();
            c.bands = bands;
            for b in c.bands.iter_mut() {
                b.z_re = 0.0;
                b.z_im = 0.0;
                b.w_re = 0.0;
                b.w_im = 0.0;
            }
        }
        self.ctrl = 0;
        self.clock = 0;
        self.control();
    }

    fn set_param(&mut self, id: &str, value: f32) {
        if let Some(i) = PARAMS.iter().position(|pi| pi.id == id) {
            let s = self.s.with(i, value);
            self.set(s);
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_block(buffers);
    }
}

#[cfg(test)]
mod tests;
