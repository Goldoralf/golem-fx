// SPDX-License-Identifier: GPL-3.0-or-later
//! Band-limited sampling and band-limited steps at fractional instants, with one minimum-phase kernel
//! (gfx-crusher's virtual converter; candidate: gfx-delay's Lo-Fi model).
//!
//! - **The kernel**: a Kaiser-windowed low-pass (pass band to 0.35 fs, stop band ≥ 86 dB from 0.6 fs,
//!   relative to the host rate), converted to minimum phase by the cepstral method, tabulated as
//!   [`TAPS`] host taps × [`PHASES`] phases with linear interpolation between phases. Each impulse row
//!   is normalised to unity DC gain. Built once per process (it does not depend on the sample rate).
//! - [`Reader`]: the input's value at a fractional instant inside the last host sample (the impulse
//!   table, transposed: 32 MACs). Causal, no look-ahead.
//! - [`Renderer`]: a held (stepped) signal rendered at the host rate with minBLEP corrections (Brandt
//!   2001): each step adds `Δ · (band-limited step − ideal step)` to a residual ring. Causal.
//!
//! Neither reports latency; each carries the kernel's low-frequency group delay ([`Kernel::delay`],
//! a few samples), which a parallel dry path can match with a whole-sample delay.
//!
//! Time convention: an instant is processed at the host sample `n` that follows it, and given by
//! `d = n − t` ∈ [0, 1] (0: on the sample, 1: on the previous one).

use realfft::num_complex::Complex;
use realfft::RealFftPlanner;
use std::sync::OnceLock;

/// Kernel length in host samples.
pub const TAPS: usize = 32;
/// Table phases per host sample (rows 0 … PHASES, the last one = the next sample's first).
pub const PHASES: usize = 64;
const FINE: usize = TAPS * PHASES;
/// Kaiser β of the linear-phase prototype (≈ 100 dB side lobes).
const BETA: f64 = 10.0;
/// Centre of the transition band (fraction of the host rate): 0.35 – 0.6 fs.
const CUTOFF: f64 = 0.475;

/// The tabulated minimum-phase kernel.
pub struct Kernel {
    /// The continuous kernel sampled at PHASES × the host rate, unit area (Σ = PHASES).
    pub fine: Vec<f64>,
    /// Impulse rows: `imp[p][i]` = h(i + p / PHASES), each row summing to 1.
    imp: Vec<[f32; TAPS]>,
    /// Step residual rows: `step[p][i]` = S(i + p / PHASES) − 1, S the band-limited unit step.
    step: Vec<[f32; TAPS]>,
    /// Low-frequency group delay in host samples (the kernel's first moment).
    pub delay: f64,
}

impl Kernel {
    /// The shared kernel (built on first use: about 10 ms, from `prepare`).
    pub fn get() -> &'static Kernel {
        static K: OnceLock<Kernel> = OnceLock::new();
        K.get_or_init(Kernel::build)
    }

    fn build() -> Kernel {
        // linear-phase prototype at the fine rate, odd length, centred
        let m = FINE - 1;
        let c = (m - 1) as f64 / 2.0;
        let fc = CUTOFF / PHASES as f64;
        let i0b = bessel_i0(BETA);
        let proto: Vec<f64> = (0..m)
            .map(|u| {
                let t = u as f64 - c;
                let x = 2.0 * fc * t;
                let sinc = if x.abs() < 1e-12 { 1.0 } else { (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x) };
                let r = t / c;
                let w = bessel_i0(BETA * (1.0 - r * r).max(0.0).sqrt()) / i0b;
                2.0 * fc * sinc * w
            })
            .collect();
        let g = minimum_phase(&proto, 16 * FINE, FINE);
        let sum: f64 = g.iter().sum();
        let fine: Vec<f64> = g.iter().map(|v| v * PHASES as f64 / sum).collect();
        // g at index FINE (and beyond) is 0
        let at = |u: usize| if u < FINE { fine[u] } else { 0.0 };
        let mut imp = vec![[0.0f32; TAPS]; PHASES + 1];
        for (p, row) in imp.iter_mut().enumerate() {
            let s: f64 = (0..TAPS).map(|i| at(i * PHASES + p)).sum();
            for (i, v) in row.iter_mut().enumerate() {
                *v = (at(i * PHASES + p) / s) as f32;
            }
        }
        // running sum (trapezoid) normalised to end at 1
        let mut cum = vec![0.0f64; FINE + 1];
        let mut acc = 0.0;
        for u in 0..=FINE {
            let prev = if u == 0 { 0.0 } else { at(u - 1) };
            acc += 0.5 * (prev + at(u));
            cum[u] = acc;
        }
        let total = cum[FINE];
        let mut step = vec![[0.0f32; TAPS]; PHASES + 1];
        for (p, row) in step.iter_mut().enumerate() {
            for (i, v) in row.iter_mut().enumerate() {
                let u = (i * PHASES + p).min(FINE);
                *v = (cum[u] / total - 1.0) as f32;
            }
        }
        let delay = fine.iter().enumerate().map(|(u, v)| u as f64 / PHASES as f64 * v).sum::<f64>() / PHASES as f64;
        Kernel { fine, imp, step, delay }
    }

    /// The continuous kernel's frequency response at `f` (fraction of the host rate): (magnitude, phase).
    pub fn response(&self, f: f64) -> (f64, f64) {
        let w = 2.0 * std::f64::consts::PI * f / PHASES as f64;
        let (mut re, mut im) = (0.0, 0.0);
        for (u, v) in self.fine.iter().enumerate() {
            re += v * (w * u as f64).cos();
            im -= v * (w * u as f64).sin();
        }
        let s = PHASES as f64;
        ((re / s).hypot(im / s), im.atan2(re))
    }

    /// An impulse row's DC gain (tests).
    pub fn row_sum(&self, p: usize) -> f64 {
        self.imp[p].iter().map(|v| *v as f64).sum()
    }

    /// The dry path's whole-sample alignment for `n` kernels in series (sampling + rendering = 2).
    pub fn align(&self, n: usize) -> usize {
        (self.delay * n as f64).round() as usize
    }

    /// The host-rate FIR a parallel dry needs to match a read + render pair: the kernel at whole
    /// samples, convolved with itself (2 · TAPS − 1 taps, unity DC, low-frequency delay 2 · delay).
    /// Matches the pair's magnitude and phase below 0.4 fs, where a whole-sample delay leaves a
    /// fraction of a sample and the minimum phase's curvature.
    pub fn pair_fir(&self) -> [f32; 2 * TAPS - 1] {
        let h = &self.imp[0];
        let mut out = [0.0f32; 2 * TAPS - 1];
        for i in 0..TAPS {
            for j in 0..TAPS {
                out[i + j] += (h[i] as f64 * h[j] as f64) as f32;
            }
        }
        out
    }
}

fn bessel_i0(x: f64) -> f64 {
    let (mut sum, mut term, mut k) = (1.0, 1.0, 1.0);
    let q = x * x / 4.0;
    while term > 1e-17 * sum {
        term *= q / (k * k);
        sum += term;
        k += 1.0;
    }
    sum
}

/// The minimum-phase filter with `h`'s magnitude (real cepstrum, folded), `len` samples of it.
fn minimum_phase(h: &[f64], nfft: usize, len: usize) -> Vec<f64> {
    let mut planner = RealFftPlanner::<f64>::new();
    let fwd = planner.plan_fft_forward(nfft);
    let inv = planner.plan_fft_inverse(nfft);
    let mut buf = vec![0.0f64; nfft];
    buf[..h.len()].copy_from_slice(h);
    let mut spec = fwd.make_output_vec();
    let _ = fwd.process(&mut buf, &mut spec);
    let peak = spec.iter().fold(0.0f64, |m, c| m.max(c.norm()));
    let floor = peak * 1e-10;
    for c in spec.iter_mut() {
        *c = Complex::new(c.norm().max(floor).ln(), 0.0);
    }
    let _ = inv.process(&mut spec, &mut buf);
    let n = nfft as f64;
    // fold the cepstrum: causal part doubled
    for (k, v) in buf.iter_mut().enumerate() {
        *v /= n;
        if k > 0 && k < nfft / 2 {
            *v *= 2.0;
        } else if k > nfft / 2 {
            *v = 0.0;
        }
    }
    let _ = fwd.process(&mut buf, &mut spec);
    let last = spec.len() - 1;
    for (k, c) in spec.iter_mut().enumerate() {
        *c = c.exp();
        if k == 0 || k == last {
            c.im = 0.0;
        }
    }
    let _ = inv.process(&mut spec, &mut buf);
    buf.iter().take(len).map(|v| v / n).collect()
}

/// Reads the input through the kernel at fractional instants. Feed every host sample with
/// [`Reader::push`] **after** reading the instants that fall before it.
#[derive(Clone, Debug)]
pub struct Reader {
    /// the last TAPS samples, newest first, stored twice (contiguous reads)
    buf: [f32; 2 * TAPS],
    w: usize,
}

impl Default for Reader {
    fn default() -> Self {
        Self { buf: [0.0; 2 * TAPS], w: 0 }
    }
}

impl Reader {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Appends host sample `x[n]`.
    #[inline]
    pub fn push(&mut self, x: f32) {
        self.w = if self.w == 0 { TAPS - 1 } else { self.w - 1 };
        self.buf[self.w] = x;
        self.buf[self.w + TAPS] = x;
    }

    /// The band-limited input at t = n − d, d ∈ [0, 1], n the sample about to be pushed (so the history
    /// ends at n − 1). Delayed by the kernel's group delay.
    #[inline]
    pub fn read(&self, k: &Kernel, d: f64) -> f32 {
        // τ = (n − 1 − i … ) → h(i + q), q = 1 − d
        let pos = ((1.0 - d).clamp(0.0, 1.0)) * PHASES as f64;
        let p = (pos as usize).min(PHASES - 1);
        let f = (pos - p as f64) as f32;
        let (a, b) = (&k.imp[p], &k.imp[p + 1]);
        let x = &self.buf[self.w..self.w + TAPS];
        let (mut s0, mut s1) = (0.0f32, 0.0f32);
        for i in 0..TAPS {
            s0 += x[i] * a[i];
            s1 += x[i] * (b[i] - a[i]);
        }
        s0 + f * s1
    }

    /// The band-limited input's running integral at t = n − d, less the plain sum of every sample
    /// pushed so far: ∫x up to t = Σ x[m ≤ n − 1] + `integral(d)` (the step table, transposed).
    #[inline]
    pub fn integral(&self, k: &Kernel, d: f64) -> f32 {
        let pos = ((1.0 - d).clamp(0.0, 1.0)) * PHASES as f64;
        let p = (pos as usize).min(PHASES - 1);
        let f = (pos - p as f64) as f32;
        let (a, b) = (&k.step[p], &k.step[p + 1]);
        let x = &self.buf[self.w..self.w + TAPS];
        let (mut s0, mut s1) = (0.0f32, 0.0f32);
        for i in 0..TAPS {
            s0 += x[i] * a[i];
            s1 += x[i] * (b[i] - a[i]);
        }
        s0 + f * s1
    }
}

const RING: usize = 64;

/// Renders a held signal with minBLEP steps.
#[derive(Clone, Debug)]
pub struct Renderer {
    ring: [f32; RING],
    pos: usize,
    held: f32,
}

impl Default for Renderer {
    fn default() -> Self {
        Self { ring: [0.0; RING], pos: 0, held: 0.0 }
    }
}

impl Renderer {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The held value (the naive staircase).
    pub fn held(&self) -> f32 {
        self.held
    }

    /// The held value becomes `v` at t = n − d (d ∈ [0, 1]), n the sample [`Renderer::next`] returns next.
    #[inline]
    pub fn set(&mut self, k: &Kernel, v: f32, d: f64) {
        let delta = v - self.held;
        self.held = v;
        if delta == 0.0 {
            return;
        }
        let pos = d.clamp(0.0, 1.0) * PHASES as f64;
        let p = (pos as usize).min(PHASES - 1);
        let f = (pos - p as f64) as f32;
        let (a, b) = (&k.step[p], &k.step[p + 1]);
        for i in 0..TAPS {
            let r = a[i] + f * (b[i] - a[i]);
            self.ring[(self.pos + i) & (RING - 1)] += delta * r;
        }
    }

    /// The rendered output at the current host sample.
    #[inline]
    pub fn next(&mut self) -> f32 {
        let y = self.held + self.ring[self.pos];
        self.ring[self.pos] = 0.0;
        self.pos = (self.pos + 1) & (RING - 1);
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn kernel_meets_its_specification() {
        let k = Kernel::get();
        let mut worst_pass: f64 = 0.0;
        for i in 0..=70 {
            let f = 0.35 * i as f64 / 70.0;
            worst_pass = worst_pass.max((20.0 * k.response(f).0.log10()).abs());
        }
        assert!(worst_pass <= 0.5, "pass band ripple {worst_pass} dB");
        let mut worst_stop = f64::MIN;
        let mut f = 0.6;
        while f < PHASES as f64 / 2.0 {
            worst_stop = worst_stop.max(20.0 * k.response(f).0.log10());
            f += if f < 2.0 { 0.002 } else { 0.0371 };
        }
        assert!(worst_stop <= -86.0, "stop band {worst_stop} dB");
        for p in 0..=PHASES {
            assert!((k.row_sum(p) - 1.0).abs() < 1e-6, "row {p}: {}", k.row_sum(p));
        }
        assert!(k.delay > 2.0 && k.delay < 8.0, "delay {}", k.delay);
        // minimum phase: the kernel starts near zero and its energy comes early
        let e: f64 = k.fine.iter().map(|v| v * v).sum();
        let e_head: f64 = k.fine[..FINE / 2].iter().map(|v| v * v).sum();
        assert!(e_head / e > 0.999, "{}", e_head / e);
        eprintln!("pass ±{worst_pass:.3} dB, stop {worst_stop:.1} dB, delay {:.3} samples, h(0) {:.2e}", k.delay, k.fine[0]);
    }

    #[test]
    fn reader_reads_a_delayed_band_limited_signal() {
        let k = Kernel::get();
        // a low sine: x(t − D) at any fractional instant
        let sr = 48000.0;
        let f = 200.0;
        let w = 2.0 * PI * f / sr;
        let mut r = Reader::default();
        let mut worst: f64 = 0.0;
        for n in 0..4000usize {
            if n > 100 {
                for d in [0.0, 0.13, 0.5, 0.77, 1.0] {
                    let t = n as f64 - d - k.delay;
                    let got = r.read(k, d) as f64;
                    worst = worst.max((got - (w * t).sin()).abs());
                }
            }
            r.push((w * n as f64).sin() as f32);
        }
        assert!(worst < 1e-4, "{worst}");
        // DC is exact
        let mut r = Reader::default();
        for _ in 0..TAPS {
            r.push(0.5);
        }
        for d in [0.0, 0.3, 1.0] {
            assert!((r.read(k, d) - 0.5).abs() < 1e-6);
        }
    }

    #[test]
    fn renderer_step_is_band_limited_and_settles() {
        let k = Kernel::get();
        for d in [0.0, 0.25, 0.6, 1.0] {
            let mut r = Renderer::default();
            r.set(k, 1.0, d);
            let y: Vec<f32> = (0..TAPS + 4).map(|_| r.next()).collect();
            // ends exactly on the held value
            assert!(y[TAPS..].iter().all(|v| (*v - 1.0).abs() < 1e-6), "{d}: {:?}", &y[TAPS..]);
            // starts near zero (causal, minimum phase)
            assert!(y[0].abs() < 0.05, "{d}: {}", y[0]);
            // the residual rings out
            r.set(k, 0.0, d);
            for _ in 0..TAPS {
                r.next();
            }
            assert_eq!(r.next(), 0.0);
        }
    }
}
