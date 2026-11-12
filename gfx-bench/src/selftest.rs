// SPDX-License-Identifier: GPL-3.0-or-later
//! Deliberately faulty effects, so the bench proves it catches what it claims to catch
//! (`cargo test -p gfx-bench`).

use gfx_dsp::{Effect, ParamInfo};

static HARD_PARAMS: [ParamInfo; 1] = [ParamInfo::float("gain", "Gain", -60.0, 12.0, 0.0)];

/// A gain with no smoothing: every change is a click.
pub struct HardGain {
    g: f32,
}

impl Default for HardGain {
    fn default() -> Self {
        Self { g: 1.0 }
    }
}

impl Effect for HardGain {
    fn params(&self) -> &'static [ParamInfo] {
        &HARD_PARAMS
    }
    fn prepare(&mut self, _: f32, _: usize, _: usize) {}
    fn reset(&mut self) {}
    fn set_param(&mut self, id: &str, v: f32) {
        if id == "gain" {
            self.g = if v <= -60.0 { 0.0 } else { 10f32.powf(v / 20.0) };
        }
    }
    fn process(&mut self, b: &mut [&mut [f32]]) {
        for ch in b.iter_mut() {
            ch.iter_mut().for_each(|x| *x *= self.g);
        }
    }
}

/// A one-sample delay that reports no latency, and a state that survives reset.
#[derive(Default)]
pub struct Sloppy {
    z: f32,
    drift: f32,
}

static SLOPPY_PARAMS: [ParamInfo; 0] = [];

impl Effect for Sloppy {
    fn params(&self) -> &'static [ParamInfo] {
        &SLOPPY_PARAMS
    }
    fn prepare(&mut self, _: f32, _: usize, _: usize) {}
    fn reset(&mut self) {
        self.z = 0.0;
    }
    fn set_param(&mut self, _: &str, _: f32) {}
    fn process(&mut self, b: &mut [&mut [f32]]) {
        for ch in b.iter_mut() {
            for x in ch.iter_mut() {
                let y = self.z + self.drift;
                self.z = *x;
                self.drift += 1e-7;
                *x = y;
            }
        }
    }
}
