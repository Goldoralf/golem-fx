// SPDX-License-Identifier: GPL-3.0-or-later
//! Direct-form FIR at zero latency (gfx-acoustic §2.5): output sample 0 depends on input sample 0, no
//! partitioning, no FFT. For short kernels (≈ 1,000 – 4,000 taps) where a partitioned convolution's
//! latency or block cost is not wanted.
//!
//! - **History** as a doubled ring (each sample written twice): the last N inputs are always one
//!   contiguous slice, so one dot product per output.
//! - **Dot product**: AVX2 + FMA when the CPU has them (runtime detection, cached at `prepare`), four
//!   8-lane accumulators; otherwise a portable 8-accumulator loop the compiler vectorises (SSE).
//!   Both are deterministic on one machine; they may differ in the last bits from each other.
//! - **Kernel blend**: a linear crossfade of two FIRs fed the same input is one FIR with blended taps
//!   ([`blend`]); a moving blend is rebuilt per control block and the old and new kernels' outputs
//!   crossfaded over that block (the caller runs [`Fir::process2`]).
//! - **Resampling** of a kernel measured at one rate to another ([`resample_kernel`]): windowed-sinc
//!   interpolation, band-limited to the lower Nyquist, scaled so the frequency response is kept.
//!   Done once outside the audio path.

/// A direct-form FIR whose kernel the caller owns (several kernels may share one history).
#[derive(Clone, Debug, Default)]
pub struct Fir {
    hist: Vec<f32>,
    n: usize,
    pos: usize,
    avx: bool,
}

impl Fir {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sizes the history for kernels of `taps` (a multiple of 32 is fastest).
    pub fn prepare(&mut self, taps: usize) {
        self.n = taps.max(1);
        self.hist = vec![0.0; 2 * self.n];
        self.pos = 0;
        self.avx = has_avx2();
    }

    pub fn reset(&mut self) {
        self.hist.iter_mut().for_each(|v| *v = 0.0);
        self.pos = 0;
    }

    pub fn taps(&self) -> usize {
        self.n
    }

    /// Takes another FIR's history (same length): a kernel switched in starts warm, without the
    /// transient of an input that would seem to start from silence.
    pub fn copy_history_from(&mut self, other: &Fir) {
        if other.n == self.n {
            self.hist.copy_from_slice(&other.hist);
            self.pos = other.pos;
        }
    }

    /// Pushes one input sample.
    #[inline]
    pub fn push(&mut self, x: f32) {
        self.hist[self.pos] = x;
        self.hist[self.pos + self.n] = x;
        self.pos += 1;
        if self.pos == self.n {
            self.pos = 0;
        }
    }

    /// The last N inputs, oldest first.
    #[inline]
    pub fn window(&self) -> &[f32] {
        &self.hist[self.pos..self.pos + self.n]
    }

    /// The output for a kernel stored **reversed** (`rev[i] = h[N − 1 − i]`, see [`reverse`]), after
    /// `push`.
    #[inline]
    pub fn process(&self, rev: &[f32]) -> f32 {
        dot_with(self.window(), rev, self.avx)
    }

    /// Two kernels' outputs on the same history (a kernel crossfade).
    #[inline]
    pub fn process2(&self, a: &[f32], b: &[f32]) -> (f32, f32) {
        let w = self.window();
        (dot_with(w, a, self.avx), dot_with(w, b, self.avx))
    }
}

/// Whether the AVX2 + FMA path runs on this machine.
pub fn has_avx2() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// Σ a[i] · b[i] (equal lengths).
#[inline]
pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    dot_with(a, b, has_avx2())
}

#[inline]
fn dot_with(a: &[f32], b: &[f32], avx: bool) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    #[cfg(target_arch = "x86_64")]
    {
        if avx {
            // SAFETY: the CPU's AVX2 and FMA support was checked at `prepare` (or just now)
            return unsafe { dot_avx2(a, b) };
        }
    }
    let _ = avx;
    dot_portable(a, b)
}

fn dot_portable(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    let mut acc = [0.0f32; 8];
    let chunks = n / 8;
    for c in 0..chunks {
        let (x, y) = (&a[c * 8..c * 8 + 8], &b[c * 8..c * 8 + 8]);
        for k in 0..8 {
            acc[k] += x[k] * y[k];
        }
    }
    let mut s = (acc[0] + acc[4]) + (acc[1] + acc[5]) + (acc[2] + acc[6]) + (acc[3] + acc[7]);
    for i in chunks * 8..n {
        s += a[i] * b[i];
    }
    s
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn dot_avx2(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::x86_64::*;
    let n = a.len().min(b.len());
    let (pa, pb) = (a.as_ptr(), b.as_ptr());
    let mut s0 = _mm256_setzero_ps();
    let mut s1 = _mm256_setzero_ps();
    let mut s2 = _mm256_setzero_ps();
    let mut s3 = _mm256_setzero_ps();
    let mut i = 0;
    while i + 32 <= n {
        s0 = _mm256_fmadd_ps(_mm256_loadu_ps(pa.add(i)), _mm256_loadu_ps(pb.add(i)), s0);
        s1 = _mm256_fmadd_ps(_mm256_loadu_ps(pa.add(i + 8)), _mm256_loadu_ps(pb.add(i + 8)), s1);
        s2 = _mm256_fmadd_ps(_mm256_loadu_ps(pa.add(i + 16)), _mm256_loadu_ps(pb.add(i + 16)), s2);
        s3 = _mm256_fmadd_ps(_mm256_loadu_ps(pa.add(i + 24)), _mm256_loadu_ps(pb.add(i + 24)), s3);
        i += 32;
    }
    while i + 8 <= n {
        s0 = _mm256_fmadd_ps(_mm256_loadu_ps(pa.add(i)), _mm256_loadu_ps(pb.add(i)), s0);
        i += 8;
    }
    let s = _mm256_add_ps(_mm256_add_ps(s0, s1), _mm256_add_ps(s2, s3));
    let lo = _mm_add_ps(_mm256_castps256_ps128(s), _mm256_extractf128_ps(s, 1));
    let h = _mm_add_ps(lo, _mm_movehl_ps(lo, lo));
    let h = _mm_add_ss(h, _mm_shuffle_ps(h, h, 1));
    let mut r = _mm_cvtss_f32(h);
    while i < n {
        r += *pa.add(i) * *pb.add(i);
        i += 1;
    }
    r
}

/// A kernel reversed for [`Fir::process`], zero-padded to `taps`.
pub fn reverse(h: &[f32], taps: usize) -> Vec<f32> {
    let mut r = vec![0.0; taps];
    for (i, v) in h.iter().take(taps).enumerate() {
        r[taps - 1 - i] = *v;
    }
    r
}

/// out = scale · ((1 − t) · a + t · b), tap by tap.
pub fn blend(out: &mut [f32], a: &[f32], b: &[f32], t: f32, scale: f32) {
    let (wa, wb) = ((1.0 - t) * scale, t * scale);
    for ((o, x), y) in out.iter_mut().zip(a).zip(b) {
        *o = wa * x + wb * y;
    }
}

/// A kernel `h` sampled at `from` Hz, re-sampled at `to` Hz (windowed sinc, 32 zero crossings,
/// band-limited to the lower Nyquist) so its frequency response is kept. Length ⌈len · to / from⌉.
pub fn resample_kernel(h: &[f32], from: f64, to: f64) -> Vec<f32> {
    if (from - to).abs() < 1e-9 {
        return h.to_vec();
    }
    let ratio = from / to;
    let len = ((h.len() as f64) / ratio).ceil() as usize;
    let c = (to / from).min(1.0);
    let half = 16.0 / c;
    let mut out = vec![0.0f32; len];
    for (n, o) in out.iter_mut().enumerate() {
        let t = n as f64 * ratio;
        let lo = ((t - half).ceil().max(0.0)) as usize;
        let hi = ((t + half).floor() as usize).min(h.len().saturating_sub(1));
        let mut acc = 0.0f64;
        for k in lo..=hi {
            let x = t - k as f64;
            let arg = c * x;
            let sinc = if arg.abs() < 1e-12 { 1.0 } else { (std::f64::consts::PI * arg).sin() / (std::f64::consts::PI * arg) };
            let w = 0.5 + 0.5 * (std::f64::consts::PI * x / half).cos();
            acc += h[k] as f64 * c * sinc * w;
        }
        *o = (acc * ratio) as f32;
    }
    out
}

/// A **causal** kernel (its energy from tap 0, as a zero-latency body's) at `from` Hz, re-sampled at
/// `to` Hz without losing its response. [`resample_kernel`]'s band-limited interpolation would put
/// pre-ringing before tap 0, which a causal kernel cannot hold: up by an integer ratio r the kernel is
/// zero-stuffed instead: exactly H(f), magnitude and phase, below `from` / 2, and its mirror image
/// above (it filters only what the input holds there: a DI has next to nothing). No anti-image
/// filter: its phase (≈ 40 µs of group delay for a Butterworth-8 at 21 kHz) would turn the kernel
/// against a parallel path (gfx-acoustic's modal bank: up to 2 dB of error at 2.5 kHz, measured).
/// 88.2 / 176.4 kHz go through 44.1 kHz first. Other ratios use [`resample_kernel`].
pub fn resample_kernel_causal(h: &[f32], from: f64, to: f64) -> Vec<f32> {
    let r = to / from;
    let ri = r.round();
    if ri >= 2.0 && (r - ri).abs() < 1e-9 {
        let n = ri as usize;
        let mut z = vec![0.0f32; h.len() * n];
        for (i, v) in h.iter().enumerate() {
            z[i * n] = *v;
        }
        return z;
    }
    let base = to / 44100.0;
    if (from - 44100.0).abs() > 1e-9 && base.round() >= 2.0 && (base - base.round()).abs() < 1e-9 {
        let mid = resample_kernel(h, from, 44100.0);
        return resample_kernel_causal(&mid, 44100.0, to);
    }
    resample_kernel(h, from, to)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naive(h: &[f32], x: &[f32]) -> Vec<f32> {
        (0..x.len()).map(|n| (0..h.len()).filter(|k| *k <= n).map(|k| h[k] as f64 * x[n - k] as f64).sum::<f64>() as f32).collect()
    }

    fn noise(n: usize, seed: u32) -> Vec<f32> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                (s >> 8) as f32 / (1u32 << 24) as f32 - 0.5
            })
            .collect()
    }

    #[test]
    fn matches_the_convolution_from_sample_0() {
        let h = noise(1024, 1);
        let x = noise(3000, 2);
        let rev = reverse(&h, 1024);
        let mut f = Fir::new();
        f.prepare(1024);
        let want = naive(&h, &x);
        for (n, &v) in x.iter().enumerate() {
            f.push(v);
            let y = f.process(&rev);
            assert!((y - want[n]).abs() < 1e-4, "{n}: {y} vs {}", want[n]);
        }
        // an impulse: the kernel itself, tap 0 at sample 0 (zero latency)
        f.reset();
        for n in 0..1024 {
            f.push(if n == 0 { 1.0 } else { 0.0 });
            assert!((f.process(&rev) - h[n]).abs() < 1e-7);
        }
    }

    #[test]
    fn simd_and_portable_agree() {
        let a = noise(2048, 3);
        let b = noise(2048, 4);
        let p = dot_portable(&a, &b);
        let d = dot(&a, &b);
        assert!((p - d).abs() < 1e-4, "{p} vs {d}");
    }

    #[test]
    fn a_blend_is_the_blend_of_outputs() {
        let (h1, h2) = (noise(256, 5), noise(256, 6));
        let mut k = vec![0.0; 256];
        blend(&mut k, &h1, &h2, 0.3, 1.0);
        let x = noise(500, 7);
        let (a, b, m) = (naive(&h1, &x), naive(&h2, &x), naive(&k, &x));
        for n in 0..500 {
            assert!((m[n] - (0.7 * a[n] + 0.3 * b[n])).abs() < 1e-5);
        }
    }

    #[test]
    fn causal_resampling_keeps_a_kernel_that_starts_at_tap_0() {
        // an impulse at tap 0 plus a decaying noise: the response within 0.1 dB to 15 kHz
        let mut h = noise(1024, 9);
        for (n, v) in h.iter_mut().enumerate() {
            *v *= (-(n as f32) / 200.0).exp();
        }
        h[0] = 3.0;
        let gain = |k: &[f32], fs: f64, f: f64| {
            let w = 2.0 * std::f64::consts::PI * f / fs;
            let (mut re, mut im) = (0.0, 0.0);
            for (n, v) in k.iter().enumerate() {
                re += *v as f64 * (w * n as f64).cos();
                im -= *v as f64 * (w * n as f64).sin();
            }
            20.0 * (re * re + im * im).sqrt().log10()
        };
        for to in [44100.0, 88200.0, 96000.0, 192000.0] {
            let r = resample_kernel_causal(&h, 48000.0, to);
            for f in [200.0, 1000.0, 3000.0, 7000.0, 12000.0, 15000.0] {
                let d = gain(&r, to, f) - gain(&h, 48000.0, f);
                // up by an integer ratio: exact but for the low-pass; 44.1 kHz: the interpolation's
                // pre-ringing before tap 0 is lost (−0.11 dB at 1 kHz on this kernel)
                let bound = if to % 48000.0 != 0.0 { 0.2 } else { 0.1 };
                assert!(d.abs() < bound, "{to} {f}: {d}");
            }
        }
    }

    #[test]
    fn resampling_keeps_the_response() {
        // a decaying 3 kHz resonance at 48 kHz → 96 / 44.1 kHz: the gain at 3 kHz within 0.1 dB
        let h: Vec<f32> = (0..1024).map(|n| ((n as f64 * 2.0 * std::f64::consts::PI * 3000.0 / 48000.0).sin() * (-(n as f64) / 100.0).exp()) as f32).collect();
        let gain = |k: &[f32], fs: f64| {
            let w = 2.0 * std::f64::consts::PI * 3000.0 / fs;
            let (mut re, mut im) = (0.0, 0.0);
            for (n, v) in k.iter().enumerate() {
                re += *v as f64 * (w * n as f64).cos();
                im -= *v as f64 * (w * n as f64).sin();
            }
            20.0 * (re * re + im * im).sqrt().log10()
        };
        let g0 = gain(&h, 48000.0);
        for to in [44100.0, 96000.0, 192000.0] {
            let r = resample_kernel(&h, 48000.0, to);
            assert_eq!(r.len(), (1024.0 * to / 48000.0f64).ceil() as usize);
            let g = gain(&r, to);
            assert!((g - g0).abs() < 0.1, "{to}: {g} vs {g0}");
        }
    }
}
