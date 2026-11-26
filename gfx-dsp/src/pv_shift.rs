// SPDX-License-Identifier: GPL-3.0-or-later
//! A low-latency polyphonic pitch shifter: a peak-locked phase vocoder (Laroche & Dolson 1999) on
//! asymmetric windows (Mauler & Martin 2007).
//!
//! - The analysis window is long (8192 at 48 kHz: a guitar's low chords are resolved) and rises
//!   slowly; it falls over the last `m` samples only. The synthesis window covers the last `2m` samples, so
//!   the latency is the synthesis window's (`2m − 1`), not the analysis length.
//! - Each spectral peak takes its region (the bins down to the minima on either side) up by the ratio as a
//!   whole: the region's bins keep their phases relative to the peak (identity phase locking), which
//!   keeps the partial's shape and its time envelope inside the frame. The peak's phase advances at the
//!   shifted frequency from frame to frame (continuity by the nearest previous peak).
//! - The frame is rotated so phases are referenced to the synthesis window's centre.
//! - Up to two voices share one analysis.
//!
//! Allocates in `prepare` only. Deterministic.

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::f32::consts::PI;
use std::sync::Arc;

type C = Complex<f32>;

pub const VOICES: usize = 2;

#[derive(Clone, Default)]
struct PvVoice {
    /// the synthesized phase of each peak (by analysis bin), this frame and the previous
    syn: Vec<f32>,
    syn_prev: Vec<f32>,
    y: Vec<C>,
    time: Vec<f32>,
    /// overlap-add of the last `2m` output samples; its first `r` are final after each frame
    ola: Vec<f32>,
    ready: Vec<f32>,
    active: bool,
}

pub struct PvShifter {
    modes: [(usize, usize); 2],
    fwd: Vec<Arc<dyn RealToComplex<f32>>>,
    inv: Vec<Arc<dyn ComplexToReal<f32>>>,
    plan: usize,
    n: usize,
    m: usize,
    r: usize,
    ring: Vec<f32>,
    pos: usize,
    wa: Vec<f32>,
    ws: Vec<f32>,
    frame: Vec<f32>,
    spec: Vec<C>,

    mag: Vec<f32>,
    scratch: Vec<C>,
    peaks: Vec<usize>,
    prev_peaks: Vec<usize>,
    bounds: Vec<usize>,
    /// per bin: the analysis phase of this frame's peaks and the previous frame's
    phase: Vec<f32>,
    phase_prev: Vec<f32>,
    /// per peak: the previous frame's peak it continues (`usize::MAX`: a new one)
    near: Vec<usize>,
    lin: Vec<f32>,
    voices: [PvVoice; VOICES],
    count: usize,
    out_i: usize,
    ratio: [f32; VOICES],
    gain: f32,
}

impl Default for PvShifter {
    fn default() -> Self {
        Self {
            modes: [(0, 0); 2],
            fwd: Vec::new(),
            inv: Vec::new(),
            plan: 0,
            n: 0,
            m: 0,
            r: 0,
            ring: Vec::new(),
            pos: 0,
            wa: Vec::new(),
            ws: Vec::new(),
            frame: Vec::new(),
            spec: Vec::new(),

            mag: Vec::new(),
            scratch: Vec::new(),
            peaks: Vec::new(),
            prev_peaks: Vec::new(),
            bounds: Vec::new(),
            phase: Vec::new(),
            phase_prev: Vec::new(),
            near: Vec::new(),
            lin: Vec::new(),
            voices: Default::default(),
            count: 0,
            out_i: 0,
            ratio: [1.0; VOICES],
            gain: 0.0,
        }
    }
}

/// At ratio 1 the synthesis phases return to the analysis phases: the offset is multiplied by this per
/// hop (two neighbouring frames differ by at most a quarter of it: no dip in the overlap-add), and below
/// `UNISON_SNAP` rad it is dropped (from π: 36 hops, 120 ms Fast, 180 ms Stable).
const UNISON_DECAY: f32 = 0.75;
const UNISON_SNAP: f32 = 1e-4;

/// Peaks below this (relative to the frame's largest) are left to their neighbours' regions.
const PEAK_FLOOR: f32 = 1e-8;
/// atan2 to ~1e-5 rad (a minimax polynomial for atan on [0, 1] and the octant folds): the phases of a
/// noisy frame are thousands of peaks, and libm's atan2 was most of the cost.
#[inline]
fn fast_atan2(y: f32, x: f32) -> f32 {
    let (ax, ay) = (x.abs(), y.abs());
    let (mn, mx) = if ax < ay { (ax, ay) } else { (ay, ax) };
    if mx == 0.0 {
        return 0.0;
    }
    let a = mn / mx;
    let s = a * a;
    let mut r = a * (0.999_977_26 + s * (-0.332_623_47 + s * (0.193_543_46 + s * (-0.116_432_87 + s * (0.052_653_32 + s * -0.011_721_2)))));
    if ay > ax {
        r = core::f32::consts::FRAC_PI_2 - r;
    }
    if x < 0.0 {
        r = PI - r;
    }
    if y < 0.0 {
        -r
    } else {
        r
    }
}

/// e^{iθ} for θ in [−π, π] to ~4e-6 (folded to [−π/2, π/2], Taylor to the 9th / 10th order).
#[inline]
fn fast_cis(t: f32) -> C {
    let h = core::f32::consts::FRAC_PI_2;
    let (u, flip) = if t > h { (PI - t, true) } else if t < -h { (-PI - t, true) } else { (t, false) };
    let u2 = u * u;
    let sn = u * (1.0 + u2 * (-1.0 / 6.0 + u2 * (1.0 / 120.0 + u2 * (-1.0 / 5040.0 + u2 * (1.0 / 362_880.0)))));
    let cs = 1.0 + u2 * (-0.5 + u2 * (1.0 / 24.0 + u2 * (-1.0 / 720.0 + u2 * (1.0 / 40320.0 + u2 * (-1.0 / 3_628_800.0)))));
    C::new(if flip { -cs } else { cs }, sn)
}

/// `x` wrapped to about [−π, π] (to the nearest turn; |x| below ~6000 rad): an integer cast, since
/// `round` is a libm call on baseline x86-64.
#[inline]
fn wrap(x: f32) -> f32 {
    const INV: f32 = 1.0 / (2.0 * PI);
    let k = (x * INV + 1024.5) as i32 - 1024;
    x - 2.0 * PI * k as f32
}

/// The modes at 48 kHz: (analysis length, synthesis half-length). Fast: 6.7 ms; Stable: 10 ms.
const MODES_48K: [(f64, f64); 2] = [(8192.0, 160.0), (8192.0, 240.0)];

impl PvShifter {
    /// The modes at `sr`: (analysis length, a power of two; synthesis half-length `m`). Latency `2m − 1`.
    pub fn modes(sr: f32) -> [(usize, usize); 2] {
        let k = sr as f64 / 48000.0;
        MODES_48K.map(|(n, m)| (((n * k * 0.95).round() as usize).next_power_of_two(), ((m * k).round() as usize).max(16)))
    }

    /// Plans both modes and sizes every buffer for the larger (allocates).
    pub fn prepare(&mut self, sr: f32) {
        let modes = Self::modes(sr);
        self.modes = modes;
        let mut planner = RealFftPlanner::<f32>::new();
        self.fwd = modes.iter().map(|&(n, _)| planner.plan_fft_forward(n)).collect();
        self.inv = modes.iter().map(|&(n, _)| planner.plan_fft_inverse(n)).collect();
        let nmax = modes.iter().map(|x| x.0).max().unwrap_or(0);
        let m = modes.iter().map(|x| x.1).max().unwrap_or(0);
        let kmax = nmax / 2 + 1;
        let scr = self.fwd.iter().map(|f| f.get_scratch_len()).chain(self.inv.iter().map(|f| f.get_scratch_len())).max().unwrap_or(0);
        self.ring = vec![0.0; nmax];
        self.wa = vec![0.0; nmax];
        self.ws = vec![0.0; 2 * m];
        self.frame = vec![0.0; nmax];
        self.spec = vec![C::default(); kmax];

        self.mag = vec![0.0; kmax];
        self.scratch = vec![C::default(); scr.max(1)];
        self.peaks = Vec::with_capacity(kmax);
        self.prev_peaks = Vec::with_capacity(kmax);
        self.bounds = Vec::with_capacity(kmax + 1);
        self.phase = vec![0.0; kmax];
        self.phase_prev = vec![0.0; kmax];
        self.near = Vec::with_capacity(kmax);
        self.lin = vec![0.0; nmax];
        for v in self.voices.iter_mut() {
            v.syn = vec![0.0; kmax];
            v.syn_prev = vec![0.0; kmax];
            v.y = vec![C::default(); kmax];
            v.time = vec![0.0; nmax];
            v.ola = vec![0.0; 2 * m];
            v.ready = vec![0.0; m];
        }
        self.configure(0);
    }

    /// Picks the mode: 0 = Fast, 1 = Stable. No allocation; resets.
    pub fn configure(&mut self, mode: usize) {
        self.plan = mode.min(1);
        let (n, m) = self.modes[self.plan];
        self.n = n;
        self.m = m;
        // hop m: the Hann (2m) overlaps twice (a hop of m / 2 measured the same on chords, at twice the cost)
        self.r = m;
        let l = n - m;
        for j in 0..l {
            self.wa[j] = (0.5 - 0.5 * (PI * j as f32 / l as f32).cos()).sqrt();
        }
        for t in 0..m {
            self.wa[l + t] = (0.5 + 0.5 * (PI * t as f32 / m as f32).cos()).sqrt();
        }
        for t in 0..2 * m {
            let h = 0.5 - 0.5 * (PI * t as f32 / m as f32).cos();
            self.ws[t] = h / self.wa[n - 2 * m + t].max(1e-9);
        }
        // the Hann (2m) at hop r sums to m / r; the inverse FFT is unnormalised (× n)
        self.gain = self.r as f32 / (n as f32 * m as f32);
        self.reset();
    }

    /// Latency (samples): the synthesis window.
    pub fn latency(&self) -> usize {
        2 * self.m - 1
    }

    pub fn reset(&mut self) {
        self.ring.fill(0.0);
        self.pos = 0;

        self.peaks.clear();
        self.prev_peaks.clear();
        self.phase.fill(0.0);
        self.phase_prev.fill(0.0);
        for v in self.voices.iter_mut() {
            v.syn.fill(0.0);
            v.syn_prev.fill(0.0);
            v.ola.fill(0.0);
            v.ready.fill(0.0);
            v.active = false;
        }
        self.count = 0;
        self.out_i = 0;
    }

    /// The ratio of voice `v` (taken at the next frame).
    pub fn set_ratio(&mut self, v: usize, ratio: f32) {
        self.ratio[v] = ratio;
    }

    /// One sample in; each voice's output (`voices` voices run; the others are silent and restart fresh).
    #[inline]
    pub fn process(&mut self, x: f32, voices: usize) -> [f32; VOICES] {
        let n = self.n;
        self.ring[self.pos] = x;
        self.pos = (self.pos + 1) % n;
        self.count += 1;
        if self.count == self.r {
            self.count = 0;
            self.frame(voices);
            self.out_i = 0;
        }
        let mut y = [0.0; VOICES];
        for (k, v) in self.voices.iter().enumerate() {
            y[k] = v.ready[self.out_i];
        }
        self.out_i = (self.out_i + 1).min(self.r - 1);
        y
    }

    fn frame(&mut self, voices: usize) {
        let (n, m, r) = (self.n, self.m, self.r);
        let k_len = n / 2 + 1;
        // windowed, rotated so the synthesis centre (n − m) sits at index 0
        let start = self.pos; // the oldest sample
        let (old, new) = self.ring[..n].split_at(start);
        self.lin[..n - start].copy_from_slice(new);
        self.lin[n - start..n].copy_from_slice(old);
        for i in 0..m {
            self.frame[i] = self.wa[n - m + i] * self.lin[n - m + i];
        }
        for i in m..n {
            self.frame[i] = self.wa[i - m] * self.lin[i - m];
        }
        let spec = &mut self.spec[..k_len];
        let _ = self.fwd[self.plan].process_with_scratch(&mut self.frame[..n], spec, &mut self.scratch);
        // magnitudes, peaks, regions
        let mut top = 0.0f32;
        for (mg, c) in self.mag[..k_len].iter_mut().zip(spec.iter()) {
            *mg = c.norm_sqr();
            top = top.max(*mg);
        }
        self.peaks.clear();
        if top > 1e-20 {
            // branch-free: on noise a third of the bins are peaks, and a branch per bin mispredicts
            let floor = top * PEAK_FLOOR;
            let mg = &self.mag[..k_len];
            self.peaks.resize(k_len, 0);
            let mut cnt = 0usize;
            for k in 1..k_len - 1 {
                self.peaks[cnt] = k;
                cnt += ((mg[k] > floor) & (mg[k] > mg[k - 1]) & (mg[k] >= mg[k + 1])) as usize;
            }
            self.peaks.truncate(cnt);
        }
        self.bounds.clear();
        self.bounds.push(0);
        for w in self.peaks.windows(2) {
            let (a, b) = (w[0], w[1]);
            let mut lo = a;
            for k in a + 1..=b {
                if self.mag[k] < self.mag[lo] {
                    lo = k;
                }
            }
            self.bounds.push(lo);
        }
        self.bounds.push(k_len);
        // per peak, shared by the voices: its phase and the previous peak it continues
        self.near.clear();
        let mut j = 0usize;
        for &p in self.peaks.iter() {
            let x = spec[p];
            self.phase[p] = fast_atan2(x.im, x.re);
            while j + 1 < self.prev_peaks.len() && self.prev_peaks[j + 1] <= p {
                j += 1;
            }
            let mut near = usize::MAX;
            for &q in self.prev_peaks.get(j..(j + 2).min(self.prev_peaks.len())).unwrap_or(&[]) {
                if q.abs_diff(p) <= 2 && (near == usize::MAX || q.abs_diff(p) < near.abs_diff(p)) {
                    near = q;
                }
            }
            self.near.push(near);
        }
        // the advance a partial between bins q and p makes in one hop, near enough to unwrap by
        let adv_per_bin = PI * r as f32 / n as f32;
        for vi in 0..VOICES {
            let v = &mut self.voices[vi];
            if vi >= voices {
                if v.active {
                    v.active = false;
                    v.syn_prev.fill(0.0);
                    v.ola.fill(0.0);
                    v.ready.fill(0.0);
                }
                continue;
            }
            let fresh = !v.active;
            v.active = true;
            let a = self.ratio[vi];
            let y = &mut v.y[..k_len];
            y.fill(C::default());
            for (i, &p) in self.peaks.iter().enumerate() {
                let ph = self.phase[p];
                let near = self.near[i];
                // the phase advance from the previous frame's peak to this one, unwrapped around the
                // bins' frequency, scaled by the ratio: at ratio 1 the output is the input, exactly
                let th = if !fresh && near != usize::MAX {
                    if a == 1.0 {
                        // unison: the offset a sweep left between the synthesis and the analysis phase
                        // (kept as is, the output stays a comb of the input, 2 – 3 dB down on chords) decays
                        // by UNISON_DECAY per hop, then the phase is the analysis' and the output the input
                        let off = UNISON_DECAY * wrap(v.syn_prev[near] - self.phase_prev[near]);
                        if off.abs() <= UNISON_SNAP {
                            ph
                        } else {
                            ph + off
                        }
                    } else {
                        let adv = ph - self.phase_prev[near];
                        let expect = adv_per_bin * (p + near) as f32;
                        let adv = expect + wrap(adv - expect);
                        v.syn_prev[near] + a * adv
                    }
                } else {
                    ph
                };
                let th = wrap(th);
                v.syn[p] = th;
                let d = th - ph;
                let rot = fast_cis(wrap(d));
                let dst0 = (p as f32 * a).round() as isize - p as isize;
                // the region's bins whose destination lands in the spectrum
                let lo = (self.bounds[i] as isize).max(-dst0) as usize;
                let hi = (self.bounds[i + 1] as isize).min(k_len as isize - dst0).max(lo as isize) as usize;
                if hi > lo {
                    let d0 = (lo as isize + dst0) as usize;
                    for (yd, xs) in y[d0..d0 + (hi - lo)].iter_mut().zip(&spec[lo..hi]) {
                        *yd += *xs * rot;
                    }
                }
            }
            core::mem::swap(&mut v.syn, &mut v.syn_prev);
            // DC and Nyquist are real in a real signal's spectrum
            y[0].im = 0.0;
            y[k_len - 1].im = 0.0;
            let _ = self.inv[self.plan].process_with_scratch(y, &mut v.time[..n], &mut self.scratch);
            // overlap-add the last 2m samples (un-rotated: time index n − 2m + t sits at (n − m + t) mod n)
            let ola = &mut v.ola[..2 * m];
            ola.copy_within(r.., 0);
            ola[2 * m - r..].fill(0.0);
            let g = self.gain;
            for t in 0..m {
                v.ola[t] += v.time[n - m + t] * self.ws[t] * g;
            }
            for t in m..2 * m {
                v.ola[t] += v.time[t - m] * self.ws[t] * g;
            }
            v.ready[..r].copy_from_slice(&v.ola[..r]);
        }
        core::mem::swap(&mut self.peaks, &mut self.prev_peaks);
        core::mem::swap(&mut self.phase, &mut self.phase_prev);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(f: f32, n: usize, sr: f32) -> Vec<f32> {
        (0..n).map(|i| (2.0 * PI * f * i as f32 / sr).sin()).collect()
    }

    /// Amplitude at `f` (Hann-windowed correlation).
    fn amp(x: &[f32], f: f64, sr: f64) -> f64 {
        let n = x.len();
        let (mut re, mut im, mut ws) = (0.0f64, 0.0f64, 0.0f64);
        for (i, v) in x.iter().enumerate() {
            let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos();
            let ph = 2.0 * std::f64::consts::PI * f * i as f64 / sr;
            re += *v as f64 * w * ph.cos();
            im += *v as f64 * w * ph.sin();
            ws += w;
        }
        2.0 * (re * re + im * im).sqrt() / ws
    }

    #[test]
    fn unity_and_shift_and_latency() {
        for sr in [44100.0f32, 48000.0, 96000.0] {
            for size in 0..2 {
                let mut pv = PvShifter::default();
                pv.prepare(sr);
                pv.configure(size);
                // a click's arrival: the latency
                pv.set_ratio(0, 1.0);
                let n = (sr * 0.5) as usize;
                let x = tone(1000.0, n, sr);
                let y: Vec<f32> = x.iter().map(|v| pv.process(*v, 1)[0]).collect();
                let lat = pv.latency();
                let seg = &y[n / 2..];
                let xs = &x[n / 2 - lat..n - lat];
                let err = seg.iter().zip(xs).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
                assert!(err < 2e-3, "sr {sr} size {size}: unity error {err}");
                // a fifth up, in tune and at level
                for (f, st) in [(110.0f64, 7.0f64), (440.0, 12.0), (330.0, -5.0)] {
                    pv.reset();
                    let a = (st / 12.0).exp2();
                    pv.set_ratio(0, a as f32);
                    let n = sr as usize;
                    let x = tone(f as f32, n, sr);
                    let y: Vec<f32> = x.iter().map(|v| pv.process(*v, 1)[0]).collect();
                    let g = amp(&y[n / 2..], f * a, sr as f64);
                    assert!((g - 1.0).abs() < 0.05, "sr {sr} size {size} {f} Hz {st:+}: level {g:.3}");
                }
            }
        }
    }
}

#[cfg(test)]
mod approx {
    use super::*;

    #[test]
    fn fast_trig_is_accurate() {
        let mut worst = (0.0f32, 0.0f32);
        for i in 0..20000 {
            let t = -PI + 2.0 * PI * i as f32 / 20000.0;
            let (y, x) = (t.sin() * 3.7, t.cos() * 3.7);
            let d = fast_atan2(y, x) - y.atan2(x);
            worst.0 = worst.0.max((d - 2.0 * PI * (d / (2.0 * PI)).round()).abs());
            let c = fast_cis(t);
            worst.1 = worst.1.max((c.re - t.cos()).abs().max((c.im - t.sin()).abs()));
        }
        assert!(worst.0 < 2e-5 && worst.1 < 1e-5, "{worst:?}");
    }
}

#[cfg(test)]
mod profile {
    use super::*;
    use std::time::Instant;

    #[test]
    #[ignore]
    fn profile_pv_shift() {
        let sr = 48000.0f32;
        let n = 48000 * 5;
        let mut rng = crate::lfo::Rng::new(2);
        let noise: Vec<f32> = (0..n).map(|_| 0.3 * rng.bipolar()).collect();
        let chord: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / sr;
                [82.4f32, 123.5, 164.8, 207.7, 246.9, 329.6].iter().map(|f| (1..6).map(|h| (2.0 * PI * f * h as f32 * t).sin() / h as f32).sum::<f32>()).sum::<f32>() * 0.05
            })
            .collect();
        for size in 0..2 {
            let mut pv = PvShifter::default();
            pv.prepare(sr);
            pv.configure(size);
            let (fft_n, plan) = (pv.n, pv.plan);
            let mut buf = vec![0.0f32; fft_n];
            let mut spec = vec![C::default(); fft_n / 2 + 1];
            let mut scr = vec![C::default(); pv.scratch.len()];
            let t = Instant::now();
            for _ in 0..2000 {
                let _ = pv.fwd[plan].process_with_scratch(&mut buf, &mut spec, &mut scr);
                let _ = pv.inv[plan].process_with_scratch(&mut spec, &mut buf, &mut scr);
            }
            let fft_us = t.elapsed().as_secs_f64() * 1e6 / 2000.0;
            eprintln!("size {fft_n}: fwd+inv {fft_us:.1} us per frame = {:.0} ns / sample", fft_us * 1000.0 / pv.r as f64);
            for (name, x) in [("noise", &noise), ("chord", &chord)] {
                pv.reset();
                pv.set_ratio(0, 1.5);
                let t = Instant::now();
                let mut acc = 0.0;
                for v in x.iter() {
                    acc += pv.process(*v, 1)[0];
                }
                eprintln!("  {name}: {:.0} ns / sample ({acc:.1}), {} peaks in the last frame", t.elapsed().as_secs_f64() * 1e9 / n as f64, pv.prev_peaks.len());
                pv.reset();
                let t = Instant::now();
                for v in x.iter() {
                    acc += pv.process(*v, 0)[0];
                }
                eprintln!("    analysis only: {:.0} ns / sample ({acc:.1})", t.elapsed().as_secs_f64() * 1e9 / n as f64);
            }
        }
    }
}
