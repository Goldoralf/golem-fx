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
    /// coefficient ramp (set_biquad_ramped): per-sample step and samples left
    step: Coefs,
    left: u32,
}

impl Coefs {
    fn from_biquad(b: &crate::matched::Biquad) -> Option<Coefs> {
        // denominator → g, k (bilinear structure of the TPT SVF)
        let (a1, a2) = (b.a1, b.a2);
        let den = 1.0 - a1 + a2;
        let g2 = (1.0 + a1 + a2) / den;
        if !(g2 > 0.0) || !(den > 0.0) {
            return None;
        }
        let g = g2.sqrt();
        let d = 4.0 / den; // 1 + g² + g k
        let gk = 0.5 * d * (1.0 - a2);
        // numerator → m0, m1, m2: N(z)·D = m0·Den(z) + m1·g(1 − z⁻²) + m2·g²(1 + z⁻¹)²
        let r = [b.b0 * d, b.b1 * d, b.b2 * d];
        let m = [[d, g, g2], [2.0 * g2 - 2.0, 0.0, 2.0 * g2], [1.0 + g2 - gk, -g, g2]];
        let det = |m: &[[f64; 3]; 3]| {
            m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
                + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
        };
        let dm = det(&m);
        if dm.abs() < 1e-30 {
            return None;
        }
        let mut sol = [0.0; 3];
        for (j, out) in sol.iter_mut().enumerate() {
            let mut mj = m;
            for i in 0..3 {
                mj[i][j] = r[i];
            }
            *out = det(&mj) / dm;
        }
        let a1s = 1.0 / d;
        let a2s = g * a1s;
        let a3s = g * a2s;
        Some(Coefs { a1: a1s, a2: a2s, a3: a3s, m0: sol[0], m1: sol[1], m2: sol[2] })
    }
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

    /// Realises any stable biquad (e.g. a matched design) in the TPT structure.
    pub fn set_biquad(&mut self, b: &crate::matched::Biquad) {
        if let Some(c) = Coefs::from_biquad(b) {
            self.c = c;
            self.left = 0;
        }
    }

    /// Same, reaching the new coefficients linearly over `n` samples (control-rate updates without
    /// steps).
    pub fn set_biquad_ramped(&mut self, b: &crate::matched::Biquad, n: u32) {
        if let Some(t) = Coefs::from_biquad(b) {
            let n = n.max(1);
            let k = 1.0 / n as f64;
            self.step = Coefs {
                a1: (t.a1 - self.c.a1) * k,
                a2: (t.a2 - self.c.a2) * k,
                a3: (t.a3 - self.c.a3) * k,
                m0: (t.m0 - self.c.m0) * k,
                m1: (t.m1 - self.c.m1) * k,
                m2: (t.m2 - self.c.m2) * k,
            };
            self.left = n;
        }
    }

    pub fn reset(&mut self) {
        self.ic1 = 0.0;
        self.ic2 = 0.0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        if self.left > 0 {
            self.left -= 1;
            let s = self.step;
            let c = &mut self.c;
            c.a1 += s.a1;
            c.a2 += s.a2;
            c.a3 += s.a3;
            c.m0 += s.m0;
            c.m1 += s.m1;
            c.m2 += s.m2;
        }
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
    fn biquad_mapping_matches_direct_form() {
        use crate::matched::{design, Analog};
        for (p, f0) in [(Analog::bell(1.0, 9.0), 3000.0), (Analog::low_pass(0.707), 7000.0), (Analog::high_shelf(0.6, -5.0), 9000.0)] {
            let b = design(p, f0, 48000.0);
            let mut s = Svf::new();
            s.set_biquad(&b);
            let (mut x1, mut x2, mut y1, mut y2) = (0.0f64, 0.0, 0.0, 0.0);
            let mut rng = crate::lfo::Rng::new(5);
            for _ in 0..2000 {
                let x = rng.bipolar() as f64;
                let y = b.b0 * x + b.b1 * x1 + b.b2 * x2 - b.a1 * y1 - b.a2 * y2;
                x2 = x1;
                x1 = x;
                y2 = y1;
                y1 = y;
                let z = s.process(x as f32) as f64;
                assert!((z - y).abs() < 1e-4, "{z} vs {y}");
            }
        }
    }

    #[test]
    fn bandpass_unity_at_centre() {
        assert!(gain(SvfMode::BandPass, 800.0, 4.0, 0.0, 800.0).abs() < 0.05);
    }
}
