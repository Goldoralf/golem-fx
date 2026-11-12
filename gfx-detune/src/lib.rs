// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-detune — golem's detune / doubler (design: gfx-pitch/README.md §4).
//!
//! Stands for golem's `detune` block (GxDetune). It keeps the ids `detune` (0 – 1) and `wet` / `dry`
//! (linear 0 – 1):
//! - two tiny-ratio voices on gfx-dsp's centred splice-search shifter, one up and one down by `detune`
//!   × 25 cents;
//! - 3 ms centre, 50 ms grains: the splices are inaudible;
//! - `delay` offsets the voices (a doubler); with two voices on a stereo output, up goes left and down
//!   goes right.
//!
//! Reported latency 0 (the 3 ms centre is part of the doubled voice).

#![allow(clippy::needless_range_loop)]

use gfx_dsp::delay::{DelayLine, Interp};
use gfx_dsp::pitch::CenteredShifter;
use gfx_dsp::smooth::OnePole;
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
mod plugin;

const MAX_CH: usize = 2;
/// `detune` = 1 is this many cents each way.
pub const MAX_CENTS: f32 = 25.0;

pub static PARAMS: [ParamInfo; 5] = [
    ParamInfo::float("detune", "Detune", 0.0, 1.0, 0.3).with_exp(),
    ParamInfo::float("wet", "Wet", 0.0, 1.0, 0.5).with_exp(),
    ParamInfo::float("dry", "Dry", 0.0, 1.0, 0.7),
    ParamInfo::float("delay", "Doubling Delay", 0.0, 30.0, 0.0),
    ParamInfo::enumeration("voices", "Voices", 2, 1),
];

pub mod p {
    pub const DETUNE: usize = 0;
    pub const WET: usize = 1;
    pub const DRY: usize = 2;
    pub const DELAY: usize = 3;
    pub const VOICES: usize = 4;
    pub const N: usize = 5;
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

pub struct Detune {
    s: Settings,
    sr: f32,
    pre: [DelayLine; MAX_CH],
    up: [CenteredShifter; MAX_CH],
    down: [CenteredShifter; MAX_CH],
    detune: OnePole,
    wet: OnePole,
    dry: OnePole,
    /// the doubling delay changes by a crossfade between two taps (a glide would bend the pitch)
    tap: [f64; 2],
    xf: f64,
    two: OnePole,
}

impl Detune {
    pub fn new() -> Self {
        let s = Settings::default();
        let v = s.v;
        Self {
            s,
            sr: 48000.0,
            pre: Default::default(),
            up: Default::default(),
            down: Default::default(),
            detune: OnePole::new(30.0, v[p::DETUNE], 1e-5),
            wet: OnePole::new(20.0, v[p::WET], 1e-5),
            dry: OnePole::new(20.0, v[p::DRY], 1e-5),
            tap: [0.0; 2],
            xf: 1.0,
            two: OnePole::new(20.0, if v[p::VOICES] >= 0.5 { 1.0 } else { 0.0 }, 1e-5),
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn set(&mut self, s: Settings) {
        let v = &s.v;
        self.detune.set_target(v[p::DETUNE]);
        self.wet.set_target(v[p::WET]);
        self.dry.set_target(v[p::DRY]);
        self.two.set_target(if v[p::VOICES] >= 0.5 { 1.0 } else { 0.0 });
        self.s = s;
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(MAX_CH);
        let n = buffers.first().map_or(0, |b| b.len());
        let ms = self.sr as f64 * 0.001;
        for i in 0..n {
            let cents = (self.detune.next() * MAX_CENTS) as f64;
            let (wg, dg) = (self.wet.next(), self.dry.next());
            let target = self.s.v[p::DELAY].clamp(0.0, 30.0) as f64 * ms;
            if self.xf >= 1.0 && (target - self.tap[1]).abs() > 0.5 {
                self.tap = [self.tap[1], target];
                self.xf = 0.0;
            }
            if self.xf < 1.0 {
                self.xf = (self.xf + 1.0 / (0.05 * self.sr as f64)).min(1.0);
            }
            let (xf, ta, tb) = (self.xf as f32, self.tap[0], self.tap[1]);
            let two = self.two.next();
            let x0 = buffers[0][i];
            let x1 = if nch > 1 { buffers[1][i] } else { x0 };
            let mut up = [0.0f32; 2];
            let mut dn = [0.0f32; 2];
            for (c, x) in [x0, x1].into_iter().enumerate().take(nch) {
                self.pre[c].push(x);
                let rd = |line: &DelayLine, d: f64| if d < 0.5 { x } else { line.read(d, Interp::Lagrange3) };
                let xi = if xf < 1.0 { (1.0 - xf) * rd(&self.pre[c], ta) + xf * rd(&self.pre[c], tb) } else { rd(&self.pre[c], tb) };
                self.up[c].set_semitones(cents / 100.0);
                self.down[c].set_semitones(-cents / 100.0);
                up[c] = self.up[c].process(xi);
                dn[c] = self.down[c].process(xi);
            }
            if nch > 1 {
                // two voices: up left, down right; one voice: up on both sides
                let (l, r) = (up[0], dn[1] * two + up[1] * (1.0 - two));
                buffers[0][i] = dg * x0 + wg * l;
                buffers[1][i] = dg * x1 + wg * r;
            } else {
                let w = (up[0] + two * dn[0]) / (1.0 + 0.41 * two);
                buffers[0][i] = dg * x0 + wg * w;
            }
        }
    }
}

impl Default for Detune {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Detune {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        for o in [&mut self.detune, &mut self.wet, &mut self.dry, &mut self.two] {
            o.prepare(sample_rate);
        }
        for c in 0..MAX_CH {
            self.pre[c].prepare((0.032 * sample_rate as f64) as usize);
            self.up[c].prepare(sample_rate, 3.0, 1.0, 50.0);
            self.down[c].prepare(sample_rate, 3.0, 1.0, 50.0);
        }
    }

    fn reset(&mut self) {
        for o in [&mut self.detune, &mut self.wet, &mut self.dry, &mut self.two] {
            o.settle();
        }
        let d = self.s.v[p::DELAY].clamp(0.0, 30.0) as f64 * self.sr as f64 * 0.001;
        self.tap = [d, d];
        self.xf = 1.0;
        for c in 0..MAX_CH {
            self.pre[c].reset();
            self.up[c].reset();
            self.down[c].reset();
        }
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
mod tests {
    use super::*;

    #[test]
    fn detunes_by_the_set_cents() {
        let mut d = Detune::new();
        d.prepare(48000.0, 4800, 2);
        d.set(Settings::default().with(p::DETUNE, 1.0).with(p::DRY, 0.0).with(p::WET, 1.0));
        d.reset();
        let x: Vec<f32> = (0..96000).map(|i| 0.3 * (std::f32::consts::TAU * 1000.0 * i as f32 / 48000.0).sin()).collect();
        let (mut l, mut r) = (x.clone(), x.clone());
        for (a, b) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            d.process(&mut [a, b]);
        }
        let f = |y: &[f32]| {
            let mut t = Vec::new();
            for i in 1..y.len() {
                if y[i - 1] < 0.0 && y[i] >= 0.0 {
                    t.push((i - 1) as f64 + (-y[i - 1] / (y[i] - y[i - 1])) as f64);
                }
            }
            1200.0 * ((t.len() - 1) as f64 / (t[t.len() - 1] - t[0]) * 48000.0 / 1000.0).log2()
        };
        let (cl, cr) = (f(&l[12000..]), f(&r[12000..]));
        eprintln!("detune L {cl:+.3}, R {cr:+.3} cents");
        assert!((cl - 25.0).abs() < 0.2 && (cr + 25.0).abs() < 0.2, "L {cl:.2}, R {cr:.2} cents");
    }
}
