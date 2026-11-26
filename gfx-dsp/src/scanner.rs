// SPDX-License-Identifier: GPL-3.0-or-later
//! The Hammond chorus-vibrato scanner (gfx-vibrato's Scanner model, extracted unchanged so that
//! gfx-organ plays the same one: gfx-organ README §2.5).
//!
//! The line box is 19 LC sections 55 µs apart, read on the caller's delay line; the rotating
//! capacitor crossfades two adjacent taps. `depth` sweeps the reach continuously: V1 ≈ 0.47 (tap 9),
//! V2 ≈ 0.68 (tap 13), V3 = 1 (tap 19). The taps are Lagrange-3 reads; a 7 kHz low-pass follows.
//! Chorus = vibrato + dry, which the caller mixes. A bit-identical null of gfx-vibrato's Scanner
//! renders guards the extraction (gfx-vibrato `tests.rs`).

use crate::delay::{DelayLine, Interp};
use crate::svf::{Svf, SvfMode};

/// One ladder section (s).
pub const STEP: f64 = 55e-6;
/// Sections of the line box.
pub const TAPS: usize = 19;
/// The reach of V1, V2, V3 as a `depth` (tap 9, 13, 19 of 19).
pub const V_DEPTH: [f64; 3] = [9.0 / 19.0, 13.0 / 19.0, 1.0];

/// The scanner's read and its low-pass (one per channel).
#[derive(Clone, Debug, Default)]
pub struct Scanner {
    lp: Svf,
}

impl Scanner {
    pub fn new() -> Self {
        Self { lp: Svf::new() }
    }

    /// The low-pass at `sample_rate` (7 kHz, under 0.45 × the rate).
    pub fn prepare(&mut self, sample_rate: f32) {
        self.lp.set(SvfMode::LowPass, 7000.0f32.min(0.45 * sample_rate), std::f32::consts::FRAC_1_SQRT_2, 0.0, sample_rate);
    }

    pub fn reset(&mut self) {
        self.lp.reset();
    }

    /// One sample read on `line` (whose newest sample is the input): `depth` 0 – 1 the reach, `w`
    /// the bipolar scan position, `guard` (s) the line's minimum read, `sr` the rate.
    #[inline]
    pub fn process(&mut self, line: &DelayLine, depth: f64, w: f64, guard: f64, sr: f64) -> f32 {
        let pos = TAPS as f64 * depth * (0.5 + 0.5 * w);
        let i = pos.floor() as usize;
        let f = (pos - i as f64) as f32;
        let tap = |k: usize| line.read((k.min(TAPS) as f64 * STEP + guard) * sr, Interp::Lagrange3);
        let y = tap(i) + f * (tap(i + 1) - tap(i));
        self.lp.process(y)
    }
}
