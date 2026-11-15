// SPDX-License-Identifier: GPL-3.0-or-later
//! A zoom track: the digital strobe of gfx-tuner (gfx-tuner README §2.4 – 2.5).
//!
//! A track heterodynes a (decimated) real signal by a centre frequency `fc` and keeps the baseband:
//! - complex demodulation z[n] = x[n] · e^(−j·2π·fc·n / fd), the oscillator's phase taken from an integer
//!   sample counter in f64 (a rotator resynchronised every 256 samples), so it does not drift;
//! - a **wide** 4th-order Butterworth low-pass at 0.5 · fc on Re and Im (f64 TPT sections): it removes the
//!   image and the upper partials and settles in about 2 periods. It selects nothing;
//! - decimation to fb = fd / ⌊fd / 4 fc⌋ into a ring of [`RING`] complex samples.
//!
//! Two estimators read the ring, both **finite windows** (a sample older than the window has no weight
//! at all, so the attack is forgotten exactly):
//! - [`Zoom::estimate`] (Mono): nine Hann frames of length T / 2 spaced T / 16, each rotated by the last
//!   estimate, and a |X|²-weighted line fit of their unwrapped phases. The slope is the frequency offset;
//!   the fit's residual gives an error bar σ (three independent frames in the span).
//! - [`Zoom::grid_peak`] + [`Zoom::refine`] (Poly): a 4-term Blackman-Harris DFT of the last T on a grid
//!   of cents around fc, then the phase difference of two half-overlapping Blackman-Harris frames
//!   (length 2T / 3) at the peak, iterated twice.
//!
//! Memory is allocated in `prepare`; `start`, `push` and the estimators never allocate.

use std::f64::consts::TAU;

/// Ring length (complex samples at fb): holds the longest window at any note.
pub const RING: usize = 4096;

/// Cents of a frequency ratio.
#[inline]
pub fn cents(ratio: f64) -> f64 {
    1200.0 * ratio.log2()
}

/// One TPT state-variable low-pass section in f64.
#[derive(Clone, Copy, Debug, Default)]
struct Lp {
    a1: f64,
    a2: f64,
    a3: f64,
    s1: f64,
    s2: f64,
}

impl Lp {
    fn set(&mut self, f: f64, q: f64, sr: f64) {
        let g = (std::f64::consts::PI * f.clamp(0.1, 0.49 * sr) / sr).tan();
        let k = 1.0 / q;
        self.a1 = 1.0 / (1.0 + g * (g + k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    #[inline]
    fn tick(&mut self, x: f64) -> f64 {
        let v3 = x - self.s2;
        let v1 = self.a1 * self.s1 + self.a2 * v3;
        let v2 = self.s2 + self.a2 * self.s1 + self.a3 * v3;
        self.s1 = 2.0 * v1 - self.s1;
        self.s2 = 2.0 * v2 - self.s2;
        v2
    }
}

/// The 4-term Blackman-Harris window's coefficients (−92 dB side lobes).
const BH: [f64; 4] = [0.35875, 0.48829, 0.14128, 0.01168];

/// The Mono estimator's result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Estimate {
    /// The frequency read (Hz).
    pub hz: f64,
    /// Its error bar (Hz, one standard deviation, from the fit's residual).
    pub sigma_hz: f64,
    /// The newest frame's power (amplitude² of the baseband component).
    pub power: f64,
    /// The fit left the capture range and was held at its edge ([`Zoom::set_capture`]).
    pub clamped: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Zoom {
    fd: f64,
    fc: f64,
    /// oscillator cycles per input sample
    inc: f64,
    /// input samples since `start`
    n: u64,
    rot: (f64, f64),
    step: (f64, f64),
    lp: [Lp; 4],
    dec: usize,
    dphase: usize,
    ring: Vec<(f64, f64)>,
    /// ring samples written since `start` (the absolute index of the next one)
    m: u64,
    /// the Mono estimator's last offset from fc (Hz): the frames are rotated by it
    delta: f64,
    /// the capture range (Hz): the estimate may not leave it
    capture: (f64, f64),
    /// scratch for the Poly estimators: a windowed window, and its sums in fours
    work: Vec<(f64, f64)>,
    chunks: Vec<(f64, f64)>,
}

impl Zoom {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocates the ring for a track fed at `fd` (Hz).
    pub fn prepare(&mut self, fd: f32) {
        self.fd = fd as f64;
        self.ring = vec![(0.0, 0.0); RING];
        self.work = vec![(0.0, 0.0); RING];
        self.chunks = vec![(0.0, 0.0); RING / 4 + 1];
        self.start(100.0);
    }

    /// (Re)starts the track centred on `fc` (Hz): silence in the ring and the filters, offset 0.
    pub fn start(&mut self, fc: f64) {
        let fd = self.fd.max(1.0);
        self.fc = fc.clamp(1.0, 0.24 * fd);
        self.inc = self.fc / fd;
        self.n = 0;
        self.rot = (1.0, 0.0);
        self.step = ((TAU * self.inc).cos(), -(TAU * self.inc).sin());
        let qs = [0.541_196_100_146_197, 1.306_562_964_876_376_5];
        for (k, l) in self.lp.iter_mut().enumerate() {
            l.set(0.5 * self.fc, qs[k % 2], fd);
            l.s1 = 0.0;
            l.s2 = 0.0;
        }
        self.dec = ((fd / (4.0 * self.fc)).floor() as usize).max(1);
        self.dphase = 0;
        self.ring.iter_mut().for_each(|z| *z = (0.0, 0.0));
        self.m = 0;
        self.delta = 0.0;
        self.capture = (0.0, f64::INFINITY);
    }

    /// The capture range (Hz): an estimate outside it is held at its edge and flagged `clamped`, so
    /// the frames stay centred where the caller knows the note is (a louder neighbour cannot pull the
    /// track away while the window is still too short to reject it). Unbounded after `start`.
    pub fn set_capture(&mut self, lo: f64, hi: f64) {
        self.capture = (lo, hi);
    }

    /// The centre frequency (Hz).
    pub fn fc(&self) -> f64 {
        self.fc
    }

    /// The ring's rate (Hz).
    pub fn fb(&self) -> f64 {
        self.fd / self.dec as f64
    }

    /// Ring samples written since `start`.
    pub fn len(&self) -> u64 {
        self.m
    }

    pub fn is_empty(&self) -> bool {
        self.m == 0
    }

    /// Seconds of signal fed since `start`.
    pub fn elapsed(&self) -> f64 {
        self.n as f64 / self.fd
    }

    /// The Mono estimator's current offset from fc (Hz).
    pub fn delta(&self) -> f64 {
        self.delta
    }

    /// Feeds one sample at fd.
    #[inline]
    pub fn push(&mut self, x: f32) {
        let x = x as f64;
        let (c, s) = self.rot;
        let (re, im) = (x * c, x * s);
        self.n += 1;
        if self.n & 255 == 0 {
            // resynchronise the rotator on the exact phase (integer counter, f64)
            let ph = (self.n as f64 * self.inc).fract();
            self.rot = ((TAU * ph).cos(), -(TAU * ph).sin());
        } else {
            let (a, b) = self.step;
            self.rot = (c * a - s * b, c * b + s * a);
        }
        let re = self.lp[0].tick(re);
        let re = self.lp[1].tick(re);
        let im = self.lp[2].tick(im);
        let im = self.lp[3].tick(im);
        self.dphase += 1;
        if self.dphase >= self.dec {
            self.dphase = 0;
            self.ring[(self.m as usize) % RING] = (re, im);
            self.m += 1;
        }
    }

    /// The windowed sum Σ w_k z[i0 + k] e^(−j2π·d·(i0 + k) / fb) over `len` ring samples starting at
    /// the absolute index `i0`; `bh` = Blackman-Harris, else Hann (numpy's `hanning(len + 2)[1:-1]`).
    /// Returns (X, Σw).
    fn frame(&self, i0: u64, len: usize, d: f64, bh: bool) -> ((f64, f64), f64) {
        let fb = self.fb();
        let ph = -TAU * d * (i0 as f64 / fb);
        let (mut rc, mut rs) = (ph.cos(), ph.sin());
        let w0 = -TAU * d / fb;
        let (sc, ss) = (w0.cos(), w0.sin());
        // window phase rotator
        let th = if bh { TAU / (len.max(2) - 1) as f64 } else { TAU / (len + 1) as f64 };
        let (mut wc, mut ws) = if bh { (1.0, 0.0) } else { (th.cos(), th.sin()) };
        let (tc, ts) = (th.cos(), th.sin());
        let (mut xr, mut xi, mut sw) = (0.0, 0.0, 0.0);
        for k in 0..len {
            let w = if bh {
                let c2 = 2.0 * wc * wc - 1.0;
                let c3 = (4.0 * wc * wc - 3.0) * wc;
                BH[0] - BH[1] * wc + BH[2] * c2 - BH[3] * c3
            } else {
                0.5 - 0.5 * wc
            };
            let (zr, zi) = self.ring[((i0 + k as u64) as usize) % RING];
            // w · z · rot
            let (pr, pi) = (zr * rc - zi * rs, zr * rs + zi * rc);
            xr += w * pr;
            xi += w * pi;
            sw += w;
            let nrc = rc * sc - rs * ss;
            rs = rc * ss + rs * sc;
            rc = nrc;
            let nwc = wc * tc - ws * ts;
            ws = wc * ts + ws * tc;
            wc = nwc;
        }
        ((xr, xi), sw)
    }

    /// The oldest absolute ring index still held.
    fn oldest(&self) -> u64 {
        self.m.saturating_sub(RING as u64)
    }

    /// Mono estimate over the window of `t` seconds ending now, whose frames never reach before the
    /// absolute ring index `from`. None when fewer than three frames fit or the signal is null.
    /// Updates the offset the next estimate's frames are rotated by.
    pub fn estimate(&mut self, t: f64, from: u64) -> Option<Estimate> {
        let fb = self.fb();
        if self.m == 0 {
            return None;
        }
        let from = from.max(self.oldest());
        let l = 0.5 * t;
        let nl = ((l * fb).round() as usize).max(4);
        let sub = (nl / 8).max(1);
        let i1 = self.m - 1;
        let span = ((t - l) * fb).round() as u64;
        // frame ends, newest first; at most nine (the span holds 8 steps of L / 8)
        let mut ends = [0u64; 16];
        let mut ne = 0;
        let mut e = i1;
        loop {
            if e + 1 < nl as u64 || e + 1 - (nl as u64) < from || i1 - e > span || ne == ends.len() {
                break;
            }
            ends[ne] = e;
            ne += 1;
            if e < sub as u64 {
                break;
            }
            e -= sub as u64;
        }
        if ne < 3 {
            return None;
        }
        let d = self.delta;
        let mut ph = [0.0f64; 16];
        let mut wt = [0.0f64; 16];
        let mut tt = [0.0f64; 16];
        let mut power = 0.0;
        // oldest first
        for j in 0..ne {
            let e = ends[ne - 1 - j];
            let ((xr, xi), sw) = self.frame(e + 1 - nl as u64, nl, d, false);
            ph[j] = xi.atan2(xr);
            wt[j] = xr * xr + xi * xi;
            tt[j] = e as f64 / fb;
            if j == ne - 1 {
                power = wt[j] / (sw * sw).max(1e-300);
            }
        }
        for j in 1..ne {
            let mut dp = ph[j] - ph[j - 1];
            while dp > std::f64::consts::PI {
                dp -= TAU;
            }
            while dp < -std::f64::consts::PI {
                dp += TAU;
            }
            ph[j] = ph[j - 1] + dp;
        }
        let w: f64 = wt[..ne].iter().sum();
        if w <= 1e-300 || !w.is_finite() {
            return None;
        }
        let tm = (0..ne).map(|j| wt[j] * tt[j]).sum::<f64>() / w;
        let pm = (0..ne).map(|j| wt[j] * ph[j]).sum::<f64>() / w;
        let stt = (0..ne).map(|j| wt[j] * (tt[j] - tm) * (tt[j] - tm)).sum::<f64>();
        if stt <= 0.0 {
            return None;
        }
        let slope = (0..ne).map(|j| wt[j] * (tt[j] - tm) * (ph[j] - pm)).sum::<f64>() / stt;
        let rs2 = (0..ne)
            .map(|j| {
                let r = ph[j] - pm - slope * (tt[j] - tm);
                wt[j] * r * r
            })
            .sum::<f64>()
            / w;
        // about three independent frames in the span (the nine overlap by 7 / 8)
        let sigma = rs2.sqrt() / (stt / w * 3.0).sqrt() / TAU;
        let hz = self.fc + d + slope / TAU;
        let held = hz.clamp(self.capture.0, self.capture.1.max(self.capture.0));
        self.delta = held - self.fc;
        Some(Estimate { hz: held, sigma_hz: sigma, power, clamped: held != hz })
    }

    /// Power (amplitude² of the baseband component) of one Hann frame of `t` seconds ending now, at an
    /// offset `d` Hz from fc. 0 when the ring does not hold it.
    pub fn frame_power(&self, t: f64, d: f64) -> f64 {
        let n = ((t * self.fb()).round() as usize).max(4);
        if (n as u64) > self.m || (n as u64) > self.m - self.oldest() {
            return 0.0;
        }
        let ((xr, xi), sw) = self.frame(self.m - n as u64, n, d, false);
        (xr * xr + xi * xi) / (sw * sw).max(1e-300)
    }

    /// Loads the Blackman-Harris-windowed window (`n` ring samples from `i0`) into the scratch buffer;
    /// returns Σw.
    fn load_bh(&mut self, i0: u64, n: usize) -> f64 {
        let th = TAU / (n.max(2) - 1) as f64;
        let (tc, ts) = (th.cos(), th.sin());
        let (mut wc, mut ws) = (1.0f64, 0.0f64);
        let mut sw = 0.0;
        for k in 0..n {
            let c2 = 2.0 * wc * wc - 1.0;
            let c3 = (4.0 * wc * wc - 3.0) * wc;
            let w = BH[0] - BH[1] * wc + BH[2] * c2 - BH[3] * c3;
            let (zr, zi) = self.ring[((i0 + k as u64) as usize) % RING];
            self.work[k] = (w * zr, w * zi);
            sw += w;
            let nwc = wc * tc - ws * ts;
            ws = wc * ts + ws * tc;
            wc = nwc;
        }
        sw
    }

    /// Σ s_k e^(−j2π·d·(i0 + k) / fb) over the first `n` entries of `s` (a loaded window or its chunks,
    /// `step` ring samples apart): |·|².
    fn dft_power(s: &[(f64, f64)], i0: f64, step: f64, d: f64, fb: f64) -> f64 {
        let ph = -TAU * d * i0 / fb;
        let (mut rc, mut rs) = (ph.cos(), ph.sin());
        let w0 = -TAU * d * step / fb;
        let (sc, ss) = (w0.cos(), w0.sin());
        let (mut xr, mut xi) = (0.0, 0.0);
        for &(zr, zi) in s {
            xr += zr * rc - zi * rs;
            xi += zr * rs + zi * rc;
            let nrc = rc * sc - rs * ss;
            rs = rc * ss + rs * sc;
            rc = nrc;
        }
        xr * xr + xi * xi
    }

    /// Poly peak search: a Blackman-Harris DFT of the last `t` seconds (not before index `from`) at
    /// fc · 2^(c / 1200) for c = −`band` … +`band` in `step` cents. The windowed samples are summed in
    /// fours first (the grid spans ±0.04 fc at fb ≈ 4 fc: a smooth taper of the grid, < 0.3 %, which
    /// [`Self::refine`] does not have). Returns (Hz, power, at the grid's edge), or None when the
    /// window does not fit.
    pub fn grid_peak(&mut self, t: f64, from: u64, band: f64, step: f64) -> Option<(f64, f64, bool)> {
        let (i0, n) = self.window(t, from)?;
        let sw = self.load_bh(i0, n);
        let fb = self.fb();
        const M: usize = 4;
        let nc = n.div_ceil(M);
        for c in 0..nc {
            let (mut r, mut i) = (0.0, 0.0);
            for k in c * M..((c + 1) * M).min(n) {
                r += self.work[k].0;
                i += self.work[k].1;
            }
            self.chunks[c] = (r, i);
        }
        let t0 = i0 as f64 + 0.5 * (M - 1) as f64;
        let k = (band / step).round() as i32;
        let mut best = (0.0f64, -1.0f64, 0i32);
        for j in -k..=k {
            let d = self.fc * ((j as f64 * step) / 1200.0).exp2() - self.fc;
            let p = Self::dft_power(&self.chunks[..nc], t0, M as f64, d, fb) / (sw * sw).max(1e-300);
            if p > best.1 {
                best = (d, p, j);
            }
        }
        let p = Self::dft_power(&self.work[..n], i0 as f64, 1.0, best.0, fb) / (sw * sw).max(1e-300);
        Some((self.fc + best.0, p, best.2.abs() == k))
    }

    /// Poly refinement around `hz`: the peak of the full window's Blackman-Harris magnitude, by
    /// Gaussian interpolation of three points one bin (1 / T) apart, recentred three times. The magnitude
    /// of a real-enveloped tone is symmetric about its frequency, so a decay does not bias it, and a
    /// neighbour outside the main lobe (±4 / T) is under the −92 dB side lobes. Returns (Hz, power).
    pub fn refine(&mut self, t: f64, from: u64, hz: f64) -> Option<(f64, f64)> {
        let (i0, n) = self.window(t, from)?;
        let sw = self.load_bh(i0, n);
        let fb = self.fb();
        let bin = fb / n as f64;
        let norm = 1.0 / (sw * sw).max(1e-300);
        let s = &self.work[..n];
        let mag = |d: f64| Self::dft_power(s, i0 as f64, 1.0, d, fb) * norm;
        let mut d = hz - self.fc;
        let mut p0 = mag(d);
        for _ in 0..3 {
            let (pm, pp) = (mag(d - bin), mag(d + bin));
            let (lm, l0, lp) = (pm.max(1e-300).ln(), p0.max(1e-300).ln(), pp.max(1e-300).ln());
            let den = 2.0 * l0 - lm - lp;
            if den <= 0.0 {
                break;
            }
            d += (0.5 * bin * (lp - lm) / den).clamp(-bin, bin);
            p0 = mag(d);
        }
        Some((self.fc + d, p0))
    }

    /// The design's refinement (README §2.5 step 3): the phase difference between two half-overlapping
    /// Blackman-Harris frames of 2T / 3 at the frequency, iterated twice (the phase-vocoder estimator).
    /// Its frames are a third shorter than the window, so a neighbour 4 / T away sits inside their main
    /// lobe: kept for comparison, [`Self::refine`] is the one used. Returns (Hz, power).
    pub fn refine_pv(&self, t: f64, from: u64, hz: f64) -> Option<(f64, f64)> {
        let (i0, n) = self.window(t, from)?;
        let fb = self.fb();
        let nl = 2 * n / 3;
        let h = n - nl;
        let mut d = hz - self.fc;
        for _ in 0..2 {
            let ((ar, ai), _) = self.frame(i0, nl, d, true);
            let ((br, bi), _) = self.frame(i0 + h as u64, nl, d, true);
            // B · conj(A)
            let (cr, ci) = (br * ar + bi * ai, bi * ar - br * ai);
            d += ci.atan2(cr) / (TAU * h as f64 / fb);
        }
        let ((xr, xi), sw) = self.frame(i0, n, d, true);
        Some((self.fc + d, (xr * xr + xi * xi) / (sw * sw).max(1e-300)))
    }

    /// The last `t` seconds as (first absolute index, length), clipped at `from` and at the ring.
    fn window(&self, t: f64, from: u64) -> Option<(u64, usize)> {
        let from = from.max(self.oldest());
        let n = (t * self.fb()).round() as u64;
        if self.m < from + 8 {
            return None;
        }
        let n = n.min(self.m - from).max(8);
        Some((self.m - n, n as usize))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plucked-string-like tone at fd: partials 1 / k up to `top` Hz, an optional neighbour.
    fn tone(f: f64, fd: f64, secs: f64, neighbour: Option<(f64, f64)>) -> Vec<f32> {
        let n = (secs * fd) as usize;
        (0..n)
            .map(|i| {
                let t = i as f64 / fd;
                let mut s = 0.0;
                let mut k = 1;
                while k as f64 * f < 2500.0 && k < 40 {
                    s += (TAU * f * k as f64 * t + 0.3 * k as f64).sin() / k as f64;
                    k += 1;
                }
                if let Some((g, a)) = neighbour {
                    s += a * (TAU * g * t + 1.1).sin();
                }
                (0.2 * s) as f32
            })
            .collect()
    }

    #[test]
    fn mono_reads_a_clean_note_to_a_hundredth_of_a_cent() {
        let fd = 12000.0;
        for (f, off) in [(30.868f64, -37.0f64), (82.407, 12.0), (329.63, -4.0), (1318.5, 30.0)] {
            let truth = f * (off / 1200.0f64).exp2();
            let x = tone(truth, fd, 1.5, None);
            let mut z = Zoom::new();
            z.prepare(fd as f32);
            z.start(f * (4.0f64 / 1200.0).exp2()); // the coarse reading, 4 ¢ off the note heard
            let t = (24.0 / f).clamp(0.08, 0.6);
            let mut last = None;
            for (i, v) in x.iter().enumerate() {
                z.push(*v);
                if i % 120 == 119 && z.elapsed() > t + 3.0 / f {
                    last = z.estimate(t, (2.0 / f * z.fb()) as u64);
                }
            }
            let e = last.expect("an estimate");
            let err = cents(e.hz / truth);
            let sig = cents((truth + e.sigma_hz) / truth);
            assert!(err.abs() < 0.01, "{f} Hz {off:+} ¢: {err:+.4} ¢");
            assert!(sig < 0.05, "{f} Hz: σ {sig:.4} ¢");
        }
    }

    #[test]
    fn mono_rejects_a_neighbour_and_flags_it() {
        // A2 read with a partial 200 ¢ below at 0 dB: the 48-period window holds 0.2 ¢ and σ rises
        let fd = 12000.0;
        let f = 110.0;
        let x = tone(f, fd, 2.0, Some((98.0, 1.0)));
        let mut z = Zoom::new();
        z.prepare(fd as f32);
        z.start(f);
        let t = (48.0 / f).clamp(0.3, 1.2);
        let mut worst = 0.0f64;
        let mut sig = 0.0f64;
        for (i, v) in x.iter().enumerate() {
            z.push(*v);
            if i % 120 == 119 && z.elapsed() > t + 0.05 {
                let e = z.estimate(t, 0).unwrap();
                worst = worst.max(cents(e.hz / f).abs());
                sig = sig.max(cents((f + e.sigma_hz) / f));
            }
        }
        assert!(worst < 0.2, "neighbour pulled {worst:.3} ¢");
        assert!(sig > 0.01, "σ {sig:.4} ¢ did not rise");
    }

    #[test]
    fn poly_grid_and_refine_find_a_string_off_target() {
        let fd = 12000.0;
        let target = 110.0;
        let truth = target * (23.0f64 / 1200.0).exp2();
        let x = tone(truth, fd, 0.6, Some((82.407 * 2.0, 0.5)));
        let mut z = Zoom::new();
        z.prepare(fd as f32);
        z.start(target);
        for v in x {
            z.push(v);
        }
        let (hz, p, edge) = z.grid_peak(0.3, 0, 60.0, 2.5).unwrap();
        assert!(!edge && p > 0.0);
        assert!(cents(hz / truth).abs() < 2.5, "grid {:.2} ¢", cents(hz / truth));
        let (hz, _) = z.refine(0.3, 0, hz).unwrap();
        assert!(cents(hz / truth).abs() < 0.05, "refined {:.4} ¢", cents(hz / truth));
        // a band with nothing in it peaks at the edge or far below
        let mut q = Zoom::new();
        q.prepare(fd as f32);
        q.start(196.0);
        for v in tone(82.407, fd, 0.6, None) {
            q.push(v);
        }
        let (_, pq, _) = q.grid_peak(0.3, 0, 60.0, 2.5).unwrap();
        assert!(10.0 * (pq / p).log10() < -40.0);
    }

    #[test]
    fn poly_refine_holds_a_neighbour_five_lobes_away() {
        // A1 beside E1 brought 60 ¢ closer, the window T = 5 / Δ: the full-window magnitude peak holds;
        // the two-frame phase estimator of the design (frames of 2T / 3) has the neighbour in its main
        // lobe
        let fd = 12000.0;
        let a = 55.0 * (12.0f64 / 1200.0).exp2();
        let e = 41.203 * (72.0f64 / 1200.0).exp2();
        let t = 5.0 / (a - e);
        let n = (1.0 * fd) as usize;
        let x: Vec<f32> = (0..n)
            .map(|i| {
                let s = i as f64 / fd;
                (0.2 * (-s / 2.0).exp() * ((TAU * a * s).sin() + (TAU * e * s + 1.0).sin())) as f32
            })
            .collect();
        let mut z = Zoom::new();
        z.prepare(fd as f32);
        z.start(55.0);
        for v in x {
            z.push(v);
        }
        let (g, _, edge) = z.grid_peak(t, 0, 60.0, 2.5).unwrap();
        assert!(!edge);
        let (r, _) = z.refine(t, 0, g).unwrap();
        let (pv, _) = z.refine_pv(t, 0, g).unwrap();
        let (er, epv) = (cents(r / a).abs(), cents(pv / a).abs());
        assert!(er < 0.05, "refine {er:.3} ¢");
        assert!(epv > 5.0 * er, "the two-frame estimator {epv:.3} ¢ against {er:.3} ¢");
    }

    #[test]
    fn the_oscillator_does_not_drift() {
        // 10 minutes at fd = 12 kHz: the rotator's phase against the exact one
        let mut z = Zoom::new();
        z.prepare(12000.0);
        z.start(82.407);
        for _ in 0..12000 * 600 {
            z.push(0.0);
        }
        let ph = (z.n as f64 * z.inc).fract();
        let exact = ((TAU * ph).cos(), -(TAU * ph).sin());
        let err = ((z.rot.0 - exact.0).powi(2) + (z.rot.1 - exact.1).powi(2)).sqrt();
        assert!(err < 1e-9, "rotator off by {err:.2e}");
    }
}
