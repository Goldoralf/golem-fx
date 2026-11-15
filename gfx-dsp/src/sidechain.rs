// SPDX-License-Identifier: GPL-3.0-or-later
//! The detector's level, shared by the dynamics blocks that read "one period of the lowest note":
//! the sidechain filters (Butterworth high-pass and low-pass, each off at its end of the range), the
//! sliding-maximum window rule with its divisor, and the phrase level (peak held 5 s, then falling
//! 3 dB / s, never under the absolute floor).
//!
//! - Window rule: W = clamp(1.2 / max(`hpf` / divisor, 30 Hz), 8 ms, 40 ms). gfx-gate's close detector
//!   uses divisor 1 (its hysteresis hides the ripple of a note under the corner); gfx-transient uses 2
//!   (a 12 dB / octave high-pass leaves a note under its corner 7 – 10 dB down with its period intact,
//!   and a shaper's gain would follow it). gfx-gate keeps its own copy of the rule (unchanged).
//! - The peak itself is [`crate::envelope::SlidingMax`] over W: the caller rectifies (and links) first.

use crate::svf::{Svf, SvfMode};
use std::f32::consts::FRAC_1_SQRT_2;

/// The sliding-maximum window (seconds) for a sidechain high-pass at `hpf` Hz (0 = off).
#[inline]
pub fn window_secs(hpf: f32, divisor: f32) -> f32 {
    (1.2 / (hpf / divisor.max(1e-3)).max(30.0)).clamp(0.008, 0.040)
}

/// The longest window [`window_secs`] gives (seconds), to size a `SlidingMax`.
pub const MAX_WINDOW_S: f32 = 0.040;

/// Sidechain filters: Butterworth 2nd-order high-pass (on from 10 Hz) and low-pass (on under 20 kHz).
/// Coefficients are set at control rate; the filters run (and are bypassed) per sample.
#[derive(Clone, Debug, Default)]
pub struct ScFilter {
    hp: Svf,
    lp: Svf,
    hp_on: bool,
    lp_on: bool,
}

impl ScFilter {
    pub const HP_ON_HZ: f32 = 10.0;
    pub const LP_OFF_HZ: f32 = 20000.0;

    pub fn set(&mut self, hpf: f32, lpf: f32, sample_rate: f32) {
        self.hp_on = hpf >= Self::HP_ON_HZ;
        self.lp_on = lpf < Self::LP_OFF_HZ;
        if self.hp_on {
            self.hp.set(SvfMode::HighPass, hpf, FRAC_1_SQRT_2, 0.0, sample_rate);
        }
        if self.lp_on {
            self.lp.set(SvfMode::LowPass, lpf, FRAC_1_SQRT_2, 0.0, sample_rate);
        }
    }

    pub fn reset(&mut self) {
        self.hp.reset();
        self.lp.reset();
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = if self.hp_on { self.hp.process(x) } else { x };
        if self.lp_on {
            self.lp.process(y)
        } else {
            y
        }
    }
}

/// The phrase level, on a linear amplitude (a peak level): held `HOLD_S` after its last peak, then
/// falling `FALL_DB_S`, never under the floor the caller passes (gfx-swell §2.2's rule, on the
/// sliding maximum instead of a follower).
#[derive(Clone, Debug)]
pub struct Phrase {
    value: f64,
    hold: u32,
    hold_n: u32,
    fall: f64,
}

impl Default for Phrase {
    fn default() -> Self {
        Self { value: 0.0, hold: 0, hold_n: 0, fall: 1.0 }
    }
}

impl Phrase {
    pub const HOLD_S: f32 = 5.0;
    pub const FALL_DB_S: f32 = 3.0;

    pub fn prepare(&mut self, sample_rate: f32) {
        self.hold_n = (Self::HOLD_S * sample_rate) as u32;
        self.fall = 10f64.powf(-(Self::FALL_DB_S as f64) / 20.0 / sample_rate as f64);
    }

    pub fn reset(&mut self) {
        self.value = 0.0;
        self.hold = 0;
    }

    /// One sample of the peak level `p`; `floor` (linear) is the absolute floor. Returns the phrase level.
    #[inline]
    pub fn next(&mut self, p: f64, floor: f64) -> f64 {
        if p >= self.value {
            self.value = p;
            self.hold = self.hold_n;
        } else if self.hold > 0 {
            self.hold -= 1;
        } else {
            self.value = (self.value * self.fall).max(p);
        }
        self.value = self.value.max(floor);
        self.value
    }

    pub fn value(&self) -> f64 {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svf::measure_gain_db;

    #[test]
    fn window_rule() {
        // divisor 1: gfx-gate's rule
        assert!((window_secs(0.0, 1.0) - 0.040).abs() < 1e-6);
        assert!((window_secs(120.0, 1.0) - 0.010).abs() < 1e-6);
        assert!((window_secs(1000.0, 1.0) - 0.008).abs() < 1e-6);
        // divisor 2: 40 ms up to 60 Hz, 30 at 80, 20 at 120, 15 at 160, 12 at 200, 8 from 300
        for (hpf, w) in [(0.0, 0.040), (60.0, 0.040), (80.0, 0.030), (120.0, 0.020), (160.0, 0.015), (200.0, 0.012), (300.0, 0.008), (2000.0, 0.008)] {
            assert!((window_secs(hpf, 2.0) - w).abs() < 1e-6, "{hpf}");
        }
    }

    #[test]
    fn filters_off_are_bit_transparent() {
        let mut f = ScFilter::default();
        f.set(0.0, 22000.0, 48000.0);
        for i in 0..100 {
            let x = (i as f32 * 0.37).sin();
            assert_eq!(f.process(x), x);
        }
        let mut f = ScFilter::default();
        f.set(200.0, 22000.0, 48000.0);
        let g = measure_gain_db(|x| f.process(x), 200.0, 48000.0);
        assert!((g + 3.01).abs() < 0.05, "{g}");
    }

    #[test]
    fn phrase_holds_then_falls() {
        let sr = 1000.0;
        let mut p = Phrase::default();
        p.prepare(sr);
        p.next(1.0, 1e-3);
        for _ in 0..4999 {
            assert_eq!(p.next(0.0, 1e-3), 1.0);
        }
        for _ in 0..1000 {
            p.next(0.0, 1e-3);
        }
        let db = 20.0 * p.value().log10();
        assert!((db + 3.0).abs() < 0.02, "{db}");
        for _ in 0..100_000 {
            p.next(0.0, 1e-3);
        }
        assert_eq!(p.value(), 1e-3);
    }
}
