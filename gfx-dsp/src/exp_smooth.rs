// SPDX-License-Identifier: GPL-3.0-or-later
//! The expression-pedal smoother of the golem contract (docs/golem-contract.md §Expression pedal).
//!
//! golem's EXP values are 7-bit MIDI steps, forwarded at irregular intervals (a few ms during a fast
//! sweep, nothing while the foot rests) and sometimes in bursts. A short one-pole leaves a staircase
//! that buzzes on a resonant filter; a long one lags. So:
//!
//! 1. on each new value, ramp **linearly** from where the ramp is to the new value, over the recent
//!    update interval (median of the last four, clamped to 5 – 30 ms; values arriving < 1 ms apart are a
//!    burst: the ramp is retargeted, its length kept);
//! 2. a slew limit (full range in no less than 15 ms) against 7-bit jumps;
//! 3. two identical one-poles in series (critically damped, 2 ms each) to round the corners.
//!
//! Each step of a steady sweep thus becomes one continuous slope: no staircase, and a step reaches
//! 90 % within about one interval + 5 ms. The output lands exactly on the target once settled.

use crate::util::one_pole_coef;

const RING: usize = 4;

#[derive(Clone, Debug)]
pub struct ExpSmoother {
    min: f64,
    max: f64,
    target: f64,
    ramp: f64,
    ramp_step: f64,
    ramp_left: u32,
    slewed: f64,
    s1: f64,
    s2: f64,
    coef: f64,
    slew: f64,
    intervals: [u32; RING],
    n_intervals: usize,
    next_slot: usize,
    last_update: Option<u64>,
    ramp_len: u32,
    min_len: u32,
    max_len: u32,
    burst: u32,
    snap: f64,
}

impl ExpSmoother {
    /// `min` / `max`: the parameter's plain range (used by the slew limit and the snap distance).
    pub fn new(min: f32, max: f32, initial: f32) -> Self {
        let v = initial as f64;
        Self {
            min: min as f64,
            max: max as f64,
            target: v,
            ramp: v,
            ramp_step: 0.0,
            ramp_left: 0,
            slewed: v,
            s1: v,
            s2: v,
            coef: 0.0,
            slew: f64::INFINITY,
            intervals: [0; RING],
            n_intervals: 0,
            next_slot: 0,
            last_update: None,
            ramp_len: 1,
            min_len: 1,
            max_len: 1,
            burst: 1,
            snap: ((max - min).abs() as f64 * 1e-7).max(1e-12),
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        let sr = sample_rate as f64;
        self.coef = one_pole_coef(0.002 * sr);
        self.slew = (self.max - self.min).abs() / (0.015 * sr);
        self.min_len = (0.005 * sr).round() as u32;
        self.max_len = (0.030 * sr).round() as u32;
        self.burst = (0.001 * sr).round().max(1.0) as u32;
        self.ramp_len = (0.015 * sr).round() as u32;
    }

    /// A new value from the host, `now` = the effect's sample clock (samples processed since reset).
    pub fn set_target(&mut self, value: f32, now: u64) {
        let value = value as f64;
        if let Some(last) = self.last_update {
            let interval = now.saturating_sub(last).min(u32::MAX as u64) as u32;
            if interval >= self.burst {
                self.intervals[self.next_slot] = interval;
                self.next_slot = (self.next_slot + 1) % RING;
                self.n_intervals = (self.n_intervals + 1).min(RING);
                self.ramp_len = self.median().clamp(self.min_len, self.max_len);
            }
        }
        self.last_update = Some(now);
        if value == self.target {
            return;
        }
        self.target = value;
        let len = self.ramp_len.max(1);
        self.ramp_left = len;
        self.ramp_step = (self.target - self.ramp) / len as f64;
    }

    /// Jumps to a value (reset, preset recall).
    pub fn settle(&mut self, value: f32) {
        let v = value as f64;
        self.target = v;
        self.ramp = v;
        self.ramp_left = 0;
        self.slewed = v;
        self.s1 = v;
        self.s2 = v;
    }

    /// Settles on the current target and forgets the update history.
    pub fn reset(&mut self) {
        self.settle(self.target as f32);
        self.n_intervals = 0;
        self.next_slot = 0;
        self.last_update = None;
    }

    #[inline]
    pub fn target(&self) -> f32 {
        self.target as f32
    }

    #[inline]
    pub fn value(&self) -> f32 {
        self.s2 as f32
    }

    #[inline]
    pub fn is_settled(&self) -> bool {
        self.ramp_left == 0 && self.s2 == self.target
    }

    #[inline]
    pub fn next(&mut self) -> f32 {
        if self.is_settled() {
            return self.s2 as f32;
        }
        if self.ramp_left > 0 {
            self.ramp_left -= 1;
            self.ramp = if self.ramp_left == 0 { self.target } else { self.ramp + self.ramp_step };
        }
        let d = (self.ramp - self.slewed).clamp(-self.slew, self.slew);
        self.slewed += d;
        self.s1 = self.slewed + (self.s1 - self.slewed) * self.coef;
        self.s2 = self.s1 + (self.s2 - self.s1) * self.coef;
        if self.ramp_left == 0
            && (self.slewed - self.target).abs() <= self.snap
            && (self.s2 - self.target).abs() <= self.snap
        {
            self.slewed = self.target;
            self.s1 = self.target;
            self.s2 = self.target;
        }
        self.s2 as f32
    }

    fn median(&self) -> u32 {
        let n = self.n_intervals;
        let mut v = [0u32; RING];
        v[..n].copy_from_slice(&self.intervals[..n]);
        v[..n].sort_unstable();
        if n % 2 == 1 {
            v[n / 2]
        } else {
            ((v[n / 2 - 1] as u64 + v[n / 2] as u64) / 2) as u32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48000.0;

    /// A 128-step pedal sweep, one 7-bit step every `interval` samples.
    fn sweep(interval: u64) -> Vec<f32> {
        let mut s = ExpSmoother::new(0.0, 1.0, 0.0);
        s.prepare(SR);
        let mut out = Vec::new();
        let mut now = 0u64;
        for step in 1..=127u32 {
            s.set_target(step as f32 / 127.0, now);
            for _ in 0..interval {
                out.push(s.next());
                now += 1;
            }
        }
        for _ in 0..4800 {
            out.push(s.next());
        }
        out
    }

    #[test]
    fn lands_exactly() {
        let out = sweep(480);
        assert_eq!(*out.last().unwrap(), 1.0);
    }

    #[test]
    fn steady_sweep_has_no_staircase() {
        // 10 ms between steps: after the first few steps the slope must be constant within a few %.
        let out = sweep(480);
        let slope: Vec<f32> = out.windows(2).map(|w| w[1] - w[0]).collect();
        let mid = &slope[480 * 20..480 * 100];
        let max = mid.iter().cloned().fold(f32::MIN, f32::max);
        let min = mid.iter().cloned().fold(f32::MAX, f32::min);
        assert!(min > 0.0, "the value went backwards");
        assert!((max - min) / max < 0.05, "slope ripple {:.3}", (max - min) / max);
    }

    #[test]
    fn step_reaches_90_percent_within_interval_plus_5ms() {
        let mut s = ExpSmoother::new(0.0, 1.0, 0.0);
        s.prepare(SR);
        // teach it a 15 ms interval
        let mut now = 0u64;
        for k in 1..=4 {
            s.set_target(k as f32 * 0.01, now);
            for _ in 0..720 {
                s.next();
                now += 1;
            }
        }
        let start = s.value();
        s.set_target(1.0, now);
        let goal = start + 0.9 * (1.0 - start);
        let mut n = 0;
        while s.next() < goal {
            n += 1;
            assert!(n < 100_000);
        }
        let limit = 720 + 240;
        assert!(n <= limit, "{n} samples > {limit}");
    }

    #[test]
    fn burst_keeps_ramp_length() {
        let mut s = ExpSmoother::new(0.0, 1.0, 0.0);
        s.prepare(SR);
        s.set_target(0.1, 0);
        s.set_target(0.2, 960);
        let len = s.ramp_len;
        // three values in the same instant (REAPER batch): length unchanged, last value wins
        s.set_target(0.3, 1920);
        s.set_target(0.4, 1920);
        s.set_target(0.5, 1920);
        assert_eq!(s.target(), 0.5);
        assert!(s.ramp_len >= len.min(s.max_len));
    }

    #[test]
    fn slew_limits_a_full_jump() {
        let mut s = ExpSmoother::new(0.0, 1.0, 0.0);
        s.prepare(SR);
        s.set_target(1.0, 0);
        let max_step = (0..2000).map(|_| {
            let a = s.value();
            let b = s.next();
            b - a
        });
        let m = max_step.fold(0.0f32, f32::max);
        assert!(m <= 1.0 / (0.015 * SR) + 1e-6);
    }
}
