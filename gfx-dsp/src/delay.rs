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

    /// Copies another line's contents and position without allocating (same prepared size); a line
    /// of another size is left as it is.
    pub fn copy_from(&mut self, other: &DelayLine) {
        if self.buf.len() == other.buf.len() {
            self.buf.copy_from_slice(&other.buf);
            self.write = other.write;
        }
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
                lagrange4(&p, t)
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
                lagrange6(&p, t)
            }
        }
    }
}

/// `N` delay lines sharing one interleaved buffer and one write index: a push writes one contiguous row,
/// and the lines of a network (an FDN, a diffuser step) stay together in cache.
#[derive(Clone, Debug, Default)]
pub struct MultiLine<const N: usize> {
    buf: Vec<f32>,
    mask: usize,
    write: usize,
}

impl<const N: usize> MultiLine<N> {
    pub fn prepare(&mut self, max_delay: usize) {
        let rows = (max_delay + 8).next_power_of_two();
        self.buf = vec![0.0; rows * N];
        self.mask = rows - 1;
        self.write = 0;
    }

    pub fn reset(&mut self) {
        self.buf.iter_mut().for_each(|x| *x = 0.0);
        self.write = 0;
    }

    pub fn max_delay(&self) -> usize {
        self.mask.saturating_sub(4)
    }

    /// Writes one sample to every line.
    #[inline]
    pub fn push(&mut self, x: &[f32; N]) {
        self.write = (self.write + 1) & self.mask;
        let at = self.write * N;
        self.buf[at..at + N].copy_from_slice(x);
    }

    /// Line `i`, `d` pushes ago (0 = the last one).
    #[inline]
    pub fn tap(&self, i: usize, d: usize) -> f32 {
        self.buf[((self.write.wrapping_sub(d)) & self.mask) * N + i]
    }

    /// Line `i` at a fractional delay.
    #[inline]
    pub fn read(&self, i: usize, delay: f64, interp: Interp) -> f32 {
        let d = delay.max(0.0);
        let k = d.floor() as usize;
        let t = (d - k as f64) as f32;
        match interp {
            Interp::Linear => {
                let a = self.tap(i, k);
                a + (self.tap(i, k + 1) - a) * t
            }
            _ => {
                let p = [self.tap(i, k.saturating_sub(1)), self.tap(i, k), self.tap(i, k + 1), self.tap(i, k + 2)];
                lagrange4(&p, t)
            }
        }
    }
}

/// Windowed-sinc fractional reader: 16 taps, Kaiser window (β = 7), cut-off at 0.9 × Nyquist, 256
/// fractional phases interpolated linearly. Needs a delay of at least 8 samples. Built once (it
/// allocates), read on the audio path.
#[derive(Clone, Debug)]
pub struct SincTable {
    /// 257 phases × 16 taps
    k: Vec<f32>,
}

impl SincTable {
    pub const HALF: usize = 8;
    const PHASES: usize = 256;

    pub fn new() -> Self {
        let taps = 2 * Self::HALF;
        let bessel = |x: f64| {
            let (mut sum, mut term) = (1.0f64, 1.0f64);
            for k in 1..30 {
                term *= (x / (2.0 * k as f64)).powi(2);
                sum += term;
            }
            sum
        };
        let beta = 7.0;
        let fc = 0.9;
        let mut k = vec![0.0f32; (Self::PHASES + 1) * taps];
        for p in 0..=Self::PHASES {
            let t = p as f64 / Self::PHASES as f64;
            let mut row = [0.0f64; 16];
            let mut sum = 0.0;
            for j in 0..taps {
                // tap j sits at delay offset (j − HALF + 1) from the integer part; distance from the read point
                let x = (j as f64 - Self::HALF as f64 + 1.0) - t;
                let r = x / Self::HALF as f64;
                let w = if r.abs() < 1.0 { bessel(beta * (1.0 - r * r).sqrt()) / bessel(beta) } else { 0.0 };
                let sinc = if x.abs() < 1e-12 { 1.0 } else { (std::f64::consts::PI * fc * x).sin() / (std::f64::consts::PI * fc * x) };
                row[j] = fc * sinc * w;
                sum += row[j];
            }
            for j in 0..taps {
                k[p * taps + j] = (row[j] / sum) as f32;
            }
        }
        Self { k }
    }

    /// Fractional read of `line` at `delay` samples (≥ 8).
    #[inline]
    pub fn read(&self, line: &DelayLine, delay: f64) -> f32 {
        let taps = 2 * Self::HALF;
        let d = delay.max(Self::HALF as f64);
        let i = d.floor() as usize;
        let pf = (d - i as f64) * Self::PHASES as f64;
        let p = pf.floor() as usize;
        let u = (pf - p as f64) as f32;
        let (r0, r1) = (&self.k[p * taps..p * taps + taps], &self.k[(p + 1) * taps..(p + 2) * taps]);
        let mut y = 0.0f32;
        for j in 0..taps {
            let w = r0[j] + u * (r1[j] - r0[j]);
            y += w * line.tap(i + j + 1 - Self::HALF);
        }
        y
    }
}

impl Default for SincTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Cubic Lagrange through points at −1, 0, 1, 2, evaluated at `t`: closed-form weights with constant
/// denominators (the generic form recomputed them with 16 divisions per read).
#[inline]
fn lagrange4(p: &[f32; 4], t: f32) -> f32 {
    let (a, b, c, d) = (t + 1.0, t, t - 1.0, t - 2.0);
    let (cd, ab) = (c * d, a * b);
    -p[0] * b * cd * (1.0 / 6.0) + p[1] * a * cd * 0.5 - p[2] * ab * d * 0.5 + p[3] * ab * c * (1.0 / 6.0)
}

/// Quintic Lagrange through points at −2 … 3, evaluated at `t` (closed form, prefix / suffix products).
#[inline]
fn lagrange6(p: &[f32; 6], t: f32) -> f32 {
    let x = [t + 2.0, t + 1.0, t, t - 1.0, t - 2.0, t - 3.0];
    // pre[j] = x0 … x(j−1), suf[j] = x(j+1) … x5
    let mut pre = [1.0f32; 6];
    let mut suf = [1.0f32; 6];
    for j in 1..6 {
        pre[j] = pre[j - 1] * x[j - 1];
    }
    for j in (0..5).rev() {
        suf[j] = suf[j + 1] * x[j + 1];
    }
    const DEN: [f32; 6] = [-1.0 / 120.0, 1.0 / 24.0, -1.0 / 12.0, 1.0 / 12.0, -1.0 / 24.0, 1.0 / 120.0];
    let mut y = 0.0;
    for j in 0..6 {
        y += p[j] * pre[j] * suf[j] * DEN[j];
    }
    y
}

/// The generic form (reference for the tests).
#[cfg(test)]
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

    #[test]
    fn closed_form_lagrange_matches_the_generic() {
        let p4 = [0.3f32, -0.7, 1.1, 0.2];
        let p6 = [0.5f32, -0.1, 0.9, -1.2, 0.4, 0.8];
        for k in 0..=20 {
            let t = k as f32 / 20.0;
            assert!((lagrange4(&p4, t) - lagrange(&p4, -1.0, t)).abs() < 1e-5);
            assert!((lagrange6(&p6, t) - lagrange(&p6, -2.0, t)).abs() < 1e-5);
        }
    }

    #[test]
    fn sinc_reads_fractional_delays() {
        let sinc = SincTable::new();
        let mut l = DelayLine::new();
        l.prepare(256);
        let f = 0.05f64;
        for n in 0..200 {
            l.push((std::f64::consts::TAU * f * n as f64).sin() as f32);
        }
        for &d in &[10.0f64, 10.25, 10.5, 33.7] {
            let want = (std::f64::consts::TAU * f * (199.0 - d)).sin() as f32;
            let got = sinc.read(&l, d);
            assert!((got - want).abs() < 2e-3, "d {d}: {got} vs {want}");
        }
    }

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
