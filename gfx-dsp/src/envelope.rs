// SPDX-License-Identifier: GPL-3.0-or-later
//! Level detectors: peak follower and sliding maximum.

use crate::util::one_pole_coef;

/// Peak follower on |x| with separate attack and release (one-pole each). Attack 0 = instant.
#[derive(Clone, Debug)]
pub struct PeakFollower {
    env: f64,
    att: f64,
    rel: f64,
    att_ms: f32,
    rel_ms: f32,
}

impl PeakFollower {
    pub fn new(attack_ms: f32, release_ms: f32) -> Self {
        Self { env: 0.0, att: 0.0, rel: 0.0, att_ms: attack_ms, rel_ms: release_ms }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.set_times(self.att_ms, self.rel_ms, sample_rate);
    }

    pub fn set_times(&mut self, attack_ms: f32, release_ms: f32, sample_rate: f32) {
        self.att_ms = attack_ms;
        self.rel_ms = release_ms;
        let sr = sample_rate as f64;
        self.att = one_pole_coef(attack_ms as f64 * 0.001 * sr);
        self.rel = one_pole_coef(release_ms as f64 * 0.001 * sr);
    }

    pub fn reset(&mut self) {
        self.env = 0.0;
    }

    #[inline]
    pub fn value(&self) -> f32 {
        self.env as f32
    }

    #[inline]
    pub fn next(&mut self, x: f32) -> f32 {
        let a = (x as f64).abs();
        let c = if a > self.env { self.att } else { self.rel };
        self.env = a + (self.env - a) * c;
        self.env as f32
    }
}

/// Maximum of |x| over the last `window` samples, O(1) amortised (monotonic queue on a fixed ring).
///
/// Used where a follower would ripple between the peaks of a low note: over one period of the lowest
/// content, the maximum is ripple-free by construction (gfx-gate's close detector).
#[derive(Clone, Debug)]
pub struct SlidingMax {
    values: Vec<f32>,
    stamps: Vec<u64>,
    head: usize,
    len: usize,
    window: u64,
    now: u64,
}

impl SlidingMax {
    pub fn new() -> Self {
        Self { values: Vec::new(), stamps: Vec::new(), head: 0, len: 0, window: 1, now: 0 }
    }

    /// Allocates for windows up to `max_window` samples.
    pub fn prepare(&mut self, max_window: usize) {
        let cap = max_window.max(1) + 1;
        self.values = vec![0.0; cap];
        self.stamps = vec![0; cap];
        self.reset();
    }

    pub fn reset(&mut self) {
        self.head = 0;
        self.len = 0;
        self.now = 0;
    }

    /// Window length in samples (clamped to the prepared capacity).
    pub fn set_window(&mut self, window: usize) {
        self.window = window.clamp(1, self.values.len().saturating_sub(1).max(1)) as u64;
    }

    #[inline]
    fn idx(&self, i: usize) -> usize {
        (self.head + i) % self.values.len()
    }

    #[inline]
    pub fn next(&mut self, x: f32) -> f32 {
        let a = x.abs();
        let cap = self.values.len();
        // drop expired entries at the front
        while self.len > 0 && self.stamps[self.head] + self.window <= self.now {
            self.head = (self.head + 1) % cap;
            self.len -= 1;
        }
        // drop smaller-or-equal entries at the back
        while self.len > 0 {
            let back = self.idx(self.len - 1);
            if self.values[back] <= a {
                self.len -= 1;
            } else {
                break;
            }
        }
        let slot = self.idx(self.len);
        self.values[slot] = a;
        self.stamps[slot] = self.now;
        self.len += 1;
        self.now += 1;
        self.values[self.head]
    }
}

impl Default for SlidingMax {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sliding_max_matches_brute_force() {
        let mut m = SlidingMax::new();
        m.prepare(64);
        m.set_window(37);
        let mut seed = 1u64;
        let mut xs = Vec::new();
        for _ in 0..5000 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            xs.push(((seed >> 11) as f64 / (1u64 << 53) as f64) as f32 * 2.0 - 1.0);
        }
        for (i, &x) in xs.iter().enumerate() {
            let got = m.next(x);
            let lo = i.saturating_sub(36);
            let want = xs[lo..=i].iter().map(|v| v.abs()).fold(0.0f32, f32::max);
            assert_eq!(got, want, "at {i}");
        }
    }

    #[test]
    fn sliding_max_has_no_ripple_on_a_low_note() {
        // 31 Hz at 48 kHz, window = 1.2 periods: once filled, the maximum stays at the amplitude.
        let sr = 48000.0f32;
        let f = 31.0f32;
        let mut m = SlidingMax::new();
        m.prepare(4096);
        m.set_window((1.2 * sr / f) as usize);
        let mut min_after = f32::MAX;
        for n in 0..48000 {
            let y = m.next((2.0 * std::f32::consts::PI * f * n as f32 / sr).sin());
            if n > 4000 {
                min_after = min_after.min(y);
            }
        }
        assert!(min_after > 0.999, "{min_after}");
    }

    #[test]
    fn peak_follower_instant_attack() {
        let mut p = PeakFollower::new(0.0, 50.0);
        p.prepare(48000.0);
        assert_eq!(p.next(0.5), 0.5);
        assert!(p.next(0.0) < 0.5);
    }
}
