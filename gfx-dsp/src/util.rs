// SPDX-License-Identifier: GPL-3.0-or-later
//! Small helpers: decibels, time conversions, flush-to-zero.

/// dB → linear gain.
#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db * 0.05)
}

/// Linear gain → dB, floored at −200 dB.
#[inline]
pub fn gain_to_db(gain: f32) -> f32 {
    if gain <= 1e-10 {
        -200.0
    } else {
        20.0 * gain.log10()
    }
}

/// Milliseconds → samples.
#[inline]
pub fn ms_to_samples(ms: f32, sample_rate: f32) -> f32 {
    ms * 0.001 * sample_rate
}

/// One-pole coefficient for a time constant in samples (63 % of a step after `samples`).
#[inline]
pub fn one_pole_coef(samples: f64) -> f64 {
    if samples <= 0.0 {
        0.0
    } else {
        (-1.0 / samples).exp()
    }
}

/// Puts the current thread's FPU in flush-to-zero / denormals-are-zero mode for its lifetime and
/// restores the previous mode on drop. nih-plug does this for plugins; gfx-bench uses it so that
/// what it measures matches what runs in a host.
pub struct FtzGuard {
    #[cfg(target_arch = "x86_64")]
    previous: u32,
}

impl FtzGuard {
    #[allow(deprecated)]
    pub fn new() -> Self {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            use std::arch::x86_64::{_mm_getcsr, _mm_setcsr};
            let previous = _mm_getcsr();
            // FTZ (bit 15) + DAZ (bit 6)
            _mm_setcsr(previous | 0x8040);
            Self { previous }
        }
        #[cfg(not(target_arch = "x86_64"))]
        Self {}
    }
}

impl Default for FtzGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for FtzGuard {
    #[allow(deprecated)]
    fn drop(&mut self) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            std::arch::x86_64::_mm_setcsr(self.previous);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_round_trip() {
        for db in [-60.0f32, -6.0, 0.0, 6.0, 12.0] {
            assert!((gain_to_db(db_to_gain(db)) - db).abs() < 1e-4);
        }
        assert_eq!(db_to_gain(0.0), 1.0);
    }

    #[test]
    fn one_pole_reaches_63_percent() {
        let a = one_pole_coef(100.0);
        let mut y = 0.0;
        for _ in 0..100 {
            y = a * y + (1.0 - a) * 1.0;
        }
        assert!((y - (1.0 - (-1.0f64).exp())).abs() < 1e-3);
    }
}
