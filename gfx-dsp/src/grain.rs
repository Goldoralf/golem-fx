// SPDX-License-Identifier: GPL-3.0-or-later
//! Granular building blocks (design: gfx-grains/README.md §2.1 – §2.4), shared so that gfx-delay
//! (reverse, loop pitch) and gfx-shimmer can use them later.
//!
//! - [`HalfBand`]: the 51-tap Kaiser half-band (β for 80 dB) that makes a buffer's half-rate copy;
//!   group delay 25 input samples, constant, absorbed into read positions.
//! - [`MipBuffer`]: a 1- or 2-channel interleaved f32 ring (level 0) with its half-rate copy (level 1),
//!   each with a mirrored guard at its end so every kernel read is contiguous (no wrap test), and a
//!   frame counter from reset (the valid length: stale content is never read, nothing is zeroed).
//! - [`KernelBank`]: the ratio-tracking Kaiser-windowed sinc kernels, 129 phases each (linear
//!   interpolation between phases, every row normalised to a DC gain of 1): one for ρ ≤ 1 (pass 0.40,
//!   stop 0.60 of the level's rate) and 16 for ρ ∈ (1, 2] in steps of 2^(1/16) (pass 0.40 / ρ⁺, stop
//!   0.60 / ρ⁺), 32 taps up to ρ⁺ = √2 and 48 above. Read with the octave switch: level 0 for r < 2,
//!   level 1 for r ≥ 2 (so 1 ≤ ρ ≤ 2 there: a decimated level is never read below its own rate).
//! - [`Window`]: the grain window (flat → triangle → Hann by `shape`, peak moved by `skew`, raised-cosine
//!   edge guard on the flat and triangular parts, C¹ at both ends), its moments E[w], E[w²] and its
//!   Fourier transform (the variance of a periodic window sum).
//! - [`GrainPool`]: a fixed pool of voices; when full, a new grain is dropped, never stolen.
//!
//! Allocation in the constructors / `prepare` only; deterministic.

use std::f64::consts::PI;

/// Half-band taps.
pub const HB_TAPS: usize = 51;
/// Its group delay in level-0 samples: level-1 sample j stands for level-0 time 2j − [`L1_OFFSET`].
pub const HB_DELAY: usize = 25;
/// Level-1 sample j is centred on level-0 frame 2j − 24 (computed when frame 2j + 1 arrives).
pub const L1_OFFSET: f64 = 24.0;
/// Phase rows of every kernel (129 stored: the last row closes the interpolation).
pub const ROWS: usize = 128;
/// Kernels in the bank: 0 for ρ ≤ 1, j = 1 … 16 for ρ⁺ = 2^(j / 16).
pub const NK: usize = 17;
/// The longest kernel.
pub const MAX_TAPS: usize = 48;
/// Mirrored guard of every level (frames): the longest kernel, rounded up.
pub const GUARD: usize = 64;

/// Zeroth-order modified Bessel function of the first kind (power series).
pub fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let h = 0.25 * x * x;
    for k in 1..200 {
        term *= h / (k * k) as f64;
        sum += term;
        if term < sum * 1e-17 {
            break;
        }
    }
    sum
}

/// Kaiser's β for an attenuation of `a` dB.
pub fn kaiser_beta(a: f64) -> f64 {
    if a > 50.0 {
        0.1102 * (a - 8.7)
    } else if a > 21.0 {
        0.5842 * (a - 21.0).powf(0.4) + 0.07886 * (a - 21.0)
    } else {
        0.0
    }
}

#[inline]
fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}

// ---------------------------------------------------------------------------------------------
// Half-band

/// The 51-tap linear-phase half-band (Kaiser, β for 80 dB), normalised to a DC gain of 1. Its even
/// offsets from the centre are exactly 0, so a decimated output costs 13 multiplies (symmetry).
#[derive(Clone, Debug)]
pub struct HalfBand {
    /// h[25 ± k] for k = 1, 3, …, 25
    odd: [f32; 13],
    centre: f32,
    full: [f64; HB_TAPS],
}

impl Default for HalfBand {
    fn default() -> Self {
        Self::new()
    }
}

impl HalfBand {
    pub fn new() -> Self {
        let beta = kaiser_beta(80.0);
        let i0b = bessel_i0(beta);
        let mut full = [0.0f64; HB_TAPS];
        let c = HB_DELAY as f64;
        for (n, h) in full.iter_mut().enumerate() {
            let k = n as f64 - c;
            if (n as i64 - HB_DELAY as i64) % 2 == 0 && n != HB_DELAY {
                continue;
            }
            let r = k / c;
            *h = 0.5 * sinc(0.5 * k) * bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / i0b;
        }
        let s: f64 = full.iter().sum();
        full.iter_mut().for_each(|h| *h /= s);
        let mut odd = [0.0f32; 13];
        for (i, o) in odd.iter_mut().enumerate() {
            *o = full[HB_DELAY + 1 + 2 * i] as f32;
        }
        Self { odd, centre: full[HB_DELAY] as f32, full }
    }

    /// The taps (f64), for tests and offline references.
    pub fn taps(&self) -> &[f64; HB_TAPS] {
        &self.full
    }

    /// One output from 51 consecutive input samples, oldest first (`x[25]` is the centre).
    #[inline]
    pub fn apply(&self, x: impl Fn(usize) -> f32) -> f32 {
        let mut y = self.centre * x(HB_DELAY);
        for i in 0..13 {
            let k = 1 + 2 * i;
            y += self.odd[i] * (x(HB_DELAY - k) + x(HB_DELAY + k));
        }
        y
    }
}

// ---------------------------------------------------------------------------------------------
// Kernel bank

/// The ratio-tracking kernels (module doc). Row k of kernel j holds the taps for a fractional offset
/// k / 128: tap m weighs the level sample at floor(q) − taps / 2 + 1 + m.
#[derive(Clone, Debug)]
pub struct KernelBank {
    table: Vec<f32>,
    /// AVX2 and FMA available (the stereo read's wide path)
    avx: bool,
    offset: [usize; NK],
    taps: [usize; NK],
    pass: [f64; NK],
    stop: [f64; NK],
}

impl Default for KernelBank {
    fn default() -> Self {
        Self::new()
    }
}

impl KernelBank {
    pub fn new() -> Self {
        let mut offset = [0usize; NK];
        let mut taps = [0usize; NK];
        let mut pass = [0.0f64; NK];
        let mut stop = [0.0f64; NK];
        let mut total = 0;
        for j in 0..NK {
            let rp = Self::rho_plus(j);
            pass[j] = 0.40 / rp;
            stop[j] = 0.60 / rp;
            taps[j] = if rp <= std::f64::consts::SQRT_2 + 1e-9 { 32 } else { 48 };
            offset[j] = total;
            total += (ROWS + 1) * taps[j];
        }
        let mut table = vec![0.0f32; total];
        for j in 0..NK {
            let n = taps[j];
            let (p, s) = (pass[j], stop[j]);
            let fc = 0.5 * (p + s);
            let a = 2.285 * (n as f64 - 1.0) * 2.0 * PI * (s - p) + 8.0;
            let beta = kaiser_beta(a);
            let i0b = bessel_i0(beta);
            for k in 0..=ROWS {
                let mut row = [0.0f64; MAX_TAPS];
                let mut sum = 0.0;
                for m in 0..n {
                    let t = (m as f64 - n as f64 / 2.0 + 1.0) - k as f64 / ROWS as f64;
                    let r = 2.0 * t / n as f64;
                    let w = bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / i0b;
                    row[m] = 2.0 * fc * sinc(2.0 * fc * t) * w;
                    sum += row[m];
                }
                let base = offset[j] + k * n;
                for m in 0..n {
                    table[base + m] = (row[m] / sum) as f32;
                }
            }
        }
        #[cfg(target_arch = "x86_64")]
        let avx = std::is_x86_feature_detected!("avx2") && std::is_x86_feature_detected!("fma");
        #[cfg(not(target_arch = "x86_64"))]
        let avx = false;
        Self { table, avx, offset, taps, pass, stop }
    }

    /// The upper edge ρ⁺ of kernel j (1 for kernel 0).
    pub fn rho_plus(j: usize) -> f64 {
        (j as f64 / 16.0).exp2()
    }

    /// Kernel index for a within-level ratio ρ (> 0): 0 for ρ ≤ 1, else ⌈16 log2 ρ⌉ (≤ 16).
    #[inline]
    pub fn index_for(rho: f64) -> usize {
        if rho <= 1.0 {
            0
        } else {
            ((rho.log2() * 16.0 - 1e-9).ceil() as usize).clamp(1, NK - 1)
        }
    }

    /// Level and kernel for a total ratio |r| (octave switch: level 1 from r = 2).
    #[inline]
    pub fn choose(r: f64) -> (usize, usize) {
        let r = r.abs();
        if r < 2.0 {
            (0, Self::index_for(r))
        } else {
            (1, Self::index_for(r * 0.5))
        }
    }

    pub fn taps(&self, j: usize) -> usize {
        self.taps[j]
    }

    /// Pass and stop edges of kernel j (cycles per level sample).
    pub fn edges(&self, j: usize) -> (f64, f64) {
        (self.pass[j], self.stop[j])
    }

    /// Row k (0 – 128) of kernel j.
    pub fn row(&self, j: usize, k: usize) -> &[f32] {
        let n = self.taps[j];
        let b = self.offset[j] + k * n;
        &self.table[b..b + n]
    }

    /// The interpolated taps for a fractional offset `fr` ∈ [0, 1) into `out` (first `taps(j)` used).
    #[inline]
    pub fn taps_at(&self, j: usize, fr: f64, out: &mut [f32; MAX_TAPS]) -> usize {
        let n = self.taps[j];
        let x = fr * ROWS as f64;
        let k = (x as usize).min(ROWS - 1);
        let a = (x - k as f64) as f32;
        let b = self.offset[j] + k * n;
        let (r0, r1) = self.table[b..b + 2 * n].split_at(n);
        for ((o, &p), &q) in out[..n].iter_mut().zip(r0).zip(r1) {
            *o = p + a * (q - p);
        }
        n
    }

    /// Bytes of tables.
    pub fn bytes(&self) -> usize {
        self.table.len() * 4
    }
}

// ---------------------------------------------------------------------------------------------
// Multi-rate buffer

/// A 1- or 2-channel ring of `n0` frames (level 0, a power of two: addressing is a mask) and its
/// half-rate copy of `n0 / 2` (level 1), interleaved, each with a mirrored guard of [`GUARD`] frames. Frames are addressed by their absolute
/// index since reset (`written` counts them); level-1 sample j stands for level-0 time 2j − 24.
#[derive(Clone, Debug, Default)]
pub struct MipBuffer {
    ch: usize,
    n0: usize,
    n1: usize,
    l0: Vec<f32>,
    l1: Vec<f32>,
    written: u64,
    hb: HalfBand,
}

impl MipBuffer {
    /// `frames` is rounded up to a power of two.
    pub fn new(channels: usize, frames: usize) -> Self {
        let ch = channels.clamp(1, 2);
        let n0 = frames.max(4 * GUARD).next_power_of_two();
        let n1 = n0 / 2;
        Self { ch, n0, n1, l0: vec![0.0; (n0 + GUARD) * ch], l1: vec![0.0; (n1 + GUARD) * ch], written: 0, hb: HalfBand::new() }
    }

    pub fn channels(&self) -> usize {
        self.ch
    }

    /// Level-0 capacity (frames).
    pub fn frames(&self) -> usize {
        self.n0
    }

    /// Frames written since reset.
    pub fn written(&self) -> u64 {
        self.written
    }

    /// Back to empty: only the counter (the content is masked by it, not zeroed).
    pub fn reset(&mut self) {
        self.written = 0;
    }

    /// Bytes allocated.
    pub fn bytes(&self) -> usize {
        (self.l0.len() + self.l1.len()) * 4
    }

    /// The oldest level-0 frame still held (absolute).
    pub fn oldest(&self) -> u64 {
        self.written.saturating_sub(self.n0 as u64)
    }

    #[inline]
    fn put(buf: &mut [f32], n: usize, ch: usize, idx: usize, v: &[f32]) {
        let b = idx * ch;
        buf[b..b + ch].copy_from_slice(&v[..ch]);
        if idx < GUARD {
            let g = (n + idx) * ch;
            buf[g..g + ch].copy_from_slice(&v[..ch]);
        }
    }

    /// Level-0 sample of channel c at absolute frame f (must be held).
    #[inline]
    pub fn at(&self, f: u64, c: usize) -> f32 {
        self.l0[(f as usize & (self.n0 - 1)) * self.ch + c]
    }

    /// Level-1 sample of channel c at absolute index j.
    #[inline]
    pub fn at1(&self, j: u64, c: usize) -> f32 {
        self.l1[(j as usize & (self.n1 - 1)) * self.ch + c]
    }

    /// Appends one frame (`ch` values); every second frame also makes a level-1 sample.
    #[inline]
    pub fn push(&mut self, frame: &[f32]) {
        let f = self.written;
        let idx = f as usize & (self.n0 - 1);
        Self::put(&mut self.l0, self.n0, self.ch, idx, frame);
        self.written = f + 1;
        if f % 2 == 1 {
            let j = (f - 1) / 2;
            let mut y = [0.0f32; 2];
            for (c, yc) in y.iter_mut().enumerate().take(self.ch) {
                *yc = if f >= (HB_TAPS - 1) as u64 {
                    // the 51 frames are contiguous thanks to the mirrored guard
                    let b = (f - (HB_TAPS - 1) as u64) as usize & (self.n0 - 1);
                    let ch = self.ch;
                    let s = &self.l0[b * ch + c..(b + HB_TAPS - 1) * ch + c + 1];
                    self.hb.apply(|m| s[m * ch])
                } else {
                    // after a reset: the frames before frame 0 are silence
                    let base = f as i64 - (HB_TAPS - 1) as i64;
                    self.hb.apply(|m| {
                        let a = base + m as i64;
                        if a < 0 {
                            0.0
                        } else {
                            self.at(a as u64, c)
                        }
                    })
                };
            }
            let jdx = j as usize & (self.n1 - 1);
            Self::put(&mut self.l1, self.n1, self.ch, jdx, &y);
        }
    }

    /// Copies `n` level-0 frames from `src` (absolute `src0`) to this buffer's frames `dst0 ..`
    /// (used as a linear buffer: HOLD); the counter becomes at least `dst0 + n`.
    pub fn copy_l0_from(&mut self, src: &MipBuffer, src0: u64, dst0: usize, n: usize) {
        let ch = self.ch.min(src.ch);
        for k in 0..n {
            let mut v = [0.0f32; 2];
            for (c, vc) in v.iter_mut().enumerate().take(ch) {
                *vc = src.at(src0 + k as u64, c);
            }
            if self.ch == 2 && src.ch == 1 {
                v[1] = v[0];
            }
            let idx = (dst0 + k) & (self.n0 - 1);
            Self::put(&mut self.l0, self.n0, self.ch, idx, &v);
        }
        self.written = self.written.max((dst0 + n) as u64);
    }

    /// Copies `n` level-1 samples from `src` (absolute index `src0`) to indices `dst0 ..`.
    pub fn copy_l1_from(&mut self, src: &MipBuffer, src0: u64, dst0: usize, n: usize) {
        let ch = self.ch.min(src.ch);
        for k in 0..n {
            let mut v = [0.0f32; 2];
            for (c, vc) in v.iter_mut().enumerate().take(ch) {
                *vc = src.at1(src0 + k as u64, c);
            }
            if self.ch == 2 && src.ch == 1 {
                v[1] = v[0];
            }
            let idx = (dst0 + k) & (self.n1 - 1);
            Self::put(&mut self.l1, self.n1, self.ch, idx, &v);
        }
    }

    /// The frame at an integer position (the unity path: a copy, bit-exact).
    #[inline]
    pub fn read_int(&self, f: u64) -> [f32; 2] {
        let b = (f as usize & (self.n0 - 1)) * self.ch;
        if self.ch == 2 {
            [self.l0[b], self.l0[b + 1]]
        } else {
            [self.l0[b], self.l0[b]]
        }
    }

    /// A band-limited read: `pos` in level-0 frames (absolute), on `level` (0 or 1) with kernel `j`.
    /// The caller keeps `pos` inside the held span by the kernel's half-length (+ the half-band's
    /// delay on level 1).
    #[inline]
    pub fn read(&self, bank: &KernelBank, level: usize, j: usize, pos: f64) -> [f32; 2] {
        let (q, buf, n) = if level == 0 { (pos, &self.l0, self.n0) } else { ((pos + L1_OFFSET) * 0.5, &self.l1, self.n1) };
        // q ≥ 0 (callers keep reads inside the buffer): truncation is the floor, without a libm call
        let taps = bank.taps[j];
        let (start, k, a) = setup(q, taps, n);
        let rb = bank.offset[j] + k;
        let rows = &bank.table[rb..rb + 2 * taps];
        if self.ch == 2 {
            let s = &buf[start * 2..start * 2 + 2 * taps];
            #[cfg(target_arch = "x86_64")]
            if bank.avx {
                // SAFETY: `avx` is set only when the CPU has AVX2 and FMA (KernelBank::new)
                return unsafe { dot2_avx(rows, taps, a, s) };
            }
            dot2(rows, taps, a, s)
        } else {
            let v = dot1(rows, taps, a, &buf[start..start + taps]);
            [v, v]
        }
    }

    /// A run of `out.len()` consecutive reads of one grain: from `pos`, advancing by `step` (level-0
    /// frames per sample), `step` multiplied by `mul` after each sample (exponential glide). `pos`
    /// and `step` are left where the run ends. The same values as [`MipBuffer::read`] sample by
    /// sample, with the kernel's setup hoisted out of the loop.
    #[allow(clippy::too_many_arguments)]
    pub fn read_run(&self, bank: &KernelBank, level: usize, j: usize, pos: &mut f64, step: &mut f64, mul: f64, out: &mut [[f32; 2]]) {
        let (buf, n, half, off): (&[f32], usize, f64, f64) = if level == 0 { (&self.l0, self.n0, 1.0, 0.0) } else { (&self.l1, self.n1, 0.5, L1_OFFSET) };
        let taps = bank.taps[j];
        let base = bank.offset[j];
        let table = &bank.table[base..base + (ROWS + 1) * taps];
        #[cfg(target_arch = "x86_64")]
        if self.ch == 2 && bank.avx {
            // SAFETY: `avx` is set only when the CPU has AVX2 and FMA (KernelBank::new)
            unsafe { run2_avx(buf, n, table, taps, half, off, pos, step, mul, out) };
            return;
        }
        let (mut p, mut st) = (*pos, *step);
        for o in out.iter_mut() {
            let (start, rb, a) = setup((p + off) * half, taps, n);
            let rows = &table[rb..rb + 2 * taps];
            *o = if self.ch == 2 {
                dot2(rows, taps, a, &buf[start * 2..start * 2 + 2 * taps])
            } else {
                let v = dot1(rows, taps, a, &buf[start..start + taps]);
                [v, v]
            };
            p += st;
            if mul != 1.0 {
                st *= mul;
            }
        }
        *pos = p;
        *step = st;
    }
}

/// The first tap's frame, the row's offset in the kernel's table and the interpolation weight for a
/// level position q ≥ 0.
#[inline(always)]
fn setup(q: f64, taps: usize, n: usize) -> (usize, usize, f32) {
    debug_assert!((0.0..9.0e15).contains(&q));
    // SAFETY: q is a finite, non-negative position below 2^53 (the callers keep reads inside the
    // buffer), and x ∈ [0, 128): both conversions are in range (no saturating cast needed)
    let i: i64 = unsafe { q.to_int_unchecked() };
    let x = (q - i as f64) * ROWS as f64;
    let k = (unsafe { x.to_int_unchecked::<usize>() }).min(ROWS - 1);
    let a = (x - k as f64) as f32;
    ((i - taps as i64 / 2 + 1) as usize & (n - 1), k * taps, a)
}

/// `read_run`'s loop with AVX2 / FMA, stereo.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
#[allow(clippy::too_many_arguments)]
unsafe fn run2_avx(buf: &[f32], n: usize, table: &[f32], taps: usize, half: f64, off: f64, pos: &mut f64, step: &mut f64, mul: f64, out: &mut [[f32; 2]]) {
    let (mut p, mut st) = (*pos, *step);
    for o in out.iter_mut() {
        let (start, rb, a) = setup((p + off) * half, taps, n);
        *o = dot2_avx(&table[rb..rb + 2 * taps], taps, a, &buf[start * 2..start * 2 + 2 * taps]);
        p += st;
        if mul != 1.0 {
            st *= mul;
        }
    }
    *pos = p;
    *step = st;
}

/// `dot2` with AVX2 / FMA (8 lanes; the CPU is checked once, in `KernelBank::new`).
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
#[inline]
unsafe fn dot2_avx(rows: &[f32], n: usize, a: f32, s: &[f32]) -> [f32; 2] {
    use std::arch::x86_64::*;
    let (r0, r1) = rows.split_at(n);
    let s = &s[..2 * n];
    let va = _mm256_set1_ps(a);
    let (mut acc0, mut acc1) = (_mm256_setzero_ps(), _mm256_setzero_ps());
    let mut m = 0;
    // n is 32 or 48: a multiple of 8; `s` holds 2n values
    while m < n {
        let p = _mm256_loadu_ps(r0.as_ptr().add(m));
        let q = _mm256_loadu_ps(r1.as_ptr().add(m));
        let h = _mm256_fmadd_ps(va, _mm256_sub_ps(q, p), p);
        let lo = _mm256_unpacklo_ps(h, h);
        let hi = _mm256_unpackhi_ps(h, h);
        let h0 = _mm256_permute2f128_ps(lo, hi, 0x20);
        let h1 = _mm256_permute2f128_ps(lo, hi, 0x31);
        acc0 = _mm256_fmadd_ps(h0, _mm256_loadu_ps(s.as_ptr().add(2 * m)), acc0);
        acc1 = _mm256_fmadd_ps(h1, _mm256_loadu_ps(s.as_ptr().add(2 * m + 8)), acc1);
        m += 8;
    }
    let acc = _mm256_add_ps(acc0, acc1);
    let v = _mm_add_ps(_mm256_castps256_ps128(acc), _mm256_extractf128_ps(acc, 1));
    let mut o = [0.0f32; 4];
    _mm_storeu_ps(o.as_mut_ptr(), v);
    [o[0] + o[2], o[1] + o[3]]
}

/// The interpolated row (r0 + a (r1 − r0), `rows` = r0 then r1) against interleaved stereo `s`.
#[inline(always)]
fn dot2(rows: &[f32], n: usize, a: f32, s: &[f32]) -> [f32; 2] {
    let (r0, r1) = rows.split_at(n);
    let s = &s[..2 * n];
    #[cfg(target_arch = "x86_64")]
    {
        use std::arch::x86_64::*;
        // SAFETY: SSE2 is part of the x86_64 baseline; every load stays inside the slices above
        // (n is a multiple of 4, `s` holds 2n values).
        unsafe {
            let va = _mm_set1_ps(a);
            let (mut acc0, mut acc1) = (_mm_setzero_ps(), _mm_setzero_ps());
            let mut m = 0;
            while m < n {
                let p = _mm_loadu_ps(r0.as_ptr().add(m));
                let q = _mm_loadu_ps(r1.as_ptr().add(m));
                let h = _mm_add_ps(p, _mm_mul_ps(va, _mm_sub_ps(q, p)));
                let lo = _mm_unpacklo_ps(h, h);
                let hi = _mm_unpackhi_ps(h, h);
                acc0 = _mm_add_ps(acc0, _mm_mul_ps(lo, _mm_loadu_ps(s.as_ptr().add(2 * m))));
                acc1 = _mm_add_ps(acc1, _mm_mul_ps(hi, _mm_loadu_ps(s.as_ptr().add(2 * m + 4))));
                m += 4;
            }
            let mut o = [0.0f32; 4];
            _mm_storeu_ps(o.as_mut_ptr(), _mm_add_ps(acc0, acc1));
            [o[0] + o[2], o[1] + o[3]]
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let (mut l, mut r) = (0.0f32, 0.0f32);
        for m in 0..n {
            let h = r0[m] + a * (r1[m] - r0[m]);
            l += h * s[2 * m];
            r += h * s[2 * m + 1];
        }
        [l, r]
    }
}

/// The interpolated row against mono `s`.
#[inline(always)]
fn dot1(rows: &[f32], n: usize, a: f32, s: &[f32]) -> f32 {
    let (r0, r1) = rows.split_at(n);
    let s = &s[..n];
    #[cfg(target_arch = "x86_64")]
    {
        use std::arch::x86_64::*;
        // SAFETY: as in `dot2` (`s` holds n values).
        unsafe {
            let va = _mm_set1_ps(a);
            let mut acc = _mm_setzero_ps();
            let mut m = 0;
            while m < n {
                let p = _mm_loadu_ps(r0.as_ptr().add(m));
                let q = _mm_loadu_ps(r1.as_ptr().add(m));
                let h = _mm_add_ps(p, _mm_mul_ps(va, _mm_sub_ps(q, p)));
                acc = _mm_add_ps(acc, _mm_mul_ps(h, _mm_loadu_ps(s.as_ptr().add(m))));
                m += 4;
            }
            let mut o = [0.0f32; 4];
            _mm_storeu_ps(o.as_mut_ptr(), acc);
            (o[0] + o[2]) + (o[1] + o[3])
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let mut v = 0.0f32;
        for m in 0..n {
            v += (r0[m] + a * (r1[m] - r0[m])) * s[m];
        }
        v
    }
}

// ---------------------------------------------------------------------------------------------
// Window

/// Number of points of the Hann (cosine) table.
const COS_N: usize = 4096;
/// Points of the numerical moments and transforms.
const MOM_N: usize = 256;

/// One grain's window, fixed at its start (README §2.4). φ ∈ [0, 1) is the time along the grain.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WindowShape {
    /// weights of flat, triangle, Hann (sum 1)
    pub wf: f32,
    pub wt: f32,
    pub wh: f32,
    /// the peak's position after the time bound
    pub peak: f64,
    /// the edge guard's length (fraction of the grain)
    pub guard: f64,
    /// 0.5 / peak, 0.5 / (1 − peak), 0.5 / guard
    k_up: f64,
    k_down: f64,
    k_guard: f64,
}

impl WindowShape {
    /// `shape` 0 – 1 (flat → triangle → Hann), `skew` −1 … 1, `guard_frac` = min(2 ms, T / 4) / T.
    pub fn new(shape: f32, skew: f32, guard_frac: f64) -> Self {
        let s = shape.clamp(0.0, 1.0);
        let (wf, wt, wh) = if s < 0.5 { (1.0 - 2.0 * s, 2.0 * s, 0.0) } else { (0.0, 2.0 - 2.0 * s, 2.0 * s - 1.0) };
        let g = guard_frac.clamp(1e-6, 0.25);
        let peak = (0.5 + 0.48 * skew.clamp(-1.0, 1.0) as f64).clamp(g, 1.0 - g);
        Self { wf, wt, wh, peak, guard: g, k_up: 0.5 / peak, k_down: 0.5 / (1.0 - peak), k_guard: 0.5 / g }
    }

    /// The plain Hann (no skew): its moments are exact (0.5, 0.375).
    pub fn is_plain_hann(&self) -> bool {
        self.wh == 1.0 && self.peak == 0.5
    }
}

/// The window evaluator: a cosine table and the shape's formula.
#[derive(Clone, Debug)]
pub struct Window {
    cos: Vec<f32>,
}

impl Default for Window {
    fn default() -> Self {
        Self::new()
    }
}

impl Window {
    pub fn new() -> Self {
        Self { cos: (0..=COS_N).map(|k| (2.0 * PI * k as f64 / COS_N as f64).cos() as f32).collect() }
    }

    /// cos(2π x), x ∈ [0, 1].
    #[inline]
    fn cos2pi(&self, x: f64) -> f32 {
        let y = x * COS_N as f64;
        let k = (y as usize).min(COS_N - 1);
        let a = (y - k as f64) as f32;
        self.cos[k] + a * (self.cos[k + 1] - self.cos[k])
    }

    /// The window at φ ∈ [0, 1) (0 outside).
    #[inline]
    pub fn value(&self, s: &WindowShape, phi: f64) -> f32 {
        if !(0.0..1.0).contains(&phi) {
            return 0.0;
        }
        let p = s.peak;
        let u = if phi < p { phi * s.k_up } else { 0.5 + (phi - p) * s.k_down };
        let mut w = 0.0f32;
        if s.wh > 0.0 {
            w += s.wh * (0.5 - 0.5 * self.cos2pi(u));
        }
        if s.wf > 0.0 || s.wt > 0.0 {
            let tri = (1.0 - (2.0 * u - 1.0).abs()) as f32;
            let base = s.wf + s.wt * tri;
            let g = s.guard;
            let guard = if phi < g {
                0.5 - 0.5 * self.cos2pi(phi * s.k_guard)
            } else if phi > 1.0 - g {
                0.5 - 0.5 * self.cos2pi((1.0 - phi) * s.k_guard)
            } else {
                1.0
            };
            w += base * guard;
        }
        w
    }

    /// The flat and triangular parts with their edge guard (the window without its Hann part; the
    /// Hann part comes from a [`WindowOsc`]).
    #[inline]
    pub fn value_ft(&self, s: &WindowShape, phi: f64) -> f32 {
        if s.wf == 0.0 && s.wt == 0.0 {
            return 0.0;
        }
        let p = s.peak;
        let u = if phi < p { phi * s.k_up } else { 0.5 + (phi - p) * s.k_down };
        let tri = (1.0 - (2.0 * u - 1.0).abs()) as f32;
        let base = s.wf + s.wt * tri;
        let g = s.guard;
        let guard = if phi < g {
            0.5 - 0.5 * self.cos2pi(phi * s.k_guard)
        } else if phi > 1.0 - g {
            0.5 - 0.5 * self.cos2pi((1.0 - phi) * s.k_guard)
        } else {
            1.0
        };
        base * guard
    }

    /// E[w] and E[w²] over the grain (exact for the plain Hann, else a 256-point midpoint sum).
    pub fn moments(&self, s: &WindowShape) -> (f64, f64) {
        if s.is_plain_hann() {
            return (0.5, 0.375);
        }
        let (mut m1, mut m2) = (0.0f64, 0.0f64);
        for k in 0..MOM_N {
            let w = self.value(s, (k as f64 + 0.5) / MOM_N as f64) as f64;
            m1 += w;
            m2 += w * w;
        }
        (m1 / MOM_N as f64, m2 / MOM_N as f64)
    }

    /// |W(f)|², W(f) = ∫₀¹ w(φ) e^(−2πifφ) dφ (f in cycles per grain).
    pub fn power_at(&self, s: &WindowShape, f: f64) -> f64 {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        let d = 2.0 * PI * f / MOM_N as f64;
        let (sd, cd) = d.sin_cos();
        let (s0, c0) = (0.5 * d).sin_cos();
        let (mut sn, mut cn) = (s0, c0);
        for k in 0..MOM_N {
            let w = self.value(s, (k as f64 + 0.5) / MOM_N as f64) as f64;
            re += w * cn;
            im -= w * sn;
            let c = cn * cd - sn * sd;
            sn = sn * cd + cn * sd;
            cn = c;
        }
        let (re, im) = (re / MOM_N as f64, im / MOM_N as f64);
        re * re + im * im
    }

    /// Variance over time of a periodic sum of windows, `n` per grain length (Poisson summation:
    /// 2 Σ_{k ≥ 1} n² |W(k n)|², k n up to 64).
    pub fn periodic_var(&self, s: &WindowShape, n: f64) -> f64 {
        if n <= 0.0 {
            return 0.0;
        }
        let mut v = 0.0;
        let mut k = 1.0;
        while k * n <= 64.0 && k <= 32.0 {
            v += 2.0 * n * n * self.power_at(s, k * n);
            k += 1.0;
        }
        v
    }
}

/// The Hann part of a grain's window, evaluated incrementally at the grain's exact phase: a rotation
/// in f64 along each side of the (skewed) peak, restarted exactly at the peak. Costs a few multiplies
/// per sample instead of a table lookup; equal to `Window::value`'s Hann part within 1e-7.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowOsc {
    c: f64,
    s: f64,
    cd: f64,
    sd: f64,
    up: bool,
    dphi: f64,
}

impl WindowOsc {
    /// At phase `phi0` (≥ 0), advancing `dphi` per sample.
    pub fn new(w: &WindowShape, phi0: f64, dphi: f64) -> Self {
        let up = phi0 < w.peak;
        let (u, k) = if up { (phi0 * w.k_up, w.k_up) } else { (0.5 + (phi0 - w.peak) * w.k_down, w.k_down) };
        let (s, c) = (2.0 * PI * u).sin_cos();
        let (sd, cd) = (2.0 * PI * dphi * k).sin_cos();
        Self { c, s, cd, sd, up, dphi }
    }

    /// The Hann part (0 – 1, before its weight) at phase `phi`, then one step on.
    #[inline]
    pub fn next(&mut self, w: &WindowShape, phi: f64) -> f32 {
        if self.up && phi >= w.peak {
            self.up = false;
            let (s, c) = (2.0 * PI * (0.5 + (phi - w.peak) * w.k_down)).sin_cos();
            let (sd, cd) = (2.0 * PI * self.dphi * w.k_down).sin_cos();
            *self = Self { c, s, cd, sd, up: false, dphi: self.dphi };
        }
        let v = 0.5 - 0.5 * self.c;
        let c = self.c * self.cd - self.s * self.sd;
        self.s = self.s * self.cd + self.c * self.sd;
        self.c = c;
        v as f32
    }
}

// ---------------------------------------------------------------------------------------------
// Pool

/// A fixed pool of voices: no allocation after `new`, a full pool drops the new voice (never steals).
#[derive(Clone, Debug)]
pub struct GrainPool<G: Copy + Default> {
    voices: Vec<G>,
    len: usize,
    dropped: u64,
}

impl<G: Copy + Default> GrainPool<G> {
    pub fn new(capacity: usize) -> Self {
        Self { voices: vec![G::default(); capacity], len: 0, dropped: 0 }
    }

    pub fn capacity(&self) -> usize {
        self.voices.len()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_full(&self) -> bool {
        self.len == self.voices.len()
    }

    /// Voices dropped because the pool was full, since the last clear.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    pub fn clear(&mut self) {
        self.len = 0;
        self.dropped = 0;
    }

    /// Adds a voice; false (and counted) when the pool is full.
    pub fn push(&mut self, g: G) -> bool {
        if self.len == self.voices.len() {
            self.dropped += 1;
            return false;
        }
        self.voices[self.len] = g;
        self.len += 1;
        true
    }

    pub fn as_slice(&self) -> &[G] {
        &self.voices[..self.len]
    }

    pub fn as_mut_slice(&mut self) -> &mut [G] {
        &mut self.voices[..self.len]
    }

    /// Keeps the voices for which `keep` is true, in order (deterministic).
    pub fn retain(&mut self, mut keep: impl FnMut(&G) -> bool) {
        let mut w = 0;
        for r in 0..self.len {
            if keep(&self.voices[r]) {
                self.voices[w] = self.voices[r];
                w += 1;
            }
        }
        self.len = w;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use realfft::RealFftPlanner;

    fn db(x: f64) -> f64 {
        20.0 * x.max(1e-30).log10()
    }

    /// |H(f)| of a tap row (f in cycles per sample).
    fn resp(h: &[f32], f: f64) -> f64 {
        let (mut re, mut im) = (0.0, 0.0);
        for (m, &c) in h.iter().enumerate() {
            let a = 2.0 * PI * f * m as f64;
            re += c as f64 * a.cos();
            im -= c as f64 * a.sin();
        }
        (re * re + im * im).sqrt()
    }

    #[test]
    fn half_band_pass_and_stop() {
        let hb = HalfBand::new();
        let h: Vec<f32> = hb.taps().iter().map(|&v| v as f32).collect();
        let mut ripple = 0.0f64;
        let mut stop = -300.0f64;
        for k in 0..=1000 {
            let f = 0.5 * k as f64 / 1000.0;
            let a = db(resp(&h, f));
            if f <= 0.2 {
                ripple = ripple.max(a.abs());
            }
            if f >= 0.3 {
                stop = stop.max(a);
            }
        }
        // pass to 0.2 fs, stop from 0.3 fs (fs = the input rate)
        assert!(ripple < 0.002, "ripple {ripple}");
        eprintln!("half-band: ripple {ripple:.4} dB, stop {stop:.1} dB");
        assert!(stop < -78.0, "stop band {stop:.1} dB");
        assert!((hb.taps().iter().sum::<f64>() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn kernels_dc_pass_and_stop() {
        let bank = KernelBank::new();
        assert_eq!(bank.bytes(), (9 * 129 * 32 + 8 * 129 * 48) * 4);
        for j in 0..NK {
            for k in 0..=ROWS {
                let dc: f64 = bank.row(j, k).iter().map(|&v| v as f64).sum();
                assert!((dc - 1.0).abs() < 2e-6, "kernel {j} row {k} dc {dc}");
            }
        }
        // the continuous kernel the rows sample (128 points per level sample): pass ±0.01 dB to its
        // pass edge, stop band from its stop edge to 64 × the level's rate (prototype: 71.7 – 99 dB)
        let nfft = 1 << 17;
        let mut planner = RealFftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(nfft);
        for j in 0..NK {
            let n = bank.taps(j);
            let (p, s) = bank.edges(j);
            let mut x = fft.make_input_vec();
            for m in 0..n {
                for k in 0..ROWS {
                    // tap m of row k sits at t = (m − n / 2 + 1) − k / 128
                    let u = ((m as i64 - n as i64 / 2 + 1) * ROWS as i64 - k as i64 + (n as i64 / 2) * ROWS as i64) as usize;
                    x[u] = bank.row(j, k)[m] as f64 / ROWS as f64;
                }
            }
            let mut y = fft.make_output_vec();
            fft.process(&mut x, &mut y).unwrap();
            let (mut pass, mut stop) = (0.0f64, -300.0f64);
            for (b, c) in y.iter().enumerate() {
                let f = b as f64 * ROWS as f64 / nfft as f64;
                let a = db(c.norm());
                if f <= p {
                    pass = pass.max(a.abs());
                } else if f >= s && f < 64.0 {
                    stop = stop.max(a);
                }
            }
            eprintln!("kernel {j:2} ({n} taps): pass ±{pass:.4} dB, stop {stop:.1} dB");
            assert!(pass < 0.01, "kernel {j}: pass ripple {pass:.4} dB");
            assert!(stop < -71.0, "kernel {j}: stop band {stop:.1} dB");
        }
    }

    /// Aliasing (README §5): a sine read at ratio r through the octave switch; every component away
    /// from r·f below 0.4 fs is ≥ 70 dB down.
    #[test]
    fn octave_switch_alias_bound() {
        let bank = KernelBank::new();
        let n_out = 1 << 14;
        let mut planner = RealFftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(n_out);
        let win: Vec<f64> = (0..n_out).map(|i| bessel_i0(26.0 * (1.0 - (2.0 * i as f64 / (n_out - 1) as f64 - 1.0).powi(2)).max(0.0).sqrt()) / bessel_i0(26.0)).collect();
        let wsum: f64 = win.iter().sum();
        let mut worst = -300.0f64;
        for st in [-24.0f64, -12.0, 1.0, 5.9, 6.1, 7.0, 11.9, 12.0, 13.0, 17.9, 18.1, 19.0, 24.0] {
            let r = (st / 12.0).exp2();
            let (level, j) = KernelBank::choose(r);
            for &fsrc in &[1000.0f64, 4100.0, 7900.0, 11300.0, 15700.0, 18000.0] {
                let fs = 48000.0;
                let need = (n_out as f64 * r) as usize + 600;
                let mut b = MipBuffer::new(1, need + 1000);
                for i in 0..need {
                    b.push(&[(2.0 * PI * fsrc / fs * i as f64).sin() as f32]);
                }
                let mut spec = fft.make_input_vec();
                for (i, s) in spec.iter_mut().enumerate() {
                    *s = b.read(&bank, level, j, 200.0 + r * i as f64)[0] as f64 * win[i];
                }
                let mut out = fft.make_output_vec();
                fft.process(&mut spec, &mut out).unwrap();
                let fo = fsrc * r;
                for (k, c) in out.iter().enumerate() {
                    let f = k as f64 * fs / n_out as f64;
                    if f < 20.0 || f > 0.4 * fs || (f - fo).abs() < 80.0 {
                        continue;
                    }
                    worst = worst.max(db(c.norm() / (wsum / 2.0)));
                }
            }
        }
        eprintln!("worst spur below 0.4 fs: {worst:.1} dB");
        assert!(worst < -70.0, "worst spur {worst:.1} dB");
    }

    #[test]
    fn level_one_is_compensated() {
        // a sine read on level 1 at r = 2 and on level 0 at r = 1.999…: same phase
        let bank = KernelBank::new();
        let mut b = MipBuffer::new(2, 1 << 14);
        let f = 0.031;
        for i in 0..12000 {
            let v = (2.0 * PI * f * i as f64).sin() as f32;
            b.push(&[v, -v]);
        }
        let mut err = 0.0f64;
        for i in 0..500 {
            let pos = 3000.0 + 7.37 * i as f64;
            let a = b.read(&bank, 1, KernelBank::index_for(1.0), pos);
            let want = (2.0 * PI * f * pos).sin();
            err = err.max((a[0] as f64 - want).abs()).max((a[1] as f64 + want).abs());
        }
        assert!(db(err) < -70.0, "level-1 error {:.1} dB", db(err));
    }

    #[test]
    fn a_run_reads_as_single_reads() {
        let bank = KernelBank::new();
        for ch in [1usize, 2] {
            let mut b = MipBuffer::new(ch, 1 << 13);
            for i in 0..20000 {
                let v = (i as f32 * 0.0123).sin();
                b.push(&[v, 0.5 * v]);
            }
            for (lv, j) in [(0usize, 0usize), (0, 12), (1, 3)] {
                let (mut pos, mut step) = (9000.25f64, 1.37f64);
                let mut out = [[0.0f32; 2]; 32];
                b.read_run(&bank, lv, j, &mut pos, &mut step, 1.001, &mut out);
                let (mut p, mut st) = (9000.25f64, 1.37f64);
                for o in out {
                    let r = b.read(&bank, lv, j, p);
                    assert!((r[0] - o[0]).abs() < 1e-6 && (r[1] - o[1]).abs() < 1e-6);
                    p += st;
                    st *= 1.001;
                }
                assert_eq!((p, st), (pos, step));
            }
        }
    }

    #[test]
    fn mirrored_guard_and_wrap() {
        let mut b = MipBuffer::new(2, 600);
        let n = b.frames() as u64;
        for i in 0..(3 * n + 17) {
            b.push(&[i as f32, -(i as f32)]);
        }
        let bank = KernelBank::new();
        // integer reads across the wrap
        let w = b.written();
        for f in (w - n + 40)..(w - 30) {
            assert_eq!(b.read_int(f), [f as f32, -(f as f32)]);
            // kernel 0 at a whole position on a ramp gives the ramp (symmetric taps, DC 1)
            let r = b.read(&bank, 0, 0, f as f64);
            assert!((r[0] - f as f32).abs() < 0.05 * (1.0 + f as f32 * 1e-3), "{f}: {}", r[0]);
        }
    }

    #[test]
    fn hann_stream_sums_to_constant() {
        let w = Window::new();
        let s = WindowShape::new(1.0, 0.0, 0.25);
        for n in [2usize, 3, 8, 64] {
            let mut worst = 0.0f64;
            for k in 0..997 {
                let u = k as f64 / 997.0 / n as f64;
                let sum: f64 = (0..n).map(|i| w.value(&s, u + i as f64 / n as f64) as f64).sum();
                worst = worst.max((sum - n as f64 * 0.5).abs() / (n as f64 * 0.5));
            }
            assert!(db(worst) < -125.0, "n {n}: {:.1} dB", db(worst));
            assert!(w.periodic_var(&s, n as f64) < 1e-12);
        }
        // one grain at a time: the sum is the window itself
        let v1 = w.periodic_var(&s, 1.0);
        assert!((v1 - (0.375 - 0.25)).abs() < 1e-6, "{v1}");
    }

    #[test]
    fn window_edges_are_c1_and_skew_is_bounded() {
        let w = Window::new();
        for shape in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
            for skew in [-1.0f32, 0.0, 1.0] {
                let s = WindowShape::new(shape, skew, 0.05);
                let e = 1e-5;
                assert!(w.value(&s, 0.0).abs() < 1e-6);
                assert!(w.value(&s, 1.0 - 1e-9).abs() < 1e-5);
                // slope near the edges ~ 0
                assert!(w.value(&s, e) / (e as f32) < 0.05, "shape {shape} skew {skew}");
                assert!(w.value(&s, 1.0 - e) / (e as f32) < 0.05, "shape {shape} skew {skew}");
                assert!(s.peak >= 0.05 && s.peak <= 0.95);
            }
        }
        let (m1, m2) = w.moments(&WindowShape::new(0.5, 0.0, 0.01));
        assert!((m1 - 0.5).abs() < 0.01 && (m2 - 1.0 / 3.0).abs() < 0.01, "{m1} {m2}");
    }

    #[test]
    fn window_osc_matches_the_table() {
        let w = Window::new();
        for (shape, skew) in [(1.0f32, 0.0f32), (1.0, 0.7), (1.0, -1.0), (0.75, 0.3)] {
            let ws = WindowShape::new(shape, skew, 0.02);
            let len = 9973.3;
            let phi0 = 0.37 / len;
            let mut o = WindowOsc::new(&ws, phi0, 1.0 / len);
            let mut worst = 0.0f32;
            for n in 0..9973 {
                let phi = phi0 + n as f64 / len;
                let v = ws.wh * o.next(&ws, phi) + w.value_ft(&ws, phi);
                worst = worst.max((v - w.value(&ws, phi)).abs());
            }
            assert!(worst < 3e-7, "shape {shape} skew {skew}: {worst}");
        }
    }

    #[test]
    fn pool_drops_never_steals() {
        let mut p: GrainPool<u32> = GrainPool::new(4);
        for i in 0..6 {
            p.push(i);
        }
        assert_eq!(p.as_slice(), &[0, 1, 2, 3]);
        assert_eq!(p.dropped(), 2);
        p.retain(|&g| g % 2 == 1);
        assert_eq!(p.as_slice(), &[1, 3]);
    }
}

#[cfg(test)]
mod speed {
    use super::*;

    #[test]
    #[ignore]
    fn read_speed() {
        let bank = KernelBank::new();
        let mut b = MipBuffer::new(2, 1 << 16);
        for i in 0..60000 {
            b.push(&[(i as f32 * 0.01).sin(), 0.0]);
        }
        let w = Window::new();
        let ws = WindowShape::new(1.0, 0.0, 0.02);
        for (lv, j) in [(0usize, 0usize), (0, 9), (1, 4)] {
            let t = std::time::Instant::now();
            let mut acc = 0.0f32;
            let mut pos = 1000.3f64;
            for _ in 0..4_000_000 {
                let v = b.read(&bank, lv, j, pos);
                acc += v[0] + v[1];
                pos += 1.0137;
                if pos > 50000.0 {
                    pos -= 48000.0;
                }
            }
            eprintln!("read level {lv} kernel {j}: {:.1} ns ({acc})", t.elapsed().as_secs_f64() * 1e9 / 4e6);
        }
        let t = std::time::Instant::now();
        let mut acc = 0.0f32;
        for k in 0..4_000_000 {
            acc += w.value(&ws, (k % 1000) as f64 * 0.001);
        }
        eprintln!("window: {:.1} ns ({acc})", t.elapsed().as_secs_f64() * 1e9 / 4e6);
    }
}
