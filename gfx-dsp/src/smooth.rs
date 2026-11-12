// SPDX-License-Identifier: GPL-3.0-or-later
//! Parameter smoothers for values set by the host (not the expression pedal: see `exp_smooth`).
//!
//! Both snap exactly onto their target once they are within a tiny distance of it, so a parameter
//! that returns to a neutral value (0 dB, 0 %) lands on it bit for bit — what lets an effect be
//! bit-transparent at neutral settings.

use crate::util::one_pole_coef;

/// One-pole smoother (exponential approach), with exact snap onto the target.
#[derive(Clone, Debug)]
pub struct OnePole {
    value: f64,
    target: f64,
    coef: f64,
    time_ms: f64,
    snap: f64,
}

impl OnePole {
    /// `time_ms`: time constant (63 % of a step). `snap`: distance under which the value jumps
    /// onto the target.
    pub fn new(time_ms: f32, initial: f32, snap: f32) -> Self {
        Self {
            value: initial as f64,
            target: initial as f64,
            coef: 0.0,
            time_ms: time_ms as f64,
            snap: snap as f64,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.coef = one_pole_coef(self.time_ms * 0.001 * sample_rate as f64);
    }

    pub fn set_target(&mut self, target: f32) {
        self.target = target as f64;
    }

    /// Changes the time constant (no allocation; real-time safe).
    pub fn set_time(&mut self, time_ms: f32, sample_rate: f32) {
        self.time_ms = time_ms as f64;
        self.prepare(sample_rate);
    }

    /// Jumps to the target (reset, first value).
    pub fn settle(&mut self) {
        self.value = self.target;
    }

    pub fn set_immediate(&mut self, value: f32) {
        self.target = value as f64;
        self.value = self.target;
    }

    #[inline]
    pub fn is_settled(&self) -> bool {
        self.value == self.target
    }

    #[inline]
    pub fn target(&self) -> f32 {
        self.target as f32
    }

    #[inline]
    pub fn value(&self) -> f32 {
        self.value as f32
    }

    #[inline]
    pub fn next(&mut self) -> f32 {
        if self.value != self.target {
            self.value = self.target + (self.value - self.target) * self.coef;
            if (self.value - self.target).abs() <= self.snap {
                self.value = self.target;
            }
        }
        self.value as f32
    }
}

/// Linear ramp of fixed duration towards each new target (crossfades, structural changes).
#[derive(Clone, Debug)]
pub struct LinearRamp {
    value: f64,
    target: f64,
    step: f64,
    remaining: u32,
    time_ms: f64,
    length: u32,
}

impl LinearRamp {
    pub fn new(time_ms: f32, initial: f32) -> Self {
        Self {
            value: initial as f64,
            target: initial as f64,
            step: 0.0,
            remaining: 0,
            time_ms: time_ms as f64,
            length: 1,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.length = ((self.time_ms * 0.001 * sample_rate as f64).round() as u32).max(1);
    }

    pub fn set_target(&mut self, target: f32) {
        let target = target as f64;
        if target == self.target {
            return;
        }
        self.target = target;
        self.remaining = self.length;
        self.step = (self.target - self.value) / self.length as f64;
    }

    pub fn settle(&mut self) {
        self.value = self.target;
        self.remaining = 0;
    }

    pub fn set_immediate(&mut self, value: f32) {
        self.target = value as f64;
        self.settle();
    }

    #[inline]
    pub fn is_settled(&self) -> bool {
        self.remaining == 0
    }

    #[inline]
    pub fn value(&self) -> f32 {
        self.value as f32
    }

    #[inline]
    pub fn next(&mut self) -> f32 {
        if self.remaining > 0 {
            self.remaining -= 1;
            self.value = if self.remaining == 0 { self.target } else { self.value + self.step };
        }
        self.value as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_pole_lands_exactly() {
        let mut s = OnePole::new(10.0, -60.0, 1e-6);
        s.prepare(48000.0);
        s.set_target(0.0);
        let mut last = 0.0;
        for _ in 0..48000 {
            last = s.next();
        }
        assert_eq!(last, 0.0);
        assert!(s.is_settled());
    }

    #[test]
    fn one_pole_time_constant() {
        let mut s = OnePole::new(10.0, 0.0, 1e-9);
        s.prepare(48000.0);
        s.set_target(1.0);
        let mut v = 0.0;
        for _ in 0..480 {
            v = s.next();
        }
        assert!((v - 0.632).abs() < 0.01, "{v}");
    }

    #[test]
    fn ramp_is_exact_and_linear() {
        let mut r = LinearRamp::new(1.0, 0.0);
        r.prepare(48000.0);
        r.set_target(1.0);
        let first = r.next();
        assert!((first - 1.0 / 48.0).abs() < 1e-6);
        for _ in 0..47 {
            r.next();
        }
        assert_eq!(r.value(), 1.0);
        assert!(r.is_settled());
    }
}
