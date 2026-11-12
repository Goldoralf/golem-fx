// SPDX-License-Identifier: GPL-3.0-or-later
//! Circular delay line with fractional reads.
//!
//! Interpolators (Laakso, Välimäki et al., "Splitting the Unit Delay", 1996): for a *moving* read
//! head, Lagrange 3 / 5 or Hermite — never a first-order allpass, which produces transients when
//! modulated. Linear is kept as a reference only (variable treble loss).

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Interp {
    Linear,
    Hermite,
    Lagrange3,
    Lagrange5,
}

impl Interp {
    /// Samples needed before (older than) the read point, for the bound on the shortest delay.
    pub const fn guard(self) -> usize {
        match self {
            Interp::Linear => 1,
            Interp::Hermite | Interp::Lagrange3 => 2,
            Interp::Lagrange5 => 3,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DelayLine {
    buf: Vec<f32>,
    mask: usize,
    write: usize,
}

impl DelayLine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocates for delays up to `max_delay` samples (plus interpolation guard).
    pub fn prepare(&mut self, max_delay: usize) {
        let size = (max_delay + 8).next_power_of_two();
        self.buf = vec![0.0; size];
        self.mask = size - 1;
        self.write = 0;
    }

    pub fn reset(&mut self) {
        self.buf.iter_mut().for_each(|x| *x = 0.0);
        self.write = 0;
    }

    /// Largest delay that can be read.
    pub fn max_delay(&self) -> usize {
        self.buf.len().saturating_sub(8)
    }

    #[inline]
    pub fn push(&mut self, x: f32) {
        self.write = (self.write + 1) & self.mask;
        self.buf[self.write] = x;
    }

    /// The sample written `d` pushes ago (0 = the last one pushed).
    #[inline]
    pub fn tap(&self, d: usize) -> f32 {
        self.buf[(self.write.wrapping_sub(d)) & self.mask]
    }

    /// Fractional read, `delay` in samples (≥ the interpolator's guard − 1).
    #[inline]
    pub fn read(&self, delay: f64, interp: Interp) -> f32 {
        let d = delay.max(0.0);
        let i = d.floor() as usize;
        let t = (d - i as f64) as f32;
        match interp {
            Interp::Linear => {
                let a = self.tap(i);
                let b = self.tap(i + 1);
                a + (b - a) * t
            }
            Interp::Hermite => {
                // points at delays i-1, i, i+1, i+2; t in [0, 1) between i and i+1
                let xm1 = self.tap(i.wrapping_sub(1).min(i));
                let x0 = self.tap(i);
                let x1 = self.tap(i + 1);
                let x2 = self.tap(i + 2);
                let c1 = 0.5 * (x1 - xm1);
                let c2 = xm1 - 2.5 * x0 + 2.0 * x1 - 0.5 * x2;
                let c3 = 0.5 * (x2 - xm1) + 1.5 * (x0 - x1);
                ((c3 * t + c2) * t + c1) * t + x0
            }
            Interp::Lagrange3 => {
                // nodes at offsets -1, 0, 1, 2 (relative to i), evaluate at t
                let p = [self.tap(i.saturating_sub(1)), self.tap(i), self.tap(i + 1), self.tap(i + 2)];
                lagrange(&p, -1.0, t)
            }
            Interp::Lagrange5 => {
                let p = [
                    self.tap(i.saturating_sub(2)),
                    self.tap(i.saturating_sub(1)),
                    self.tap(i),
                    self.tap(i + 1),
                    self.tap(i + 2),
                    self.tap(i + 3),
                ];
                lagrange(&p, -2.0, t)
            }
        }
    }
}

/// Lagrange interpolation through points at integer abscissae `first, first+1, …`, evaluated at `t`.
#[inline]
fn lagrange<const N: usize>(p: &[f32; N], first: f32, t: f32) -> f32 {
    let mut sum = 0.0f32;
    for j in 0..N {
        let xj = first + j as f32;
        let mut w = 1.0f32;
        for m in 0..N {
            if m != j {
                let xm = first + m as f32;
                w *= (t - xm) / (xj - xm);
            }
        }
        sum += w * p[j];
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error_db(interp: Interp, freq: f32) -> f32 {
        let sr = 48000.0f32;
        let mut d = DelayLine::new();
        d.prepare(1024);
        let w = 2.0 * std::f64::consts::PI * freq as f64 / sr as f64;
        let delay = 100.37f64;
        let mut err = 0.0f64;
        let mut sig = 0.0f64;
        for n in 0..8000 {
            d.push((w * n as f64).sin() as f32);
            if n > 1000 {
                let y = d.read(delay, interp) as f64;
                let want = (w * (n as f64 - delay)).sin();
                err += (y - want).powi(2);
                sig += want * want;
            }
        }
        10.0 * (err / sig).log10() as f32
    }

    #[test]
    fn integer_delay_is_exact() {
        let mut d = DelayLine::new();
        d.prepare(64);
        for n in 0..100 {
            d.push(n as f32);
        }
        assert_eq!(d.read(10.0, Interp::Lagrange5), 89.0);
        assert_eq!(d.read(10.0, Interp::Hermite), 89.0);
    }

    #[test]
    fn interpolation_quality_ranks() {
        // at 5 kHz / 48 kHz, higher order = smaller error
        let lin = error_db(Interp::Linear, 5000.0);
        let l3 = error_db(Interp::Lagrange3, 5000.0);
        let l5 = error_db(Interp::Lagrange5, 5000.0);
        assert!(l3 < lin - 10.0, "lin {lin} l3 {l3}");
        assert!(l5 < l3 - 10.0, "l3 {l3} l5 {l5}");
        assert!(l5 < -60.0, "l5 {l5}");
    }
}
