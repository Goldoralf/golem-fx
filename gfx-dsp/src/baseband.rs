// SPDX-License-Identifier: GPL-3.0-or-later
//! Radio channels simulated in complex baseband at the host rate (gfx-radio §2.3). No carrier is
//! synthesised: a station is a complex envelope around 0 Hz; tuning is a rotation; the receiver is a
//! low-pass on I and Q followed by a detector.
//!
//! - [`Cx`]: a complex number in f64.
//! - [`Gauss`]: Gaussian deviates by Box-Muller from the seeded [`Rng`] (true tails: the FM click
//!   statistics need them).
//! - [`Rotator`]: a phase-continuous complex oscillator whose frequency may glide (f64, renormalised
//!   every sample).
//! - [`IfFilter`]: a real Butterworth low-pass (even order ≤ 8) on I and Q, TPT sections in f64, and its
//!   two-sided noise bandwidth ([`noise_bandwidth`]).
//! - Noise as a **density** ([`noise_sigma`]): the complex noise variance per sample is
//!   P_carrier · fs / (ρ · B_n), so the carrier-to-noise ratio ρ in the IF bandwidth B_n is the same at
//!   every host rate.
//! - [`Multipath`]: c = e(t) + a · e^{jφ} · e(t − τ), with τ, a and φ slow seeded walks.
//! - [`EnvelopeDetector`]: |c| into a diode's RC (instant attack, release τ_d: diagonal clipping).
//! - [`Discriminator`]: the limiter-discriminator arg(c[n] · conj(c[n−1])); 2π phase slips (FM threshold
//!   clicks) emerge by themselves below about 10 dB CNR.
//! - [`NoiseSquelch`]: the discriminator's noise above 4 kHz, followed (fast when it falls, slow when it
//!   rises), against a fixed threshold.

use crate::delay::{DelayLine, Interp};
use crate::lfo::Rng;
use crate::svf::{Svf, SvfMode};
use std::f64::consts::{PI, TAU};
use std::ops::{Add, AddAssign, Mul, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cx {
    pub re: f64,
    pub im: f64,
}

impl Cx {
    pub const ZERO: Cx = Cx { re: 0.0, im: 0.0 };
    pub const ONE: Cx = Cx { re: 1.0, im: 0.0 };
    #[inline]
    pub const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }
    #[inline]
    pub fn expj(ph: f64) -> Self {
        let (s, c) = ph.sin_cos();
        Self { re: c, im: s }
    }
    #[inline]
    pub fn conj(self) -> Self {
        Self { re: self.re, im: -self.im }
    }
    #[inline]
    pub fn norm_sqr(self) -> f64 {
        self.re * self.re + self.im * self.im
    }
    #[inline]
    pub fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }
    #[inline]
    pub fn arg(self) -> f64 {
        self.im.atan2(self.re)
    }
    #[inline]
    pub fn scale(self, k: f64) -> Self {
        Self { re: self.re * k, im: self.im * k }
    }
}

impl Add for Cx {
    type Output = Cx;
    #[inline]
    fn add(self, o: Cx) -> Cx {
        Cx::new(self.re + o.re, self.im + o.im)
    }
}

impl AddAssign for Cx {
    #[inline]
    fn add_assign(&mut self, o: Cx) {
        self.re += o.re;
        self.im += o.im;
    }
}

impl Sub for Cx {
    type Output = Cx;
    #[inline]
    fn sub(self, o: Cx) -> Cx {
        Cx::new(self.re - o.re, self.im - o.im)
    }
}

impl Mul for Cx {
    type Output = Cx;
    #[inline]
    fn mul(self, o: Cx) -> Cx {
        Cx::new(self.re * o.re - self.im * o.im, self.re * o.im + self.im * o.re)
    }
}

/// Standard normal deviates (Box-Muller, the second deviate kept for the next call).
#[derive(Clone, Debug)]
pub struct Gauss {
    rng: Rng,
    spare: Option<f64>,
}

impl Gauss {
    pub fn new(seed: u64) -> Self {
        Self { rng: Rng::new(seed), spare: None }
    }

    pub fn reset(&mut self) {
        self.rng.reset();
        self.spare = None;
    }

    #[inline]
    pub fn next(&mut self) -> f64 {
        if let Some(s) = self.spare.take() {
            return s;
        }
        let u1 = 1.0 - self.rng.next_f64();
        let u2 = self.rng.next_f64();
        let r = (-2.0 * u1.ln()).sqrt();
        let (s, c) = (TAU * u2).sin_cos();
        self.spare = Some(r * s);
        r * c
    }

    /// A complex deviate with E|z|² = `var`.
    #[inline]
    pub fn complex(&mut self, var: f64) -> Cx {
        let s = (0.5 * var).sqrt();
        Cx::new(s * self.next(), s * self.next())
    }

    /// The underlying uniform source.
    pub fn rng(&mut self) -> &mut Rng {
        &mut self.rng
    }
}

/// A complex oscillator e^{j2π f t}, phase-continuous when f moves.
#[derive(Clone, Debug)]
pub struct Rotator {
    z: Cx,
    step: Cx,
    hz: f64,
    sr: f64,
}

impl Default for Rotator {
    fn default() -> Self {
        Self { z: Cx::ONE, step: Cx::ONE, hz: 0.0, sr: 48000.0 }
    }
}

impl Rotator {
    pub fn prepare(&mut self, sr: f64) {
        self.sr = sr;
        let hz = self.hz;
        self.hz = f64::NAN;
        self.set_hz(hz);
    }

    pub fn reset(&mut self) {
        self.z = Cx::ONE;
    }

    /// The frequency (Hz, either sign); the phase carries on.
    #[inline]
    pub fn set_hz(&mut self, hz: f64) {
        if hz != self.hz {
            self.hz = hz;
            self.step = Cx::expj(TAU * hz / self.sr);
        }
    }

    pub fn hz(&self) -> f64 {
        self.hz
    }

    /// The current value, then one step.
    #[inline]
    pub fn next(&mut self) -> Cx {
        let z = self.z;
        let n = z * self.step;
        // renormalise (one Newton step toward |z| = 1)
        let k = 1.5 - 0.5 * n.norm_sqr();
        self.z = n.scale(k);
        z
    }
}

/// A TPT low-pass section in f64 (Zavalishin), unity at DC.
#[derive(Clone, Copy, Debug, Default)]
pub struct Lp2 {
    g: f64,
    k: f64,
    a1: f64,
    s1: f64,
    s2: f64,
}

impl Lp2 {
    pub fn set(&mut self, hz: f64, q: f64, sr: f64) {
        self.g = (PI * hz.min(0.49 * sr) / sr).tan();
        self.k = 1.0 / q;
        self.a1 = 1.0 / (1.0 + self.g * (self.g + self.k));
    }

    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let hp = (x - (self.g + self.k) * self.s1 - self.s2) * self.a1;
        let bp = self.g * hp + self.s1;
        self.s1 = self.g * hp + bp;
        let lp = self.g * bp + self.s2;
        self.s2 = self.g * bp + lp;
        lp
    }
}

/// Most Butterworth sections per rail (order 8).
pub const IF_MAX_SECTIONS: usize = 4;

/// A Butterworth low-pass of even `order` on I and Q: the IF filter (bandwidth ±`hz`).
#[derive(Clone, Debug, Default)]
pub struct IfFilter {
    i: [Lp2; IF_MAX_SECTIONS],
    q: [Lp2; IF_MAX_SECTIONS],
    n: usize,
}

impl IfFilter {
    pub fn set(&mut self, order: usize, hz: f64, sr: f64) {
        self.n = (order / 2).clamp(1, IF_MAX_SECTIONS);
        let order = 2 * self.n;
        for k in 0..self.n {
            let theta = PI * (2 * k + 1) as f64 / (2 * order) as f64;
            let q = 1.0 / (2.0 * theta.sin());
            self.i[k].set(hz, q, sr);
            self.q[k].set(hz, q, sr);
        }
    }

    pub fn reset(&mut self) {
        for s in self.i.iter_mut().chain(self.q.iter_mut()) {
            s.reset();
        }
    }

    #[inline]
    pub fn process(&mut self, c: Cx) -> Cx {
        let (mut a, mut b) = (c.re, c.im);
        for k in 0..self.n {
            a = self.i[k].process(a);
            b = self.q[k].process(b);
        }
        Cx::new(a, b)
    }
}

/// The two-sided noise bandwidth (Hz) of a Butterworth low-pass of `order` and edge `hz` on I and Q:
/// 2 · hz · (π / 2n) / sin(π / 2n).
pub fn noise_bandwidth(order: usize, hz: f64) -> f64 {
    let a = PI / (2 * order) as f64;
    2.0 * hz * a / a.sin()
}

/// The complex noise variance per sample for a carrier of power `p_carrier` at `cnr_db` in the noise
/// bandwidth `bn` Hz, at the host rate `fs`: P · fs / (ρ · B_n).
pub fn noise_variance(p_carrier: f64, fs: f64, cnr_db: f64, bn: f64) -> f64 {
    p_carrier * fs / (10f64.powf(cnr_db / 10.0) * bn)
}

/// A seeded random walk: a new target every 1 / `rate` s, cosine-interpolated (smooth, bounded ±1).
#[derive(Clone, Debug)]
pub struct Walk {
    rng: Rng,
    a: f64,
    b: f64,
    ph: f64,
}

impl Walk {
    pub fn new(seed: u64) -> Self {
        let mut w = Self { rng: Rng::new(seed), a: 0.0, b: 0.0, ph: 0.0 };
        w.reset();
        w
    }

    pub fn reset(&mut self) {
        self.rng.reset();
        self.a = self.rng.bipolar() as f64;
        self.b = self.rng.bipolar() as f64;
        self.ph = 0.0;
    }

    /// Advances by `inc` = rate / fs; returns the walk in ±1.
    #[inline]
    pub fn next(&mut self, inc: f64) -> f64 {
        self.ph += inc;
        if self.ph >= 1.0 {
            self.ph -= 1.0;
            self.a = self.b;
            self.b = self.rng.bipolar() as f64;
        }
        let c = 0.5 - 0.5 * (PI * self.ph).cos();
        self.a + (self.b - self.a) * c
    }
}

/// Selective fading by a second path: c = e(t) + a · e^{jφ} · e(t − τ). τ, a and φ follow seeded walks
/// (0.05 – 0.5 Hz); the depth scales a (0 = one path, the input untouched).
#[derive(Clone, Debug)]
pub struct Multipath {
    line: DelayLine,
    tau: Walk,
    amp: Walk,
    phi: Walk,
    phase: f64,
    sr: f64,
}

impl Default for Multipath {
    fn default() -> Self {
        Self { line: DelayLine::new(), tau: Walk::new(0x7A01), amp: Walk::new(0x7A02), phi: Walk::new(0x7A03), phase: 0.0, sr: 48000.0 }
    }
}

impl Multipath {
    pub fn prepare(&mut self, sr: f64) {
        self.sr = sr;
        self.line.prepare((0.0012 * sr) as usize + 8);
    }

    pub fn reset(&mut self) {
        self.line.reset();
        self.tau.reset();
        self.amp.reset();
        self.phi.reset();
        self.phase = 0.0;
    }

    /// One sample of the real envelope `e`; `depth` 0 – 1.
    #[inline]
    pub fn process(&mut self, e: f64, depth: f64) -> Cx {
        self.line.push(e as f32);
        let inv = 1.0 / self.sr;
        let t = self.tau.next(0.13 * inv);
        let a = self.amp.next(0.21 * inv);
        // the second path's carrier phase drifts at up to ±0.5 Hz: the notch sweeps through the band
        self.phase += TAU * 0.5 * self.phi.next(0.05 * inv) * inv;
        if self.phase > PI {
            self.phase -= TAU;
        } else if self.phase < -PI {
            self.phase += TAU;
        }
        if depth <= 0.0 {
            return Cx::new(e, 0.0);
        }
        let tau = (0.0006 + 0.0004 * t) * self.sr;
        let a = depth * (0.5 + 0.3 * a);
        let d = self.line.read(tau.max(1.0), Interp::Hermite) as f64;
        Cx::new(e, 0.0) + Cx::expj(self.phase).scale(a * d)
    }
}

/// The AM envelope detector: |c| into a diode's RC (instant attack, exponential release).
#[derive(Clone, Debug, Default)]
pub struct EnvelopeDetector {
    env: f64,
    rel: f64,
}

impl EnvelopeDetector {
    /// The release time constant τ_d (s) at `sr`.
    pub fn set_release(&mut self, tau: f64, sr: f64) {
        self.rel = (-1.0 / (tau * sr).max(1e-9)).exp();
    }

    pub fn reset(&mut self) {
        self.env = 0.0;
    }

    #[inline]
    pub fn process(&mut self, c: Cx) -> f64 {
        let a = c.abs();
        self.env = a.max(self.env * self.rel);
        self.env
    }
}

/// The limiter-discriminator: the phase step per sample, in units of `dev_hz` (1 = full deviation).
#[derive(Clone, Debug, Default)]
pub struct Discriminator {
    c1: Cx,
    k: f64,
}

impl Discriminator {
    pub fn set(&mut self, dev_hz: f64, sr: f64) {
        self.k = sr / (TAU * dev_hz);
    }

    pub fn reset(&mut self) {
        self.c1 = Cx::ZERO;
    }

    /// The raw phase step (radians) and the scaled frequency.
    #[inline]
    pub fn process(&mut self, c: Cx) -> (f64, f64) {
        let d = (c * self.c1.conj()).arg();
        self.c1 = c;
        (d, d * self.k)
    }
}

/// The noise squelch: the discriminator output above 4 kHz, its magnitude averaged over 5 ms, then
/// followed with a fast fall (the carrier quiets it: the squelch opens) and a slow rise (the tail),
/// against a fixed threshold.
#[derive(Clone, Debug)]
pub struct NoiseSquelch {
    hp: [Svf; 2],
    avg: f64,
    avg_c: f64,
    level: f64,
    up: f64,
    down: f64,
    threshold: f64,
    open: bool,
    sr: f64,
}

impl Default for NoiseSquelch {
    fn default() -> Self {
        Self { hp: [Svf::new(), Svf::new()], avg: 0.0, avg_c: 0.0, level: 0.0, up: 0.0, down: 0.0, threshold: 1.0, open: true, sr: 48000.0 }
    }
}

impl NoiseSquelch {
    pub fn prepare(&mut self, sr: f64) {
        self.sr = sr;
        self.hp[0].set(SvfMode::HighPass, 4000.0, 0.541_196, 0.0, sr as f32);
        self.hp[1].set(SvfMode::HighPass, 4000.0, 1.306_563, 0.0, sr as f32);
        self.avg_c = (-1.0 / (0.005 * sr)).exp();
    }

    /// Times (s): the follower's rise (the tail) and fall (the opening); the threshold on the level.
    pub fn set(&mut self, rise: f64, fall: f64, threshold: f64) {
        self.up = (-1.0 / (rise * self.sr).max(1.0)).exp();
        self.down = (-1.0 / (fall * self.sr).max(1.0)).exp();
        self.threshold = threshold;
    }

    pub fn reset(&mut self) {
        for s in self.hp.iter_mut() {
            s.reset();
        }
        self.avg = 0.0;
        self.level = 0.0;
        self.open = true;
    }

    /// The noise level averaged over 5 ms (before the follower).
    pub fn noise(&self) -> f64 {
        self.avg
    }

    /// The followed noise level.
    pub fn level(&self) -> f64 {
        self.level
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// One discriminator sample; returns whether the squelch is open.
    #[inline]
    pub fn process(&mut self, d: f64) -> bool {
        let h = { let t = self.hp[0].process(d as f32); self.hp[1].process(t) } as f64;
        self.avg = h.abs() + (self.avg - h.abs()) * self.avg_c;
        let a = self.avg;
        let c = if a > self.level { self.up } else { self.down };
        self.level = a + (self.level - a) * c;
        self.open = self.level < self.threshold;
        self.open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn amp_at(y: &[f64], f: f64, sr: f64) -> f64 {
        let n = y.len();
        let (mut c, mut s, mut ws) = (0.0, 0.0, 0.0);
        for (i, v) in y.iter().enumerate() {
            let w = 0.5 - 0.5 * (TAU * i as f64 / n as f64).cos();
            let ph = TAU * f * i as f64 / sr;
            c += v * w * ph.cos();
            s += v * w * ph.sin();
            ws += w;
        }
        2.0 * c.hypot(s) / ws
    }

    /// Net 2π slips of an unmodulated carrier at `cnr` dB in a ±6 kHz 6th-order IF (FM threshold).
    fn clicks_per_s(sr: f64, cnr: f64, secs: f64) -> f64 {
        let mut g = Gauss::new(1);
        let mut f = IfFilter::default();
        f.set(6, 6000.0, sr);
        let var = noise_variance(1.0, sr, cnr, noise_bandwidth(6, 6000.0));
        let mut d = Discriminator::default();
        d.set(2500.0, sr);
        let n = (sr * secs) as usize;
        // unwrapped phase, smoothed over 5 ms, in turns: count the integer crossings
        let k = (sr * 0.005) as usize;
        let mut ring = vec![0.0; k];
        let (mut sum, mut tot, mut last, mut clicks) = (0.0, 0.0, 0.0f64, 0.0);
        for i in 0..n {
            let c = f.process(Cx::ONE + g.complex(var));
            let (ph, _) = d.process(c);
            tot += ph;
            sum += tot - ring[i % k];
            ring[i % k] = tot;
            if i > k {
                let lev = (sum / k as f64 / TAU).round();
                if i > k + 1 {
                    clicks += (lev - last).abs();
                }
                last = lev;
            }
        }
        clicks / secs
    }

    #[test]
    fn fm_clicks_emerge_at_rices_rate() {
        for sr in [48000.0, 96000.0] {
            let r15 = clicks_per_s(sr, 15.0, 10.0);
            let r8 = clicks_per_s(sr, 8.0, 20.0);
            let r6 = clicks_per_s(sr, 6.0, 10.0);
            let r4 = clicks_per_s(sr, 4.0, 10.0);
            assert!(r15 == 0.0, "{sr}: {r15} at 15 dB");
            assert!((0.5..=2.5).contains(&r8), "{sr}: {r8} at 8 dB");
            assert!((10.0..=20.0).contains(&r6), "{sr}: {r6} at 6 dB");
            assert!((50.0..=90.0).contains(&r4), "{sr}: {r4} at 4 dB");
        }
    }

    #[test]
    fn noise_density_holds_across_rates() {
        // the IF output's noise power for a given CNR is the same at every rate
        let mut p = Vec::new();
        for sr in [44100.0, 48000.0, 96000.0, 192000.0] {
            let mut g = Gauss::new(3);
            let mut f = IfFilter::default();
            f.set(6, 3000.0, sr);
            let var = noise_variance(1.0, sr, 20.0, noise_bandwidth(6, 3000.0));
            let n = (sr * 4.0) as usize;
            let mut e = 0.0;
            for i in 0..n {
                let c = f.process(g.complex(var));
                if i > n / 4 {
                    e += c.norm_sqr();
                }
            }
            p.push(10.0 * (e / (n - n / 4) as f64).log10());
        }
        for v in &p {
            assert!((v - (-20.0)).abs() < 0.3, "{p:?}");
        }
    }

    #[test]
    fn am_detuning_distortion_comes_from_the_if_filter() {
        let sr = 48000.0;
        let n = 96000;
        let thd = |delta: f64| {
            let mut f = IfFilter::default();
            f.set(6, 3000.0, sr);
            let mut rot = Rotator::default();
            rot.prepare(sr);
            rot.set_hz(-delta);
            let y: Vec<f64> = (0..n)
                .map(|i| {
                    let e = 1.0 + 0.85 * (TAU * 1000.0 * i as f64 / sr).sin();
                    f.process(rot.next().scale(e)).abs()
                })
                .collect();
            let y = &y[n / 2..];
            let a = |k: f64| amp_at(y, 1000.0 * k, sr);
            let h: f64 = (2..8).map(|k| a(k as f64).powi(2)).sum::<f64>().sqrt();
            h / a(1.0)
        };
        assert!(thd(0.0) < 0.002);
        let t2 = thd(2000.0);
        let t3 = thd(3000.0);
        assert!((0.015..=0.045).contains(&t2), "2 kHz: {t2}");
        assert!((0.06..=0.14).contains(&t3), "3 kHz: {t3}");
    }

    #[test]
    fn rotator_is_phase_continuous_and_unit() {
        let mut r = Rotator::default();
        r.prepare(48000.0);
        r.set_hz(1000.0);
        let mut last = r.next();
        for i in 0..200000 {
            if i == 1000 {
                r.set_hz(-3000.0);
            }
            let z = r.next();
            assert!((z.abs() - 1.0).abs() < 1e-9);
            assert!((z - last).abs() < 0.5);
            last = z;
        }
    }

    #[test]
    fn multipath_at_zero_depth_is_the_input() {
        let mut m = Multipath::default();
        m.prepare(48000.0);
        for i in 0..1000 {
            let e = (i as f64 * 0.01).sin();
            assert_eq!(m.process(e, 0.0), Cx::new(e, 0.0));
        }
    }
}
