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
        if g2.is_nan() || g2 <= 0.0 || den.is_nan() || den <= 0.0 {
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

/// TPT coefficients precomputed from a biquad (coefficient tables: gfx-ducker's Spectral bells), so
/// a control tick costs an interpolation instead of a design and a conversion.
#[derive(Clone, Copy, Debug, Default)]
pub struct SvfCoefs {
    c: Coefs,
}

impl SvfCoefs {
    /// The TPT realisation of a stable biquad (`None` if it has none).
    pub fn from_biquad(b: &crate::matched::Biquad) -> Option<Self> {
        Coefs::from_biquad(b).map(|c| Self { c })
    }

    /// Linear interpolation between two sets (t = 0 → self, 1 → other).
    #[inline]
    pub fn lerp(&self, other: &SvfCoefs, t: f64) -> SvfCoefs {
        let (a, b) = (&self.c, &other.c);
        let l = |x: f64, y: f64| x + (y - x) * t;
        SvfCoefs { c: Coefs { a1: l(a.a1, b.a1), a2: l(a.a2, b.a2), a3: l(a.a3, b.a3), m0: l(a.m0, b.m0), m1: l(a.m1, b.m1), m2: l(a.m2, b.m2) } }
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

    /// Sets the structure directly: cut-off `freq` (prewarped), damping k = 1 / Q, and the output mix
    /// m0·input + m1·band + m2·low (table-driven filters such as the wah models).
    pub fn set_raw(&mut self, freq: f32, q: f32, m0: f32, m1: f32, m2: f32, sample_rate: f32) {
        let sr = sample_rate as f64;
        let g = (PI * (freq as f64).clamp(1.0, 0.49 * sr) / sr).tan();
        let k = 1.0 / (q as f64).max(1e-3);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        self.c = Coefs { a1, a2, a3, m0: m0 as f64, m1: m1 as f64, m2: m2 as f64 };
        self.left = 0;
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

    /// Sets precomputed coefficients at once.
    pub fn set_coefs(&mut self, c: &SvfCoefs) {
        self.c = c.c;
        self.left = 0;
    }

    /// Reaches precomputed coefficients linearly over `n` samples (as `set_biquad_ramped`).
    pub fn set_coefs_ramped(&mut self, c: &SvfCoefs, n: u32) {
        let t = c.c;
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

/// tan(x) for 0 ≤ x ≤ 0.45 π (a cut-off up to 0.45 fs), as the [7/6] continued fraction (Lentz):
/// relative error < 1e-5 over that range, no libm call. For cut-offs that move every sample.
#[inline]
pub fn tan_fast(x: f64) -> f64 {
    let x2 = x * x;
    x * (135135.0 + x2 * (-17325.0 + x2 * (378.0 - x2))) / (135135.0 + x2 * (-62370.0 + x2 * (3150.0 - 28.0 * x2)))
}

/// TPT (trapezoidal) one-pole whose cut-off may change every sample (Zavalishin ch. 3): stable under
/// audio-rate modulation. One state, f64. Gives the low-pass; the high-pass is x − low-pass.
#[derive(Clone, Copy, Debug, Default)]
pub struct OnePoleTpt {
    s: f64,
}

impl OnePoleTpt {
    /// The per-sample coefficient G = g / (1 + g), g = tan(π f / fs), f clamped to 0.45 fs.
    #[inline]
    pub fn coef(freq: f64, sample_rate: f64) -> f64 {
        let f = freq.clamp(0.0, 0.45 * sample_rate);
        let g = tan_fast(std::f64::consts::PI * f / sample_rate);
        g / (1.0 + g)
    }

    pub fn reset(&mut self) {
        self.s = 0.0;
    }

    /// One sample with coefficient `big_g` (from [`OnePoleTpt::coef`]); returns the low-pass.
    #[inline]
    pub fn low(&mut self, x: f64, big_g: f64) -> f64 {
        let v = (x - self.s) * big_g;
        let lp = v + self.s;
        self.s = lp + v;
        lp
    }
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

    #[test]
    fn tan_fast_matches_tan() {
        let mut worst = 0.0f64;
        for i in 1..=1000 {
            let x = 0.45 * PI * i as f64 / 1000.0;
            worst = worst.max((tan_fast(x) / x.tan() - 1.0).abs());
        }
        assert!(worst < 1e-5, "{worst}");
    }

    #[test]
    fn one_pole_tpt_is_prewarped_and_survives_modulation() {
        // −3 dB at the cut-off, both outputs
        let g = OnePoleTpt::coef(1000.0, SR as f64);
        let mut f = OnePoleTpt::default();
        let lp = measure_gain_db(|x| f.low(x as f64, g) as f32, 1000.0, SR);
        let mut f = OnePoleTpt::default();
        let hp = measure_gain_db(|x| (x as f64 - f.low(x as f64, g)) as f32, 1000.0, SR);
        assert!((lp + 3.01).abs() < 0.05 && (hp + 3.01).abs() < 0.05, "{lp} {hp}");
        // random per-sample cut-offs from 10 Hz to 0.45 fs: bounded
        let mut f = OnePoleTpt::default();
        let mut r = 12345u64;
        let mut m = 0.0f64;
        for _ in 0..200_000 {
            r = r.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let u = (r >> 11) as f64 / (1u64 << 53) as f64;
            let x = if (r >> 7) & 1 == 0 { 1.0 } else { -1.0 };
            let y = f.low(x, OnePoleTpt::coef(10.0 * 2160f64.powf(u), SR as f64));
            m = m.max(y.abs());
        }
        // (the low-pass is a convex mix of the input and the state; the state stays within
        // tan(0.45 π) = 6.3 of a unit input)
        assert!(m <= 6.32, "{m}");
    }
}
