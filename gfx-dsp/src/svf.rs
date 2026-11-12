// SPDX-License-Identifier: GPL-3.0-or-later
//! Trapezoidal (TPT) state-variable filter — Simper, "SvfLinearTrapOptimised2" (Cytomic) /
//! Zavalishin, "The Art of VA Filter Design". Stable and click-free when its coefficients move at
//! audio rate; every response (low / band / high-pass, bell, shelves, notch) from one structure by
//! mixing its outputs. State in f64.
//!
//! Frequencies are prewarped (g = tan(π f / fs)): the analog magnitude is exact at the cut-off. Above
//! fs / 4 the response still "cramps" like any bilinear design; the matched designs (Vicanek,
//! Orfanidis) that fix it are added with gfx-eq.

use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SvfMode {
    LowPass,
    HighPass,
    /// Band-pass with unity gain at the centre.
    BandPass,
    Notch,
    /// `gain_db` at the centre.
    Bell,
    LowShelf,
    HighShelf,
    AllPass,
}

#[derive(Clone, Copy, Debug, Default)]
struct Coefs {
    a1: f64,
    a2: f64,
    a3: f64,
    m0: f64,
    m1: f64,
    m2: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Svf {
    c: Coefs,
    ic1: f64,
    ic2: f64,
}

impl Svf {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the response. `freq` in Hz (clamped below Nyquist), `q` > 0, `gain_db` used by bell and
    /// shelves.
    pub fn set(&mut self, mode: SvfMode, freq: f32, q: f32, gain_db: f32, sample_rate: f32) {
        let sr = sample_rate as f64;
        let f = (freq as f64).clamp(1.0, 0.49 * sr);
        let q = (q as f64).max(1e-3);
        let a = 10f64.powf(gain_db as f64 / 40.0);
        let mut g = (PI * f / sr).tan();
        let mut k = 1.0 / q;
        let (m0, m1, m2);
        match mode {
            SvfMode::LowPass => {
                m0 = 0.0;
                m1 = 0.0;
                m2 = 1.0;
            }
            SvfMode::HighPass => {
                m0 = 1.0;
                m1 = -k;
                m2 = -1.0;
            }
            SvfMode::BandPass => {
                m0 = 0.0;
                m1 = k;
                m2 = 0.0;
            }
            SvfMode::Notch => {
                m0 = 1.0;
                m1 = -k;
                m2 = 0.0;
            }
            SvfMode::AllPass => {
                m0 = 1.0;
                m1 = -2.0 * k;
                m2 = 0.0;
            }
            SvfMode::Bell => {
                k = 1.0 / (q * a);
                m0 = 1.0;
                m1 = k * (a * a - 1.0);
                m2 = 0.0;
            }
            SvfMode::LowShelf => {
                g /= a.sqrt();
                m0 = 1.0;
                m1 = k * (a - 1.0);
                m2 = a * a - 1.0;
            }
            SvfMode::HighShelf => {
                g *= a.sqrt();
                m0 = a * a;
                m1 = k * (1.0 - a) * a;
                m2 = 1.0 - a * a;
            }
        }
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        self.c = Coefs { a1, a2, a3, m0, m1, m2 };
    }

    pub fn reset(&mut self) {
        self.ic1 = 0.0;
        self.ic2 = 0.0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let c = &self.c;
        let v0 = x as f64;
        let v3 = v0 - self.ic2;
        let v1 = c.a1 * self.ic1 + c.a2 * v3;
        let v2 = self.ic2 + c.a2 * self.ic1 + c.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (c.m0 * v0 + c.m1 * v1 + c.m2 * v2) as f32
    }
}

/// Magnitude of a filter's response at `freq`, measured by running a sine through it (test helper,
/// also used by gfx-bench). RMS ratio over the second half, so it holds near Nyquist where the
/// sampled peaks miss the true amplitude.
pub fn measure_gain_db(mut f: impl FnMut(f32) -> f32, freq: f32, sample_rate: f32) -> f32 {
    let n = (sample_rate as usize).max(4096);
    let w = 2.0 * std::f64::consts::PI * freq as f64 / sample_rate as f64;
    let (mut e_in, mut e_out) = (0.0f64, 0.0f64);
    for i in 0..n {
        let x = (w * i as f64).sin() as f32;
        let y = f(x);
        if i > n / 2 {
            e_in += (x as f64) * (x as f64);
            e_out += (y as f64) * (y as f64);
        }
    }
    (10.0 * (e_out.max(1e-30) / e_in.max(1e-30)).log10()) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48000.0;

    fn gain(mode: SvfMode, fc: f32, q: f32, db: f32, at: f32) -> f32 {
        let mut s = Svf::new();
        s.set(mode, fc, q, db, SR);
        measure_gain_db(|x| s.process(x), at, SR)
    }

    #[test]
    fn butterworth_corners() {
        let q = std::f32::consts::FRAC_1_SQRT_2;
        assert!((gain(SvfMode::LowPass, 1000.0, q, 0.0, 1000.0) + 3.01).abs() < 0.05);
        assert!((gain(SvfMode::HighPass, 1000.0, q, 0.0, 1000.0) + 3.01).abs() < 0.05);
        assert!(gain(SvfMode::LowPass, 1000.0, q, 0.0, 100.0).abs() < 0.05);
        assert!(gain(SvfMode::HighPass, 1000.0, q, 0.0, 10000.0).abs() < 0.1);
    }

    #[test]
    fn bell_and_shelves() {
        assert!((gain(SvfMode::Bell, 1000.0, 1.0, 6.0, 1000.0) - 6.0).abs() < 0.05);
        assert!((gain(SvfMode::Bell, 1000.0, 1.0, -9.0, 1000.0) + 9.0).abs() < 0.05);
        assert!((gain(SvfMode::LowShelf, 200.0, 0.707, 6.0, 30.0) - 6.0).abs() < 0.2);
        assert!((gain(SvfMode::HighShelf, 4000.0, 0.707, -6.0, 16000.0) + 6.0).abs() < 0.3);
        assert!(gain(SvfMode::AllPass, 1000.0, 0.707, 0.0, 3000.0).abs() < 0.05);
    }

    #[test]
    fn bandpass_unity_at_centre() {
        assert!(gain(SvfMode::BandPass, 800.0, 4.0, 0.0, 800.0).abs() < 0.05);
    }
}
