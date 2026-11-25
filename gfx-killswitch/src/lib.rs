// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-killswitch — golem's kill switch, stutter and slicer (design: README.md).
//!
//! Stands for golem's `killswitch` block (ids `depth`, `length`, `sync`, `rate`, `trigger`, `sens`,
//! `smooth` kept, with the JSFX's meanings).
//!
//! - **Edges** are fixed-length raised-cosine crossfades, clamped to 45 % of the shortest segment, so the
//!   gain always reaches its target. The JSFX's slew limit turned a fast kill into a wobble.
//! - **Opening** is fast (≤ 1 ms) and ends on the step boundary: the note lands on the beat.
//! - **Timing**: 16-step patterns with accents, swing and offset. With the transport playing, the phase
//!   comes from the host's position (step 1 on the downbeat, loops and seeks stay aligned).
//! - **Pick restart**: a zero-latency detector, a high-passed fast / slow envelope difference.
//! - **Foot control**: momentary kill and restart.
//! - **Engage**: through the VST3 bypass parameter (each engage restarts the pattern).
//!
//! Zero latency.

#![allow(clippy::needless_range_loop)]

use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::util::one_pole_coef;
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
pub mod plugin;

/// `sync` options (catalogue order) in quarter notes; 0 = Free.
pub const SYNC_BEATS: [f64; 7] = [0.0, 1.0, 0.5, 1.0 / 3.0, 0.25, 1.0 / 6.0, 0.125];

pub static PARAMS: [ParamInfo; 17] = [
    ParamInfo::float("depth", "Depth", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::float("length", "Length", 5.0, 95.0, 50.0).with_exp(),
    ParamInfo::enumeration("sync", "Sync", 7, 4),
    ParamInfo::float("rate", "Rate", 1.0, 30.0, 8.0).with_exp(),
    ParamInfo::enumeration("trigger", "Trigger", 3, 0),
    ParamInfo::float("sens", "Sensitivity", 0.0, 100.0, 50.0),
    ParamInfo::float("smooth", "Smooth", 0.0, 20.0, 1.0),
    ParamInfo::enumeration("shape", "Shape", 5, 0),
    ParamInfo::float("pattern", "Pattern", 1.0, 65535.0, 65535.0),
    ParamInfo::float("steps", "Steps", 1.0, 16.0, 16.0),
    ParamInfo::float("accent", "Accents", 0.0, 65535.0, 0.0),
    ParamInfo::float("swing", "Swing", 0.0, 75.0, 0.0),
    ParamInfo::float("offset", "Offset", 0.0, 100.0, 0.0),
    ParamInfo::boolean("kill", "Kill", false),
    ParamInfo::boolean("restart", "Restart", false),
    ParamInfo::float("width", "Width", 0.0, 100.0, 0.0),
    ParamInfo::boolean("bypass", "Bypass", false),
];

pub mod p {
    pub const DEPTH: usize = 0;
    pub const LENGTH: usize = 1;
    pub const SYNC: usize = 2;
    pub const RATE: usize = 3;
    pub const TRIGGER: usize = 4;
    pub const SENS: usize = 5;
    pub const SMOOTH: usize = 6;
    pub const SHAPE: usize = 7;
    pub const PATTERN: usize = 8;
    pub const STEPS: usize = 9;
    pub const ACCENT: usize = 10;
    pub const SWING: usize = 11;
    pub const OFFSET: usize = 12;
    pub const KILL: usize = 13;
    pub const RESTART: usize = 14;
    pub const WIDTH: usize = 15;
    pub const BYPASS: usize = 16;
    pub const N: usize = 17;
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

#[inline]
fn rc(x: f64) -> f64 {
    0.5 - 0.5 * (std::f64::consts::PI * x.clamp(0.0, 1.0)).cos()
}

/// Where the pattern is: step index and samples into it, plus the step's and the next step's lengths.
struct Pos {
    k: usize,
    /// the absolute step number (not wrapped)
    abs: i64,
    t: f64,
    len: f64,
}

pub struct Killswitch {
    s: Settings,
    sr: f32,
    transport: TransportInfo,
    /// free-running position, in steps (Engage / Pick, or Beat with the transport stopped)
    pos: f64,
    depth: ExpSmoother,
    length: ExpSmoother,
    rate: ExpSmoother,
    width: OnePole,
    kill_g: LinearRamp,
    bypass_g: LinearRamp,
    was_bypassed: bool,
    was_restart: bool,
    // pick detector
    hp: [Svf; 2],
    fast: f32,
    slow: f32,
    armed: bool,
    refractory: u32,
    steps_count: u64,
    picks_count: u64,
    last_step: i64,
    /// pattern, steps, accents, shape and swing are latched at each step boundary (like a sequencer): a
    /// change mid-step would cut or open the current step at once
    latched: [f32; 5],
    offset: OnePole,
    /// the gain actually applied per channel, slew-limited over min(`smooth`, 1 ms): a change that lands
    /// exactly on a step boundary can still not jump (`smooth` 0 stays a hard cut)
    g_out: [f32; 2],
    clock: u64,
}

const LATCHED: [usize; 5] = [p::PATTERN, p::STEPS, p::ACCENT, p::SHAPE, p::SWING];

impl Killswitch {
    pub fn new() -> Self {
        let s = Settings::default();
        let v = s.v;
        Self {
            s,
            sr: 48000.0,
            transport: TransportInfo::default(),
            pos: 0.0,
            depth: ExpSmoother::new(0.0, 100.0, v[p::DEPTH]),
            length: ExpSmoother::new(5.0, 95.0, v[p::LENGTH]),
            rate: ExpSmoother::new(1.0, 30.0, v[p::RATE]),
            width: OnePole::new(30.0, v[p::WIDTH], 1e-4),
            kill_g: LinearRamp::new(1.0, 1.0),
            bypass_g: LinearRamp::new(5.0, 1.0),
            was_bypassed: false,
            was_restart: false,
            hp: Default::default(),
            fast: 0.0,
            slow: 0.0,
            armed: true,
            refractory: 0,
            steps_count: 0,
            picks_count: 0,
            last_step: -1,
            latched: [v[p::PATTERN], v[p::STEPS], v[p::ACCENT], v[p::SHAPE], v[p::SWING]],
            offset: OnePole::new(30.0, v[p::OFFSET], 1e-5),
            g_out: [1.0; 2],
            clock: 0,
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    /// Diagnostics (the JSFX's witness counters): steps started, picks detected.
    pub fn counters(&self) -> (u64, u64) {
        (self.steps_count, self.picks_count)
    }

    pub fn set(&mut self, s: Settings) {
        let v = &s.v;
        let old = self.s.v;
        for (i, e) in [(p::DEPTH, &mut self.depth), (p::LENGTH, &mut self.length), (p::RATE, &mut self.rate)] {
            if v[i] != old[i] {
                e.set_target(v[i].clamp(PARAMS[i].min, PARAMS[i].max), self.clock);
            }
        }
        self.width.set_target(v[p::WIDTH]);
        self.offset.set_target(v[p::OFFSET]);
        self.s = s;
    }

    /// Restarts the pattern at step 1.
    pub fn restart(&mut self) {
        self.pos = 0.0;
        self.last_step = -1;
        self.latch();
    }

    fn latch(&mut self) {
        for (l, i) in self.latched.iter_mut().zip(LATCHED) {
            *l = self.s.v[i];
        }
    }

    /// One step's length (samples).
    fn step_samples(&self, rate: f32) -> f64 {
        let b = SYNC_BEATS[idx(self.s.v[p::SYNC], 7)];
        if b == 0.0 {
            self.sr as f64 / rate.max(0.1) as f64
        } else {
            self.transport.note_samples(b, self.sr)
        }
    }

    /// Step index and position from a position counted in steps, with swing (pairs last (1 ± s)·T).
    fn locate(&self, steps_f: f64, step_len: f64, n_steps: usize) -> Pos {
        let s = self.latched[4] as f64 * 0.01;
        let pair = (steps_f / 2.0).floor();
        let within = steps_f - 2.0 * pair; // 0 … 2 in plain steps
        let first = 1.0 + s;
        let (half, t_steps, len) = if within < first { (0, within, first) } else { (1, within - first, 1.0 - s) };
        let abs = (2.0 * pair) as i64 + half;
        let k = abs as usize % n_steps.max(1);
        Pos { k, abs, t: t_steps * step_len, len: len * step_len }
    }

    /// The gain of the pattern at a position (in steps).
    fn gain_at(&self, steps_f: f64, step_len: f64, depth: f32, length: f32) -> f32 {
        let v = &self.s.v;
        let n = (self.latched[1].round() as usize).clamp(1, 16);
        let pattern = self.latched[0].round() as u32;
        let accent = self.latched[2].round() as u32;
        let pos = self.locate(steps_f, step_len, n);
        let on = |k: usize| pattern >> k & 1 == 1;
        let d = depth as f64 * 0.01;
        let shape = idx(self.latched[3], 5);
        if shape != 0 {
            // tremolo shapes over each step, depth in dB (60 dB at 100 %) so the swing sounds even
            let f = pos.t / pos.len;
            let u = match shape {
                1 => 0.5 + 0.5 * (std::f64::consts::TAU * f).cos(),
                2 => 1.0 - (2.0 * f - 1.0).abs(),
                3 => f,
                _ => 1.0 - f,
            };
            let u = if on(pos.k) { u } else { 0.0 };
            return 10f64.powf(-3.0 * d * (1.0 - u)) as f32;
        }
        let closed = |k: usize| 1.0 - d * if accent >> k & 1 == 1 || accent == 0 { 1.0 } else { 0.6 };
        let cl = if on(pos.k) { closed(pos.k) } else { 1.0 - d };
        let open_len = length as f64 * 0.01 * pos.len;
        let shut_len = pos.len - open_len;
        let sm = v[p::SMOOTH] as f64 * 0.001 * self.sr as f64;
        let e_dn = sm.min(0.45 * open_len.min(shut_len));
        let e_up = (0.001 * self.sr as f64).min(sm).min(0.45 * shut_len);
        let mut g = if on(pos.k) {
            if pos.t < open_len {
                1.0
            } else if e_dn > 0.0 && pos.t < open_len + e_dn {
                1.0 - (1.0 - cl) * rc((pos.t - open_len) / e_dn)
            } else {
                cl
            }
        } else {
            cl
        };
        // the next step opens on its boundary: the rise ends exactly there. The next step is read from the
        // pattern that will be latched at that boundary (the current settings), not the latched one
        let n_next = (v[p::STEPS].round() as usize).clamp(1, 16);
        let next = (pos.abs + 1) as usize % n_next;
        let next_on = (v[p::PATTERN].round() as u32) >> next & 1 == 1 && idx(v[p::SHAPE], 5) == 0;
        if next_on && e_up > 0.0 && pos.t > pos.len - e_up {
            g += (1.0 - g) * rc((pos.t - (pos.len - e_up)) / e_up);
        }
        g as f32
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(2);
        let n = buffers.first().map_or(0, |b| b.len());
        let sr = self.sr;
        let v = self.s.v;
        let trigger = idx(v[p::TRIGGER], 3);
        let bypassed = v[p::BYPASS] >= 0.5;
        if bypassed != self.was_bypassed {
            // engaging restarts the pattern (the JSFX inferred it from a gap in time)
            if !bypassed {
                self.restart();
            }
            self.bypass_g.set_target(if bypassed { 0.0 } else { 1.0 });
            self.was_bypassed = bypassed;
        }
        let restart = v[p::RESTART] >= 0.5;
        if restart && !self.was_restart {
            self.restart();
        }
        self.was_restart = restart;
        self.kill_g.set_target(if v[p::KILL] >= 0.5 { 0.0 } else { 1.0 });
        // the JSFX's sensitivity mapping: rise 12 … 2 dB, floor −30 … −60 dBFS
        let sens = v[p::SENS] * 0.01;
        let rise = 12.0 - 10.0 * sens;
        let floor = -30.0 - 30.0 * sens;
        let k_fast_a = one_pole_coef(0.0002 * sr as f64) as f32;
        let k_fast_r = one_pole_coef(0.005 * sr as f64) as f32;
        let k_slow = one_pole_coef(0.15 * sr as f64) as f32;
        let lim = (v[p::SMOOTH] * 0.001).min(0.001) * sr;
        let max_step = if lim >= 1.0 { 1.0 / lim } else { 0.0 };
        let beat_sync = trigger == 2 && self.transport.playing && self.transport.pos_beats.is_some() && SYNC_BEATS[idx(v[p::SYNC], 7)] > 0.0;
        for i in 0..n {
            self.clock += 1;
            let depth = self.depth.next();
            let length = self.length.next();
            let rate = self.rate.next();
            let width = self.width.next() * 0.01;
            let offset = self.offset.next() as f64 * 0.01;
            let kill = self.kill_g.next();
            let byp = self.bypass_g.next();
            let step_len = self.step_samples(rate);
            // pick detector: high-passed, fast vs slow envelope in dB, re-armed with hysteresis
            let mut x = 0.0f32;
            for c in 0..nch {
                x = x.max(buffers[c][i].abs());
            }
            let h0 = self.hp[0].process(x);
            let hpx = self.hp[1].process(h0);
            let a = hpx.abs();
            let kf = if a > self.fast { k_fast_a } else { k_fast_r };
            self.fast = a + (self.fast - a) * kf;
            self.slow = a + (self.slow - a) * k_slow;
            let (fdb, sdb) = (20.0 * self.fast.max(1e-7).log10(), 20.0 * self.slow.max(1e-7).log10());
            self.refractory = self.refractory.saturating_sub(1);
            if self.armed && self.refractory == 0 && fdb - sdb > rise && fdb > floor {
                self.picks_count += 1;
                self.armed = false;
                self.refractory = (0.04 * sr) as u32;
                if trigger == 1 {
                    self.restart();
                }
            } else if !self.armed && fdb - sdb < 0.5 * rise {
                self.armed = true;
            }
            // the position, in steps
            let steps_f = if beat_sync {
                let beats = SYNC_BEATS[idx(v[p::SYNC], 7)];
                let pos = self.transport.pos_beats.unwrap_or(0.0) + i as f64 / self.transport.samples_per_beat(sr);
                // anchored on beat 0, so in 4 / 4 step 1 falls on every downbeat; the epsilon keeps a host
                // position that lands a hair under a boundary on the right step
                pos / beats - offset + 1e-9
            } else {
                let p0 = self.pos;
                self.pos += 1.0 / step_len;
                p0 - offset
            }
            .max(0.0);
            let k = steps_f.floor() as i64;
            if k != self.last_step {
                self.steps_count += 1;
                self.last_step = k;
                self.latch();
            }
            let gl = self.gain_at(steps_f, step_len, depth, length);
            let gr = if width > 0.0 { gl + (self.gain_at(steps_f + 0.5, step_len, depth, length) - gl) * width } else { gl };
            for c in 0..nch {
                let target = if c == 0 { gl } else { gr } * kill;
                let g = if max_step > 0.0 {
                    let d = (target - self.g_out[c]).clamp(-max_step, max_step);
                    self.g_out[c] += d;
                    self.g_out[c]
                } else {
                    self.g_out[c] = target;
                    target
                };
                let x = buffers[c][i];
                buffers[c][i] = x + (x * g - x) * byp;
            }
        }
    }
}

impl Default for Killswitch {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Killswitch {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        for e in [&mut self.depth, &mut self.length, &mut self.rate] {
            e.prepare(sample_rate);
        }
        self.width.prepare(sample_rate);
        self.offset.prepare(sample_rate);
        self.kill_g.prepare(sample_rate);
        self.bypass_g.prepare(sample_rate);
        for f in self.hp.iter_mut() {
            f.set(SvfMode::HighPass, 1500.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, sample_rate);
        }
    }

    fn reset(&mut self) {
        let v = self.s.v;
        for (i, e) in [(p::DEPTH, &mut self.depth), (p::LENGTH, &mut self.length), (p::RATE, &mut self.rate)] {
            e.settle(v[i].clamp(PARAMS[i].min, PARAMS[i].max));
        }
        self.width.settle();
        self.offset.settle();
        self.kill_g.set_immediate(if v[p::KILL] >= 0.5 { 0.0 } else { 1.0 });
        self.was_bypassed = v[p::BYPASS] >= 0.5;
        self.bypass_g.set_immediate(if self.was_bypassed { 0.0 } else { 1.0 });
        self.was_restart = v[p::RESTART] >= 0.5;
        for f in self.hp.iter_mut() {
            f.reset();
        }
        self.fast = 0.0;
        self.slow = 0.0;
        self.armed = true;
        self.refractory = 0;
        self.steps_count = 0;
        self.picks_count = 0;
        self.g_out = [1.0; 2];
        self.restart();
        self.clock = 0;
    }

    fn set_param(&mut self, id: &str, value: f32) {
        if let Some(i) = PARAMS.iter().position(|pi| pi.id == id) {
            let s = self.s.with(i, value);
            self.set(s);
        }
    }

    fn set_transport(&mut self, transport: TransportInfo) {
        self.transport = transport;
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_block(buffers);
    }
}

#[cfg(test)]
mod tests;
