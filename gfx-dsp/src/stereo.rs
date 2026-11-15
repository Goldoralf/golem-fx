// SPDX-License-Identifier: GPL-3.0-or-later
//! Mid / side helpers and stereo meters (design: gfx-widener/README.md §2.1, §2.5, §2.7).
//!
//! - The matrix: M = (L + R) / 2, S = (L − R) / 2; L = M + S, R = M − S. With L = R the mid is L bit
//!   for bit and the side is exactly 0.
//! - [`balance`]: the sum-constant pan S' = S − b·M (L + R = 2M whatever b): the one pan law that
//!   keeps the mono fold exact.
//! - [`rotate`]: a true rotation of the (M, S) vector, positive = right; it moves the fold.
//! - [`Correlation`]: the broadband Pearson correlation of L and R over exponential averages (the
//!   phase-correlation meter), 1 when silent.
//! - [`FoldMeter`]: the mono fold's deviation from a reference fold (a dry fold × the output gain),
//!   in dB, floored.
//!
//! Averages in f64, updated per sample; the meters are read whenever the caller wants (per block).

use crate::util::one_pole_coef;

/// (mid, side) of a stereo pair.
#[inline]
pub fn to_ms(l: f32, r: f32) -> (f32, f32) {
    (0.5 * (l + r), 0.5 * (l - r))
}

/// (left, right) of a mid / side pair.
#[inline]
pub fn to_lr(m: f32, s: f32) -> (f32, f32) {
    (m + s, m - s)
}

/// The sum-constant balance: the new side for a balance `b` in −1 … +1 (positive = right).
/// L = (1 − b)·M + S, R = (1 + b)·M − S: at ±1 a mono source sits on one side, +6.02 dB there.
#[inline]
pub fn balance(m: f32, s: f32, b: f32) -> f32 {
    s - b * m
}

/// Rotates (M, S) by the angle whose cosine and sine are given (positive = right): M' = M cos θ +
/// S sin θ, S' = S cos θ − M sin θ. A mono source at +45° comes out on the right only, +3.01 dB.
/// At cos = 1, sin = 0 it returns its input bit for bit.
#[inline]
pub fn rotate(m: f32, s: f32, cos: f32, sin: f32) -> (f32, f32) {
    if sin == 0.0 && cos == 1.0 {
        return (m, s);
    }
    (m * cos + s * sin, s * cos - m * sin)
}

/// The L / R correlation meter: r = ⟨LR⟩ / √(⟨L²⟩⟨R²⟩), exponential averages of time constant τ.
#[derive(Clone, Debug)]
pub struct Correlation {
    tau_ms: f32,
    coef: f64,
    lr: f64,
    ll: f64,
    rr: f64,
}

impl Correlation {
    /// Below this mean power (−90 dBFS each) the meter reads 1: nothing to correlate.
    pub const SILENCE: f64 = 1e-9;

    pub fn new(tau_ms: f32) -> Self {
        Self { tau_ms, coef: 0.0, lr: 0.0, ll: 0.0, rr: 0.0 }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.coef = one_pole_coef(self.tau_ms as f64 * 0.001 * sample_rate as f64);
    }

    pub fn reset(&mut self) {
        self.lr = 0.0;
        self.ll = 0.0;
        self.rr = 0.0;
    }

    #[inline]
    pub fn push(&mut self, l: f32, r: f32) {
        let (l, r) = (l as f64, r as f64);
        let k = 1.0 - self.coef;
        self.lr += k * (l * r - self.lr);
        self.ll += k * (l * l - self.ll);
        self.rr += k * (r * r - self.rr);
    }

    /// −1 … +1; 1 when either channel is silent.
    pub fn value(&self) -> f32 {
        if self.ll < Self::SILENCE || self.rr < Self::SILENCE {
            return 1.0;
        }
        (self.lr / (self.ll * self.rr).sqrt()).clamp(-1.0, 1.0) as f32
    }
}

/// The mono-fold meter: 10 log(⟨(F − ℓ·D)²⟩ / ⟨(ℓ·D)²⟩), F the output's fold, D the dry fold, ℓ the
/// output gain (sample by sample), floored at `FLOOR_DB`; silence (⟨D²⟩ under −90 dBFS) reads the
/// floor.
#[derive(Clone, Debug)]
pub struct FoldMeter {
    tau_ms: f32,
    coef: f64,
    err: f64,
    reference: f64,
    dry: f64,
}

impl FoldMeter {
    pub const FLOOR_DB: f32 = -80.0;
    pub const CEIL_DB: f32 = 20.0;
    pub const SILENCE: f64 = 1e-9;

    pub fn new(tau_ms: f32) -> Self {
        Self { tau_ms, coef: 0.0, err: 0.0, reference: 0.0, dry: 0.0 }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.coef = one_pole_coef(self.tau_ms as f64 * 0.001 * sample_rate as f64);
    }

    pub fn reset(&mut self) {
        self.err = 0.0;
        self.reference = 0.0;
        self.dry = 0.0;
    }

    /// `fold`: the output's (L + R) / 2; `dry`: the input's; `gain`: the output gain ℓ.
    #[inline]
    pub fn push(&mut self, fold: f32, dry: f32, gain: f32) {
        let d = dry as f64;
        let r = gain as f64 * d;
        let e = fold as f64 - r;
        let k = 1.0 - self.coef;
        self.err += k * (e * e - self.err);
        self.reference += k * (r * r - self.reference);
        self.dry += k * (d * d - self.dry);
    }

    pub fn db(&self) -> f32 {
        if self.dry < Self::SILENCE || self.reference <= 0.0 || self.err <= 0.0 {
            return Self::FLOOR_DB;
        }
        ((10.0 * (self.err / self.reference).log10()) as f32).clamp(Self::FLOOR_DB, Self::CEIL_DB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(n: usize, seed: u64) -> Vec<f32> {
        let mut r = crate::lfo::Rng::new(seed);
        (0..n).map(|_| 0.3 * r.bipolar()).collect()
    }

    #[test]
    fn matrix_round_trips_and_keeps_the_mid_of_a_mono_pair() {
        for (i, x) in noise(4096, 3).into_iter().enumerate() {
            let (m, s) = to_ms(x, x);
            assert_eq!(m, x);
            assert_eq!(s, 0.0);
            let y = 0.7 * x - 0.001 * i as f32;
            let (m, s) = to_ms(x, y);
            let (l, r) = to_lr(m, s);
            let tol = 1e-6 * x.abs().max(y.abs());
            assert!((l - x).abs() <= tol && (r - y).abs() <= tol);
        }
    }

    #[test]
    fn balance_keeps_the_fold_and_pans() {
        let x = noise(4096, 5);
        for b in [-1.0f32, -0.35, 0.0, 0.35, 1.0] {
            for &v in &x {
                let s = balance(v, 0.0, b);
                let (l, r) = to_lr(v, s);
                assert!((0.5 * (l + r) - v).abs() <= 1e-7 * v.abs().max(1e-30) + 1e-12);
                if b == 1.0 {
                    assert_eq!(l, 0.0);
                    assert_eq!(r, 2.0 * v);
                }
            }
        }
    }

    #[test]
    fn rotation_is_a_constant_power_pan_on_a_mono_source() {
        let (c, s) = (std::f32::consts::FRAC_PI_4.cos(), std::f32::consts::FRAC_PI_4.sin());
        let (m, sd) = rotate(1.0, 0.0, c, s);
        let (l, r) = to_lr(m, sd);
        assert!(l.abs() < 1e-6, "{l}");
        assert!((20.0 * r.log10() - 3.0103).abs() < 0.01, "{r}");
        // θ = 0 is bit-exact
        for v in noise(1000, 9) {
            assert_eq!(rotate(v, 0.5 * v, 1.0, 0.0), (v, 0.5 * v));
        }
    }

    #[test]
    fn correlation_reads_mono_antiphase_and_independent_noise() {
        let sr = 48000.0;
        let a = noise(sr as usize, 1);
        let b = noise(sr as usize, 2);
        let run = |l: &[f32], r: &[f32]| {
            let mut c = Correlation::new(300.0);
            c.prepare(sr);
            for (x, y) in l.iter().zip(r) {
                c.push(*x, *y);
            }
            c.value()
        };
        let neg: Vec<f32> = a.iter().map(|v| -v).collect();
        assert!((run(&a, &a) - 1.0).abs() < 1e-6);
        assert!((run(&a, &neg) + 1.0).abs() < 1e-6);
        assert!(run(&a, &b).abs() < 0.05, "{}", run(&a, &b));
        assert_eq!(run(&[0.0; 100], &[0.0; 100]), 1.0);
    }

    #[test]
    fn fold_meter_floors_an_exact_fold_and_reads_a_loss() {
        let sr = 48000.0;
        let x = noise(sr as usize, 4);
        let mut f = FoldMeter::new(300.0);
        f.prepare(sr);
        for &v in &x {
            f.push(0.5 * v, v, 0.5);
        }
        assert_eq!(f.db(), FoldMeter::FLOOR_DB);
        // a fold at cos θ of the dry: 20 log(1 − cos θ)
        let th = 20f32.to_radians();
        f.reset();
        for &v in &x {
            f.push(v * th.cos(), v, 1.0);
        }
        let want = 20.0 * (1.0 - th.cos()).log10();
        assert!((f.db() - want).abs() < 0.05, "{} vs {want}", f.db());
    }
}
