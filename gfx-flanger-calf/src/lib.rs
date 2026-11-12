// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-flanger-calf — gfx-flanger's DSP behind the ids of golem's `flanger` block (Calf Flanger).
//!
//! golem has two flanger blocks whose ids clash in meaning: `flanger-js` (feedback in dB, length in ms)
//! and `flanger` (Calf: `depth` = sweep width in ms, `feedback` linear −0.9 … 0.9). gfx-flanger stands
//! for the first. This second class stands for the second, on the same engine, so Calf presets keep
//! their values. Calf's sweep runs from `min_delay` to `min_delay + depth`.

use gfx_dsp::smooth::OnePole;
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::{Effect, ParamInfo};
use gfx_dsp::flanger::{p as fp, Flanger, Settings as FlangerSettings};

#[cfg(feature = "plugin")]
mod plugin;

pub static PARAMS: [ParamInfo; 8] = [
    ParamInfo::float("rate", "Mod Rate", 0.05, 5.0, 0.3).with_exp(),
    ParamInfo::float("depth", "Mod Depth", 0.1, 10.0, 2.0).with_exp(),
    ParamInfo::float("feedback", "Feedback", -0.99, 0.99, 0.6).with_exp(),
    ParamInfo::float("min_delay", "Min Delay", 0.1, 10.0, 0.5),
    ParamInfo::float("amount", "Amount", 0.0, 1.0, 0.5),
    ParamInfo::float("dry", "Dry Amount", 0.0, 1.0, 1.0),
    ParamInfo::float("stereo_phase", "Stereo Phase", 0.0, 180.0, 90.0),
    ParamInfo::enumeration("mode", "Mode", 9, 0),
];

pub mod p {
    pub const RATE: usize = 0;
    pub const DEPTH: usize = 1;
    pub const FEEDBACK: usize = 2;
    pub const MIN_DELAY: usize = 3;
    pub const AMOUNT: usize = 4;
    pub const DRY: usize = 5;
    pub const STEREO_PHASE: usize = 6;
    pub const MODE: usize = 7;
    pub const N: usize = 8;
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

#[inline]
fn to_db(a: f32) -> f32 {
    if a <= 0.001 {
        -60.0
    } else {
        (20.0 * a.log10()).clamp(-60.0, 0.0)
    }
}

/// Calf's values → gfx-flanger's settings.
pub fn translate(s: &Settings) -> FlangerSettings {
    let v = &s.v;
    let min = v[p::MIN_DELAY].clamp(0.1, 10.0);
    let fb = v[p::FEEDBACK].clamp(-0.99, 0.99);
    FlangerSettings::default()
        .with(fp::RATE, v[p::RATE])
        .with(fp::MANUAL, min)
        .with(fp::LENGTH, (min + v[p::DEPTH].max(0.0)).min(30.0))
        // magnitude in dB (the engine's loop tops out at −1 dB), the sign as the polarity
        .with(fp::FEEDBACK, if fb.abs() < 0.001 { -60.0 } else { (20.0 * fb.abs().log10()).clamp(-60.0, -1.0) })
        .with(fp::FB_POLARITY, if fb < 0.0 { 1.0 } else { 0.0 })
        .with(fp::WET, to_db(v[p::AMOUNT]))
        .with(fp::DRY, to_db(v[p::DRY]))
        .with(fp::STEREO_PHASE, v[p::STEREO_PHASE])
        .with(fp::MODE, v[p::MODE])
}

pub struct CalfFlanger {
    s: Settings,
    inner: Flanger,
    /// Calf's min delay and depth glide together (in log ms, 50 ms), so the sweep's two ends never
    /// part ways while one of them moves
    min_ln: OnePole,
    depth_ln: OnePole,
}

/// Sub-block (samples) at which the glided values are passed to the engine.
const SUB: usize = 32;

impl CalfFlanger {
    pub fn new() -> Self {
        let s = Settings::default();
        let mut inner = Flanger::new();
        inner.set(translate(&s));
        Self {
            s,
            inner,
            min_ln: OnePole::new(50.0, s.v[p::MIN_DELAY].ln(), 1e-6),
            depth_ln: OnePole::new(50.0, s.v[p::DEPTH].ln(), 1e-6),
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn set(&mut self, s: Settings) {
        self.s = s;
        self.min_ln.set_target(s.v[p::MIN_DELAY].clamp(0.1, 10.0).ln());
        self.depth_ln.set_target(s.v[p::DEPTH].clamp(0.1, 10.0).ln());
        self.inner.set(translate(&self.glided()));
    }

    /// The settings with min delay and depth at their glided values.
    fn glided(&self) -> Settings {
        let mut g = self.s;
        g.v[p::MIN_DELAY] = self.min_ln.value().exp();
        g.v[p::DEPTH] = self.depth_ln.value().exp();
        g
    }

    pub fn latency_samples(&self) -> usize {
        self.inner.latency_samples()
    }
}

impl Default for CalfFlanger {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for CalfFlanger {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, max_block: usize, channels: usize) {
        self.min_ln.prepare(sample_rate / SUB as f32);
        self.depth_ln.prepare(sample_rate / SUB as f32);
        self.inner.prepare(sample_rate, max_block, channels);
    }

    fn reset(&mut self) {
        self.min_ln.settle();
        self.depth_ln.settle();
        self.inner.set(translate(&self.glided()));
        self.inner.reset();
    }

    fn set_param(&mut self, id: &str, value: f32) {
        if let Some(i) = PARAMS.iter().position(|pi| pi.id == id) {
            let mut s = self.s;
            s.v[i] = value;
            self.set(s);
        }
    }

    fn latency(&self) -> usize {
        self.inner.latency_samples()
    }

    fn set_transport(&mut self, transport: TransportInfo) {
        self.inner.set_transport(transport);
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        let n = buffers.first().map_or(0, |b| b.len());
        let mut start = 0;
        while start < n {
            let end = (start + SUB).min(n);
            if !(self.min_ln.is_settled() && self.depth_ln.is_settled()) {
                self.min_ln.next();
                self.depth_ln.next();
                let g = translate(&self.glided());
                self.inner.set(g);
            }
            match buffers {
                [a] => self.inner.process(&mut [&mut a[start..end]]),
                [a, b, ..] => self.inner.process(&mut [&mut a[start..end], &mut b[start..end]]),
                [] => {}
            }
            start = end;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calf_values_land_on_the_engine() {
        let s = Settings { v: [0.3, 2.0, -0.5, 0.5, 0.5, 1.0, 90.0, 0.0] };
        let t = translate(&s);
        assert_eq!(t.v[fp::MANUAL], 0.5);
        assert_eq!(t.v[fp::LENGTH], 2.5);
        assert!((t.v[fp::FEEDBACK] - (-6.02)).abs() < 0.01);
        assert_eq!(t.v[fp::FB_POLARITY], 1.0);
        assert!((t.v[fp::WET] - (-6.02)).abs() < 0.01);
        assert_eq!(t.v[fp::DRY], 0.0);
    }
}
