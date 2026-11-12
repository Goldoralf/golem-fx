// SPDX-License-Identifier: GPL-3.0-or-later
//! Matched second-order designs — Vicanek, "Matched Second Order Digital Filters" (2016): the poles
//! come from impulse invariance (no peak narrowing towards Nyquist) and the numerator is fitted so the
//! digital magnitude equals the analog one at DC, at the centre frequency and at Nyquist. No cramping
//! without oversampling, closed form, cheap enough to recompute at control rate.
//!
//! Generic over any analog second-order section: the prototype is given by its numerator and
//! denominator polynomials in s (frequencies normalised to the centre). Realised in the TPT
//! state-variable filter by [`crate::svf::Svf::set_biquad`].

use std::f64::consts::PI;

/// An analog second-order section n(s) / d(s), s normalised to the centre frequency.
#[derive(Clone, Copy, Debug)]
pub struct Analog {
    pub n: [f64; 3], // n0 + n1 s + n2 s²
    pub d: [f64; 3], // d0 + d1 s + d2 s²
}

impl Analog {
    /// |H(jΩ)|², Ω = f / f0.
    pub fn mag2(&self, w: f64) -> f64 {
        let num = (self.n[0] - self.n[2] * w * w).powi(2) + (self.n[1] * w).powi(2);
        let den = (self.d[0] - self.d[2] * w * w).powi(2) + (self.d[1] * w).powi(2);
        num / den.max(1e-300)
    }

    pub fn low_pass(q: f64) -> Self {
        Self { n: [1.0, 0.0, 0.0], d: [1.0, 1.0 / q, 1.0] }
    }
    pub fn high_pass(q: f64) -> Self {
        Self { n: [0.0, 0.0, 1.0], d: [1.0, 1.0 / q, 1.0] }
    }
    /// Unity gain at the centre.
    pub fn band_pass(q: f64) -> Self {
        Self { n: [0.0, 1.0 / q, 0.0], d: [1.0, 1.0 / q, 1.0] }
    }
    /// Symmetric bell (boost and cut mirror each other), `gain_db` at the centre.
    pub fn bell(q: f64, gain_db: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        Self { n: [1.0, a / q, 1.0], d: [1.0, 1.0 / (a * q), 1.0] }
    }
    /// Low shelf (RBJ analog prototype): `gain_db` below, 0 dB above, half gain at the centre.
    pub fn low_shelf(q: f64, gain_db: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let sa = a.sqrt();
        Self { n: [a * a, a * sa / q, a], d: [1.0, sa / q, a] }
    }
    /// High shelf: 0 dB below, `gain_db` above.
    pub fn high_shelf(q: f64, gain_db: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let sa = a.sqrt();
        Self { n: [a, a * sa / q, a * a], d: [a, sa / q, 1.0] }
    }
}

/// Biquad coefficients, a0 = 1: H(z) = (b0 + b1 z⁻¹ + b2 z⁻²) / (1 + a1 z⁻¹ + a2 z⁻²).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Biquad {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Biquad {
    /// |H(e^{jω})|² at `freq`.
    pub fn mag2(&self, freq: f64, sample_rate: f64) -> f64 {
        let w = 2.0 * PI * freq / sample_rate;
        let s = (w / 2.0).sin().powi(2);
        let (p0, p1, p2) = (1.0 - s, s, 4.0 * (1.0 - s) * s);
        let n = (self.b0 + self.b1 + self.b2).powi(2) * p0 + (self.b0 - self.b1 + self.b2).powi(2) * p1 - 4.0 * self.b0 * self.b2 * p2;
        let d = (1.0 + self.a1 + self.a2).powi(2) * p0 + (1.0 - self.a1 + self.a2).powi(2) * p1 - 4.0 * self.a2 * p2;
        n / d.max(1e-300)
    }
}

/// The matched design of `proto` centred on `f0` Hz. A cut (a section whose poles are more damped
/// than its zeros: a bell or shelf below 0 dB) is designed as the mirror boost and inverted, so the
/// impulse-invariant poles always come from the less damped side and cuts mirror boosts exactly.
pub fn design(proto: Analog, f0: f64, sample_rate: f64) -> Biquad {
    let zeta = |p: &[f64; 3]| if p[2] > 0.0 && p[0] > 0.0 { p[1] / (2.0 * (p[0] * p[2]).sqrt()) } else { f64::INFINITY };
    let invertible = proto.n[0] > 0.0 && proto.n[2] > 0.0;
    if invertible && zeta(&proto.d) > zeta(&proto.n) {
        let boost = design_direct(Analog { n: proto.d, d: proto.n }, f0, sample_rate);
        // invert: (1 + a1 z⁻¹ + a2 z⁻²) / (b0 + b1 z⁻¹ + b2 z⁻²), normalised
        return Biquad { b0: 1.0 / boost.b0, b1: boost.a1 / boost.b0, b2: boost.a2 / boost.b0, a1: boost.b1 / boost.b0, a2: boost.b2 / boost.b0 };
    }
    design_direct(proto, f0, sample_rate)
}

fn design_direct(proto: Analog, f0: f64, sample_rate: f64) -> Biquad {
    let w0 = 2.0 * PI * (f0 / sample_rate).clamp(1e-6, 0.4999);
    // poles of d(s): natural frequency and damping
    let (d0, d1, d2) = (proto.d[0], proto.d[1], proto.d[2]);
    let wp = (d0 / d2).sqrt() * w0;
    let zeta = d1 / (2.0 * (d0 * d2).sqrt());
    let (a1, a2) = if zeta <= 1.0 {
        let e = (-zeta * wp).exp();
        (-2.0 * e * ((1.0 - zeta * zeta).sqrt() * wp).cos(), e * e)
    } else {
        let e = (-zeta * wp).exp();
        (-2.0 * e * ((zeta * zeta - 1.0).sqrt() * wp).cosh(), e * e)
    };
    // denominator in the φ basis
    let a0p = (1.0 + a1 + a2).powi(2);
    let a1p = (1.0 - a1 + a2).powi(2);
    let a2p = -4.0 * a2;
    let phi = |w: f64| {
        let s = (w / 2.0).sin().powi(2);
        (1.0 - s, s, 4.0 * (1.0 - s) * s)
    };
    // analog magnitude² at DC, Nyquist and the centre (Ω = ω / ω0)
    let t0 = proto.mag2(0.0);
    let tn = proto.mag2(PI / w0);
    let b0p = t0 * a0p;
    let b1p = tn * a1p;
    // the one free term of the numerator: least squares on the relative error over a log grid
    // around the centre (Vicanek matches it at the centre only; the fit spreads the residual error
    // evenly, about halving it near Nyquist)
    let (mut num, mut den) = (0.0f64, 0.0f64);
    let lo = (w0 / 16.0).max(1e-4);
    let hi = (w0 * 16.0).min(0.95 * PI);
    let steps = 24;
    for k in 0..=steps {
        let w = lo * (hi / lo).powf(k as f64 / steps as f64);
        let (p0, p1, p2) = phi(w);
        let target = proto.mag2(w / w0) * (a0p * p0 + a1p * p1 + a2p * p2);
        if target <= 1e-12 || p2 <= 1e-12 {
            continue;
        }
        let r = target - b0p * p0 - b1p * p1;
        let wgt = 1.0 / (target * target);
        num += wgt * p2 * r;
        den += wgt * p2 * p2;
    }
    let b2p = if den > 0.0 { num / den } else { 0.0 };
    // spectral factorisation of the numerator
    let sb0 = b0p.max(0.0).sqrt();
    let sb1 = b1p.max(0.0).sqrt();
    let w = 0.5 * (sb0 + sb1);
    let b0 = 0.5 * (w + (w * w + b2p).max(0.0).sqrt());
    let b1 = 0.5 * (sb0 - sb1);
    let b2 = w - b0;
    // the sign of b1 follows the analog numerator's tilt (low-pass-like: positive)
    Biquad { b0, b1, b2, a1, a2 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worst_db(proto: Analog, f0: f64, sr: f64, up_to: f64) -> f64 {
        let b = design(proto, f0, sr);
        let mut worst = 0.0f64;
        let mut f = 20.0;
        while f < up_to {
            let a = 10.0 * proto.mag2(f / f0).max(1e-30).log10();
            let d = 10.0 * b.mag2(f, sr).max(1e-30).log10();
            if a > -40.0 {
                worst = worst.max((a - d).abs());
            }
            f *= 1.02;
        }
        worst
    }

    #[test]
    fn bells_do_not_cramp() {
        for &f0 in &[100.0, 1000.0, 8000.0, 12000.0, 15000.0] {
            for &g in &[-12.0, 6.0, 12.0] {
                for &q in &[0.7, 2.0] {
                    let e = worst_db(Analog::bell(q, g), f0, 48000.0, 0.9 * 24000.0);
                    assert!(e < 1.0, "bell {f0} Hz {g} dB Q {q}: {e:.2} dB off");
                }
            }
        }
    }

    #[test]
    fn far_better_than_bilinear_near_nyquist() {
        // an 8 kHz bell, +12 dB, Q 0.7 at 48 kHz: the RBJ (bilinear) design is several dB off at the top
        let sr = 48000.0;
        let proto = Analog::bell(0.7, 12.0);
        let mut svf = crate::svf::Svf::new();
        svf.set(crate::svf::SvfMode::Bell, 8000.0, 0.7, 12.0, sr as f32);
        let f = 18000.0;
        let analog = 10.0 * proto.mag2(f / 8000.0).log10();
        let bilinear = crate::svf::measure_gain_db(|x| svf.process(x), f as f32, sr as f32) as f64;
        let matched = 10.0 * design(proto, 8000.0, sr).mag2(f, sr).log10();
        assert!((matched - analog).abs() < 0.8, "matched {matched:.2} vs analog {analog:.2}");
        assert!((bilinear - analog).abs() > 2.0, "bilinear {bilinear:.2} vs analog {analog:.2}");
    }

    #[test]
    fn shelves_and_cuts() {
        let sr = 48000.0;
        for (name, p, f0) in [
            ("low shelf", Analog::low_shelf(0.707, 6.0), 120.0),
            ("high shelf", Analog::high_shelf(0.707, -6.0), 5000.0),
            ("high shelf boost", Analog::high_shelf(0.5, 4.0), 12000.0),
            ("low pass", Analog::low_pass(0.707), 7000.0),
            ("high pass", Analog::high_pass(0.707), 90.0),
        ] {
            let e = worst_db(p, f0, sr, 0.9 * 24000.0);
            assert!(e < 1.0, "{name}: {e:.2} dB off");
        }
    }
}
