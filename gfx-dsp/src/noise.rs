// SPDX-License-Identifier: GPL-3.0-or-later
//! Seeded white noise (gfx-vocoder's carrier and noise bank; gfx-polysynth's breath).
//!
//! An xorshift generator (the suite's [`crate::lfo::Rng`]) mapped to a uniform value in [−1, 1).
//! `reset` restores the seed, so two renders are bit-identical (golem contract).

use crate::lfo::Rng;

/// The RMS of [`White`]'s output: 1 / √3 (uniform on [−1, 1)).
pub const WHITE_RMS: f64 = 0.577_350_269_189_625_8;

/// White noise, uniform on [−1, 1), from a fixed seed.
#[derive(Clone, Debug)]
pub struct White {
    rng: Rng,
}

impl White {
    pub fn new(seed: u64) -> Self {
        Self { rng: Rng::new(seed) }
    }

    /// Back to the seed.
    pub fn reset(&mut self) {
        self.rng.reset();
    }

    /// The next sample, uniform on [−1, 1).
    #[inline]
    pub fn next(&mut self) -> f32 {
        self.rng.bipolar()
    }

    /// The next sample in f64, uniform on [−1, 1).
    #[inline]
    pub fn next_f64(&mut self) -> f64 {
        self.rng.next_f64() * 2.0 - 1.0
    }
}

impl Default for White {
    fn default() -> Self {
        Self::new(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_is_seeded_bounded_and_flat() {
        let mut a = White::new(7);
        let x: Vec<f64> = (0..200_000).map(|_| a.next_f64()).collect();
        assert!(x.iter().all(|v| (-1.0..1.0).contains(v)));
        let mean = x.iter().sum::<f64>() / x.len() as f64;
        let rms = (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64).sqrt();
        assert!(mean.abs() < 0.01, "{mean}");
        assert!((rms / WHITE_RMS - 1.0).abs() < 0.01, "{rms}");
        // lag-1 correlation ≈ 0: white
        let c1 = x.windows(2).map(|w| w[0] * w[1]).sum::<f64>() / x.len() as f64 / (rms * rms);
        assert!(c1.abs() < 0.01, "{c1}");
        a.reset();
        let y: Vec<f64> = (0..100).map(|_| a.next_f64()).collect();
        assert_eq!(&x[..100], &y[..]);
    }
}
