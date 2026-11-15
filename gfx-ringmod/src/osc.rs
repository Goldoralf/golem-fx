// SPDX-License-Identifier: GPL-3.0-or-later
//! The carrier's band-limited wavetables (README §2, `wave`): Triangle and Square, one table per octave
//! from 20 Hz, each holding the harmonics that stay at or under 20 kHz at the top of its octave,
//! 4,096 points (at least 4 × the top harmonic: the lowest table holds 500), read with a cubic Hermite
//! and crossfaded between neighbouring octaves. Under 20 Hz the lowest table is used, so a square's
//! edges soften as the carrier slows: a click-free chop. The Sine is computed (`sin`), not tabled.
//!
//! Every wave is in phase with sin(2π φ): zero at φ = 0, rising.

use std::f64::consts::PI;

pub const OCTAVES: usize = 10;
pub const LOW_HZ: f64 = 20.0;
const TOP_HZ: f64 = 20000.0;
const LEN: usize = 4096;

/// `wave` options (enum order, final; append only).
pub const WAVES: [&str; 3] = ["Sine", "Triangle", "Square"];

#[derive(Clone, Debug, Default)]
pub struct Tables {
    /// [wave − 1][octave] → LEN + 3 points (one before, two after, for the Hermite read)
    t: Vec<Vec<f32>>,
}

impl Tables {
    /// Builds the 2 × 10 tables (allocates: `prepare` only). Independent of the sample rate.
    pub fn build() -> Self {
        let mut t = Vec::with_capacity(2 * OCTAVES);
        for wave in 1..=2usize {
            for o in 0..OCTAVES {
                let top = LOW_HZ * 2f64.powi(o as i32 + 1);
                let n = ((TOP_HZ / top).floor() as usize).max(1);
                let mut v = vec![0.0f64; LEN];
                for k in (1..=n).step_by(2) {
                    let a = if wave == 1 {
                        // triangle: 8 / π² · (−1)^((k − 1) / 2) / k²
                        let s = if (k / 2) % 2 == 0 { 1.0 } else { -1.0 };
                        8.0 / (PI * PI) * s / (k * k) as f64
                    } else {
                        // square: 4 / π / k
                        4.0 / PI / k as f64
                    };
                    let w = 2.0 * PI * k as f64 / LEN as f64;
                    for (i, s) in v.iter_mut().enumerate() {
                        *s += a * (w * i as f64).sin();
                    }
                }
                let mut g = Vec::with_capacity(LEN + 3);
                g.push(v[LEN - 1] as f32);
                g.extend(v.iter().map(|&x| x as f32));
                g.push(v[0] as f32);
                g.push(v[1] as f32);
                t.push(g);
            }
        }
        Self { t }
    }

    pub fn is_built(&self) -> bool {
        !self.t.is_empty()
    }

    #[inline]
    fn read(&self, table: usize, phase: f64) -> f32 {
        Self::hermite(&self.t[table], phase)
    }

    /// sin(2π φ), any φ (f32 after the wrap: products' errors under −120 dB).
    #[inline]
    pub fn sine(&self, phase: f64) -> f32 {
        let w = phase - phase.floor();
        (std::f32::consts::TAU * w as f32).sin()
    }

    #[inline]
    fn hermite(p: &[f32], phase: f64) -> f32 {
        let x = phase * LEN as f64;
        let i = x.floor() as usize % LEN;
        let f = (x - x.floor()) as f32;
        // points i − 1 … i + 2, shifted by the guard
        let (xm1, x0, x1, x2) = (p[i], p[i + 1], p[i + 2], p[i + 3]);
        let c1 = 0.5 * (x1 - xm1);
        let c2 = xm1 - 2.5 * x0 + 2.0 * x1 - 0.5 * x2;
        let c3 = 0.5 * (x2 - xm1) + 1.5 * (x0 - x1);
        ((c3 * f + c2) * f + c1) * f + x0
    }

    /// The octave position of a carrier frequency: (table, crossfade towards the next).
    #[inline]
    pub fn octave(hz: f64) -> (usize, f32) {
        let p = (hz.abs().max(LOW_HZ) / LOW_HZ).log2().min((OCTAVES - 1) as f64);
        let j = (p.floor() as usize).min(OCTAVES - 1);
        (j, (p - j as f64) as f32)
    }

    /// The wave `wave` (0 Sine, 1 Triangle, 2 Square) at `phase` (turns), band-limited for the
    /// octave position `oct`.
    #[inline]
    pub fn value(&self, wave: usize, phase: f64, oct: (usize, f32)) -> f32 {
        if wave == 0 {
            return self.sine(phase);
        }
        let ph = phase.rem_euclid(1.0);
        let base = (wave - 1) * OCTAVES;
        let (j, fr) = oct;
        let a = self.read(base + j, ph);
        if fr <= 0.0 || j + 1 >= OCTAVES {
            a
        } else {
            a + (self.read(base + j + 1, ph) - a) * fr
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_hold_their_shapes_and_phase() {
        let t = Tables::build();
        // the lowest square's plateau is near ±1 (Gibbs aside), the triangle peaks at ±1 at φ = ¼, ¾
        let sq = t.value(2, 0.25, (0, 0.0));
        assert!((sq - 1.0).abs() < 0.02, "square plateau {sq}");
        let tr = t.value(1, 0.25, (0, 0.0));
        assert!((tr - 1.0).abs() < 0.01, "triangle peak {tr}");
        let tr0 = t.value(1, 0.0, (0, 0.0));
        assert!(tr0.abs() < 1e-3);
        // the top octave holds the fundamental only: a sine
        let s = t.value(2, 0.1, (OCTAVES - 1, 0.0));
        assert!((s as f64 - 4.0 / PI * (0.2 * PI).sin()).abs() < 1e-4);
    }

    #[test]
    fn octave_position() {
        assert_eq!(Tables::octave(10.0), (0, 0.0));
        let (j, f) = Tables::octave(80.0);
        assert_eq!(j, 2);
        assert!(f.abs() < 1e-6);
        assert_eq!(Tables::octave(30000.0).0, OCTAVES - 1);
    }
}
