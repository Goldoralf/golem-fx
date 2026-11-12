// SPDX-License-Identifier: GPL-3.0-or-later
//! LFOs and the seeded random source.
//!
//! Deterministic (golem contract): phase 0 and the fixed seed on `reset`; a parameter change never
//! touches the phase. Phase accumulators are f64.

use std::f64::consts::TAU;

/// Xorshift64* — small, fast, and the same sequence on every host and OS for a given seed.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
    seed: u64,
}

impl Default for Rng {
    /// A fixed seed (deterministic); call `new` for a chosen one.
    fn default() -> Self {
        Self::new(1)
    }
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let s = if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed };
        Self { state: s, seed: s }
    }

    pub fn reset(&mut self) {
        self.state = self.seed;
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in [0, 1).
    #[inline]
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in [−1, 1).
    #[inline]
    pub fn bipolar(&mut self) -> f32 {
        (self.next_f64() * 2.0 - 1.0) as f32
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Sine,
    Triangle,
    /// Triangle with rounded corners (no corner in pitch when it drives a delay).
    Rounded,
    RampUp,
    RampDown,
    /// Square with smoothed edges.
    Square,
    /// Smoothed sample-and-hold: a new random target each cycle, approached smoothly.
    Random,
}

/// A bipolar LFO (−1 … 1).
#[derive(Clone, Debug)]
pub struct Lfo {
    phase: f64,
    start_phase: f64,
    inc: f64,
    shape: Shape,
    rng: Rng,
    held: f64,
    from: f64,
}

impl Lfo {
    pub fn new(shape: Shape, start_phase: f64, seed: u64) -> Self {
        Self {
            phase: start_phase.rem_euclid(1.0),
            start_phase: start_phase.rem_euclid(1.0),
            inc: 0.0,
            shape,
            rng: Rng::new(seed),
            held: 0.0,
            from: 0.0,
        }
    }

    pub fn set_rate(&mut self, hz: f32, sample_rate: f32) {
        self.inc = hz as f64 / sample_rate as f64;
    }

    pub fn set_shape(&mut self, shape: Shape) {
        self.shape = shape;
    }

    pub fn reset(&mut self) {
        self.phase = self.start_phase;
        self.rng.reset();
        self.held = 0.0;
        self.from = 0.0;
    }

    /// Sets the phase directly (host-synced LFOs).
    pub fn set_phase(&mut self, phase: f64) {
        self.phase = phase.rem_euclid(1.0);
    }

    #[inline]
    pub fn phase(&self) -> f64 {
        self.phase
    }

    /// Value at the current phase, then advances.
    #[inline]
    pub fn next(&mut self) -> f32 {
        let v = self.value_at(self.phase);
        let p = self.phase + self.inc;
        if p >= 1.0 {
            if self.shape == Shape::Random {
                self.from = self.held;
                self.held = self.rng.bipolar() as f64;
            }
            self.phase = p - p.floor();
        } else {
            self.phase = p;
        }
        v
    }

    #[inline]
    fn value_at(&self, p: f64) -> f32 {
        let v = match self.shape {
            Shape::Sine => (TAU * p).sin(),
            Shape::Triangle => tri(p),
            Shape::Rounded => {
                let t = tri(p);
                1.5 * t - 0.5 * t * t * t
            }
            Shape::RampUp => 2.0 * p - 1.0,
            Shape::RampDown => 1.0 - 2.0 * p,
            Shape::Square => (12.0 * (TAU * p).sin()).tanh() / (12.0f64).tanh(),
            Shape::Random => {
                // raised-cosine glide from the previous value to the held one across the cycle
                let w = 0.5 - 0.5 * (std::f64::consts::PI * p).cos();
                self.from + (self.held - self.from) * w
            }
        };
        v as f32
    }
}

#[inline]
fn tri(p: f64) -> f64 {
    // 0 at p = 0, +1 at 0.25, −1 at 0.75 (in phase with the sine)
    let x = (p + 0.25).rem_euclid(1.0);
    1.0 - 4.0 * (x - 0.5).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_after_reset() {
        let mut a = Lfo::new(Shape::Random, 0.0, 42);
        a.set_rate(3.0, 48000.0);
        let first: Vec<f32> = (0..50000).map(|_| a.next()).collect();
        a.reset();
        let second: Vec<f32> = (0..50000).map(|_| a.next()).collect();
        assert_eq!(first, second);
    }

    #[test]
    fn shapes_in_range_and_in_phase() {
        for shape in [Shape::Sine, Shape::Triangle, Shape::Rounded, Shape::RampUp, Shape::RampDown, Shape::Square] {
            let mut l = Lfo::new(shape, 0.0, 1);
            l.set_rate(1.0, 1000.0);
            let v: Vec<f32> = (0..1000).map(|_| l.next()).collect();
            assert!(v.iter().all(|x| x.abs() <= 1.0 + 1e-6), "{shape:?}");
            if matches!(shape, Shape::Sine | Shape::Triangle | Shape::Rounded) {
                assert!((v[250] - 1.0).abs() < 1e-3, "{shape:?} peak {}", v[250]);
                assert!(v[0].abs() < 1e-6);
            }
        }
    }
}
