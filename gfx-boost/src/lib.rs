// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-boost — golem's gain utility (design: README.md).
//!
//! Stands for golem's `volume` (−60 … +6 dB) and `clean-boost` (0 … +12 dB) blocks, which share the id
//! `gain`. A gain stage has no sound of its own: it is bit-transparent at 0 dB, truly silent at the
//! bottom of its range, and never zippers under the expression pedal (the gain is smoothed **in dB**
//! by the contract's EXP smoother, then converted per sample).

use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::smooth::LinearRamp;
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
pub mod plugin;

pub const GAIN_MIN_DB: f32 = -60.0;
pub const GAIN_MAX_DB: f32 = 12.0;

pub static PARAMS: [ParamInfo; 2] = [
    ParamInfo::float("gain", "Gain", GAIN_MIN_DB, GAIN_MAX_DB, 0.0).with_exp(),
    ParamInfo::boolean("polarity", "Polarity", false),
];

/// dB → gain with the range's two exact points: 0 dB is exactly 1.0, the bottom is exactly 0.0.
#[inline]
pub fn gain_of_db(db: f32) -> f32 {
    if db <= GAIN_MIN_DB {
        0.0
    } else if db == 0.0 {
        1.0
    } else {
        10f32.powf(db * 0.05)
    }
}

pub struct Boost {
    gain_db: ExpSmoother,
    /// +1 normal, −1 inverted; crossfaded over 5 ms.
    polarity: LinearRamp,
    clock: u64,
}

impl Boost {
    pub fn new() -> Self {
        Self {
            gain_db: ExpSmoother::new(GAIN_MIN_DB, GAIN_MAX_DB, 0.0),
            polarity: LinearRamp::new(5.0, 1.0),
            clock: 0,
        }
    }

    pub fn set_gain_db(&mut self, db: f32) {
        self.gain_db.set_target(db.clamp(GAIN_MIN_DB, GAIN_MAX_DB), self.clock);
    }

    pub fn set_polarity(&mut self, inverted: bool) {
        self.polarity.set_target(if inverted { -1.0 } else { 1.0 });
    }
}

impl Default for Boost {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Boost {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.gain_db.prepare(sample_rate);
        self.polarity.prepare(sample_rate);
    }

    fn reset(&mut self) {
        self.gain_db.reset();
        self.polarity.settle();
        self.clock = 0;
    }

    fn set_param(&mut self, id: &str, value: f32) {
        match id {
            "gain" => self.set_gain_db(value),
            "polarity" => self.set_polarity(value >= 0.5),
            _ => {}
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        let n = buffers.first().map_or(0, |b| b.len());
        if self.gain_db.is_settled() && self.polarity.is_settled() {
            let g = gain_of_db(self.gain_db.value()) * self.polarity.value();
            if g != 1.0 {
                for ch in buffers.iter_mut() {
                    ch.iter_mut().for_each(|x| *x *= g);
                }
            }
        } else {
            for i in 0..n {
                let g = gain_of_db(self.gain_db.next()) * self.polarity.next();
                for ch in buffers.iter_mut() {
                    ch[i] *= g;
                }
            }
        }
        self.clock += n as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(b: &mut Boost, x: &[f32]) -> Vec<f32> {
        let mut buf = x.to_vec();
        b.process(&mut [&mut buf[..]]);
        buf
    }

    #[test]
    fn zero_db_is_bit_transparent() {
        let mut b = Boost::new();
        b.prepare(48000.0, 512, 1);
        b.reset();
        let x: Vec<f32> = (0..512).map(|i| ((i * 7919) % 1000) as f32 / 1000.0 - 0.5).collect();
        assert_eq!(run(&mut b, &x), x);
    }

    #[test]
    fn bottom_is_silence_and_return_is_exact() {
        let mut b = Boost::new();
        b.prepare(48000.0, 4800, 1);
        b.reset();
        let x = vec![0.5f32; 4800];
        b.set_gain_db(-60.0);
        for _ in 0..20 {
            run(&mut b, &x);
        }
        assert!(run(&mut b, &x).iter().all(|&y| y == 0.0));
        b.set_gain_db(0.0);
        for _ in 0..20 {
            run(&mut b, &x);
        }
        assert_eq!(run(&mut b, &x), x);
    }

    #[test]
    fn settled_gain_is_exact() {
        let mut b = Boost::new();
        b.prepare(48000.0, 4800, 1);
        b.reset();
        b.set_gain_db(6.0);
        let x = vec![0.25f32; 4800];
        for _ in 0..20 {
            run(&mut b, &x);
        }
        let y = run(&mut b, &x);
        let want = 0.25 * 10f32.powf(0.3);
        assert!((y[100] - want).abs() < 1e-6);
    }
}
