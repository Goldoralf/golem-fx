// SPDX-License-Identifier: GPL-3.0-or-later
//! The ERB complex filterbank of gfx-octaver's Poly mode, as a shared block (gfx-acoustic §2.4).
//!
//! - Non-decimated, zero latency: one complex one-pole per band, z ← p · z + (1 − r) · x, pole
//!   p = r · e^{jω}, r = e^{−π · ERB / fs}, bandwidth 1 ERB (24.7 · (4.37 f / 1000 + 1) Hz).
//! - 28 bands log-spaced 40 Hz – 5 kHz (gfx-octaver's layout); a user may run only the bands below a
//!   frequency.
//! - **Octave up**: z² / |z| per band (the band's analytic signal at twice its phase, its own
//!   magnitude), summed × 2 / 2.4 (each band carries half of a real component; the bank overlaps
//!   ≈ 2.4×). Doubling every partial keeps a note harmonic. On chords two partials closer than about
//!   one ERB share a band and z² adds their sum tone (gfx-acoustic §2.4).
//! - The arithmetic is gfx-octaver's, statement for statement (gfx-octaver keeps its inline copy until
//!   it is next touched; its renders are not affected by this module).

use std::f64::consts::{PI, TAU};

/// gfx-octaver's layout.
pub const N_BANDS: usize = 28;
pub const LOW_HZ: f64 = 40.0;
pub const HIGH_HZ: f64 = 5000.0;

#[derive(Clone, Copy, Debug, Default)]
struct Band {
    p_re: f64,
    p_im: f64,
    g: f64,
    z_re: f64,
    z_im: f64,
}

/// The ERB bank, limited to the bands centred below `max_hz`.
#[derive(Clone, Debug, Default)]
pub struct ErbBank {
    bands: Vec<Band>,
    freqs: Vec<f64>,
}

impl ErbBank {
    pub fn new() -> Self {
        Self::default()
    }

    /// Centre of band k of the full layout.
    pub fn centre(k: usize) -> f64 {
        LOW_HZ * (HIGH_HZ / LOW_HZ).powf(k as f64 / (N_BANDS - 1) as f64)
    }

    /// Builds the bands centred below `max_hz` (allocates).
    pub fn prepare(&mut self, sample_rate: f32, max_hz: f64) {
        let sr = sample_rate as f64;
        self.bands.clear();
        self.freqs.clear();
        for k in 0..N_BANDS {
            let f = Self::centre(k);
            if f >= max_hz {
                break;
            }
            let erb = 24.7 * (4.37 * f / 1000.0 + 1.0);
            let r = (-PI * erb / sr).exp();
            let w = TAU * f / sr;
            self.bands.push(Band { p_re: r * w.cos(), p_im: r * w.sin(), g: 1.0 - r, ..Default::default() });
            self.freqs.push(f);
        }
    }

    pub fn bands(&self) -> usize {
        self.bands.len()
    }

    pub fn reset(&mut self) {
        for b in self.bands.iter_mut() {
            b.z_re = 0.0;
            b.z_im = 0.0;
        }
    }

    /// One sample in; the octave-up voice out.
    #[inline]
    pub fn octave_up(&mut self, x: f64) -> f64 {
        let mut uu = 0.0f64;
        for b in self.bands.iter_mut() {
            let zr = b.p_re * b.z_re - b.p_im * b.z_im + b.g * x;
            let zi = b.p_re * b.z_im + b.p_im * b.z_re;
            b.z_re = zr;
            b.z_im = zi;
            let m = (zr * zr + zi * zi).sqrt();
            if m < 1e-9 {
                continue;
            }
            uu += (zr * zr - zi * zi) / m;
        }
        uu * 2.0 / 2.4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frequency of a signal from its zero crossings over the second half.
    fn freq(y: &[f64], sr: f64) -> f64 {
        let h = &y[y.len() / 2..];
        let mut cross = Vec::new();
        for i in 1..h.len() {
            if h[i - 1] < 0.0 && h[i] >= 0.0 {
                cross.push(i as f64 - h[i] / (h[i] - h[i - 1]));
            }
        }
        (cross.len() - 1) as f64 * sr / (cross[cross.len() - 1] - cross[0])
    }

    #[test]
    fn layout_is_gfx_octavers() {
        assert!((ErbBank::centre(0) - 40.0).abs() < 1e-9);
        assert!((ErbBank::centre(27) - 5000.0).abs() < 1e-6);
        let mut b = ErbBank::new();
        b.prepare(48000.0, 400.0);
        assert_eq!(b.bands(), 13);
    }

    #[test]
    fn octave_up_doubles_a_sine_within_a_cent() {
        for sr in [44100.0, 48000.0, 96000.0] {
            for f0 in [82.41, 110.0, 196.0] {
                let mut b = ErbBank::new();
                b.prepare(sr as f32, 400.0);
                let n = (sr * 1.0) as usize;
                let y: Vec<f64> = (0..n).map(|i| b.octave_up((TAU * f0 * i as f64 / sr).sin())).collect();
                let got = freq(&y, sr);
                let cents = 1200.0 * (got / (2.0 * f0)).log2();
                assert!(cents.abs() < 1.0, "{sr} {f0}: {got} Hz ({cents:.2} cents)");
                let rms = (y[n / 2..].iter().map(|v| v * v).sum::<f64>() / (n / 2) as f64).sqrt();
                assert!(rms > 0.2 && rms < 1.2, "{f0}: level {rms}");
            }
        }
    }

    #[test]
    fn silence_after_a_burst() {
        let mut b = ErbBank::new();
        b.prepare(48000.0, 400.0);
        for i in 0..4800 {
            b.octave_up((i as f64 * 0.01).sin());
        }
        let mut last = 1.0;
        for _ in 0..48000 {
            last = b.octave_up(0.0);
        }
        assert!(last.abs() < 1e-12);
    }
}
