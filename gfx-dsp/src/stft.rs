// SPDX-License-Identifier: GPL-3.0-or-later
//! A real-time STFT whose work is cut into small, resumable units (gfx-stretch §2.4; written from
//! gfx-glitch's engine, which keeps its own copy until its bit-exact migration).
//!
//! - **Sizes**: the half-octave grid 2^k and 3 · 2^(k−1), from 256 to 2^18 points (21 sizes).
//! - **Windows**: Hann, periodic form, analysis = synthesis, hop N / 4. Over the hop the products sum
//!   to [`OLA_AMP`] = 1.5 (coherent frames) and the squares to 1.5, so a random-phase frame comes out
//!   at [`OLA_POW`] = 0.375 · 1.5 of the region's power ([`cloud_gain`] calibrates both, and every
//!   blend between them).
//! - **Frame rotation**: a frame is rotated by N / 2 before its FFT, so phases refer to its centre.
//! - **Real FFT** = a complex FFT of N / 2 (even samples in the real part, odd in the imaginary) and
//!   an O(N) twiddle pass ([`post_twiddle`] forward, [`pre_twiddle`] inverse), both resumable. The
//!   complex FFT is one unit up to N = 16,384 ([`FOUR_STEP_ABOVE`]) and a **four-step** FFT above
//!   (Bailey 1990): N₂ column FFTs of N₁ with their twiddles, then N₁ row FFTs of N₂, grouped into
//!   units of about [`UNIT`] points. Its output stays transposed: [`post_twiddle`] and [`read_time`]
//!   read it in place, so there is no transpose pass.
//! - **Tables**: every twiddle and window value is read with a stride from two master tables of
//!   e^(−2πi m / T), T = 2^18 and 3 · 2^16, built in [`Tables::new`] with every FFT plan. A size change
//!   therefore costs nothing of O(N).
//! - **Schedule** ([`due`]): a frame's U units are spread evenly over the hop on sample counts (unit
//!   i at offset ⌊(i + 1) · H / (U + 1)⌋), never on host blocks.
//! - **Phases**: a 4,096-entry phasor table ([`Phasors`], 12-bit phase) picked by a seeded xorshift
//!   ([`XorShift`]); no sin / cos per bin.
//! - **Peak regions** ([`regions`], gfx-glitch's, and its two resumable sweeps [`RegionSweep`]): each
//!   bin's peak, regions ending at the minimum between two peaks.
//! - **Constant-Q blur** ([`blur_pass`]): a one-pole whose pole varies per bin (τ_k = k · ln 2 · σ /
//!   2,400 bins), in scatter form, so the energy is kept exactly.
//! - **Size crossfade** ([`SizeFade`]): wait for the new engine's frames to fill, then equal power.
//!
//! Allocates in [`Tables::new`] only. Deterministic.

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

pub type C32 = Complex<f32>;

/// Smallest and largest window sizes.
pub const MIN_N: usize = 256;
pub const MAX_N: usize = 1 << 18;
/// Sizes in the grid (2^k and 3 · 2^(k−1), 256 – 2^18).
pub const NUM_SIZES: usize = 21;
/// Above this size the complex FFT is cut into four-step units; up to it, one unit.
pub const FOUR_STEP_ABOVE: usize = 16384;
/// Element operations per unit, about.
pub const UNIT: usize = 1024;
/// Σ w_a · w_s over the hop (Hann / Hann, hop N / 4): the coherent overlap-add's gain.
pub const OLA_AMP: f32 = 1.5;
/// mean(w_a²) · Σ w_s² = 0.375 · 1.5: the power gain of uncorrelated (random-phase) frames.
pub const OLA_POW: f32 = 0.5625;
/// Entries of the phasor table.
pub const PHASORS: usize = 4096;

const T2: usize = 1 << 18;
const T3: usize = 3 << 16;

/// The `i`-th size of the grid (ascending): 256, 384, 512, 768, …, 196,608, 262,144.
pub const fn size(i: usize) -> usize {
    if i.is_multiple_of(2) {
        256 << (i / 2)
    } else {
        384 << (i / 2)
    }
}

/// The grid size nearest to `len` samples in log, clamped to the grid.
pub fn nearest_size(len: f64) -> usize {
    let l = len.max(1.0).ln();
    let mut best = 0;
    for i in 1..NUM_SIZES {
        if ((size(i) as f64).ln() - l).abs() < ((size(best) as f64).ln() - l).abs() {
            best = i;
        }
    }
    best
}

/// The output gain that keeps a frame's power at the region's for `cloud` ∈ [0, 1] (each bin's phase
/// offset uniform in ±π · cloud), with the inverse FFT's 1 / N left to the caller. The coherent part
/// of a bin has amplitude s = sin(π c) / (π c), so power s²: g = 1 / √(0.5625 + s² · (2.25 − 0.5625)).
pub fn cloud_gain(cloud: f32) -> f32 {
    let x = std::f64::consts::PI * cloud.clamp(0.0, 1.0) as f64;
    let s = if x < 1e-9 { 1.0 } else { x.sin() / x };
    let amp2 = (OLA_AMP * OLA_AMP) as f64;
    (1.0 / (OLA_POW as f64 + s * s * (amp2 - OLA_POW as f64)).sqrt()) as f32
}

/// How many of a frame's `units` are due `d` samples after its start (0 ≤ d < hop): unit i runs at
/// ⌊(i + 1) · hop / (units + 1)⌋, so all are done before the next frame.
#[inline]
pub fn due(d: usize, units: usize, hop: usize) -> usize {
    (((d + 1) * (units + 1) - 1) / hop).min(units)
}

/// Number of chunks of at most [`UNIT`] covering `len` items.
#[inline]
pub fn chunks(len: usize) -> usize {
    len.div_ceil(UNIT)
}

/// The item range of chunk `u` out of `len`.
#[inline]
pub fn chunk(len: usize, u: usize) -> (usize, usize) {
    ((u * UNIT).min(len), ((u + 1) * UNIT).min(len))
}

/// One size's FFT plan: the complex FFT of M = N / 2 as N₁ × N₂ (N₂ = 1: one piece).
#[derive(Clone)]
pub struct Plan {
    pub n: usize,
    pub m: usize,
    pub n1: usize,
    pub n2: usize,
    fft1: Arc<dyn Fft<f32>>,
    fft2: Option<Arc<dyn Fft<f32>>>,
    cols: usize,
    rows: usize,
    col_units: usize,
    /// units of one complex FFT
    pub units: usize,
    /// master table and its stride for W_N
    big: bool,
    stride: usize,
}

impl Plan {
    /// W_N^j = e^(−2πi j / N), 0 ≤ j < N.
    #[inline]
    pub fn w(&self, t: &Tables, j: usize) -> C32 {
        let tab = if self.big { &t.tw2 } else { &t.tw3 };
        tab[j * self.stride]
    }
    /// The periodic Hann window, w(i) = 0.5 − 0.5 cos(2π i / N).
    #[inline]
    pub fn hann(&self, t: &Tables, i: usize) -> f32 {
        0.5 - 0.5 * self.w(t, i).re
    }
    /// Where the complex FFT's output k (0 ≤ k < M) sits in the buffer.
    #[inline]
    pub fn idx(&self, k: usize) -> usize {
        if self.n2 == 1 {
            k
        } else {
            (k % self.n1) * self.n2 + k / self.n1
        }
    }
}

/// The master tables and every size's plan. Built once (`prepare`); shared by the engines.
pub struct Tables {
    tw2: Vec<C32>,
    tw3: Vec<C32>,
    pub plans: Vec<Plan>,
    /// in-place scratch every plan fits in
    pub scratch_len: usize,
    /// the four-step's gather buffer
    pub tmp_len: usize,
}

impl Tables {
    /// Builds the master tables (2^18 and 3 · 2^16 points) and the 21 plans. Allocates.
    pub fn new() -> Self {
        // e^(−2πi m / T) as a coarse step (every 256th entry) times a fine one, both exact in f64:
        // 2 × √T sin / cos instead of T (the tables are 450,000 entries; f64 products are exact to
        // the f32 they are rounded to)
        let mk = |t: usize| -> Vec<C32> {
            let e = |m: usize| {
                let a = -std::f64::consts::TAU * m as f64 / t as f64;
                (a.cos(), a.sin())
            };
            let fine: Vec<(f64, f64)> = (0..256).map(e).collect();
            let mut out = Vec::with_capacity(t);
            for k in 0..t.div_ceil(256) {
                let (cr, ci) = e(256 * k);
                for (fr, fi) in fine.iter().take((t - 256 * k).min(256)) {
                    out.push(C32::new((cr * fr - ci * fi) as f32, (cr * fi + ci * fr) as f32));
                }
            }
            out
        };
        let tw2 = mk(T2);
        let tw3 = mk(T3);
        let mut planner = FftPlanner::<f32>::new();
        let mut plans = Vec::with_capacity(NUM_SIZES);
        let mut scratch_len = 0;
        let mut tmp_len = 0;
        for i in 0..NUM_SIZES {
            let n = size(i);
            let m = n / 2;
            let big = i.is_multiple_of(2);
            let stride = if big { T2 / n } else { T3 / n };
            let (n1, n2) = if n <= FOUR_STEP_ABOVE {
                (m, 1)
            } else {
                // M = c · 2^e (c = 1 or 3): N₂ = 2^⌊e / 2⌋, N₁ = M / N₂ (it takes the factor 3)
                let e = (m / if big { 1 } else { 3 }).trailing_zeros();
                let n2 = 1usize << (e / 2);
                (m / n2, n2)
            };
            let fft1 = planner.plan_fft_forward(n1);
            scratch_len = scratch_len.max(fft1.get_inplace_scratch_len());
            let fft2 = if n2 > 1 {
                let f = planner.plan_fft_forward(n2);
                scratch_len = scratch_len.max(f.get_inplace_scratch_len());
                Some(f)
            } else {
                None
            };
            let (cols, rows) = if n2 > 1 { ((UNIT / n1).clamp(1, n2), (UNIT / n2).clamp(1, n1)) } else { (1, 1) };
            let col_units = if n2 > 1 { n2.div_ceil(cols) } else { 1 };
            let units = if n2 > 1 { col_units + n1.div_ceil(rows) } else { 1 };
            tmp_len = tmp_len.max(if n2 > 1 { cols * n1 } else { 0 });
            plans.push(Plan { n, m, n1, n2, fft1, fft2, cols, rows, col_units, units, big, stride });
        }
        Self { tw2, tw3, plans, scratch_len: scratch_len.max(1), tmp_len: tmp_len.max(1) }
    }

    /// Bytes held by the tables (plans' own twiddles not counted).
    pub fn bytes(&self) -> usize {
        (self.tw2.len() + self.tw3.len()) * std::mem::size_of::<C32>()
    }
}

impl Default for Tables {
    fn default() -> Self {
        Self::new()
    }
}

/// Unit `u` of a forward complex FFT of `buf[..plan.m]` (natural order in, [`Plan::idx`] order out).
/// `tmp` holds at least `tables.tmp_len`, `scratch` `tables.scratch_len`.
pub fn cfft_unit(plan: &Plan, t: &Tables, buf: &mut [C32], tmp: &mut [C32], scratch: &mut [C32], u: usize) {
    let m = plan.m;
    if plan.n2 == 1 {
        plan.fft1.process_with_scratch(&mut buf[..m], &mut scratch[..plan.fft1.get_inplace_scratch_len()]);
        return;
    }
    let (n1, n2) = (plan.n1, plan.n2);
    if u < plan.col_units {
        // columns n₂ ∈ [c0, c1): gather (n = N₂ n₁ + n₂), FFT of N₁, twiddle W_M^(n₂ k₁), scatter to k₁ N₂ + n₂
        let c0 = u * plan.cols;
        let c1 = (c0 + plan.cols).min(n2);
        let g = c1 - c0;
        for n1i in 0..n1 {
            let row = &buf[n1i * n2 + c0..n1i * n2 + c1];
            for (c, v) in row.iter().enumerate() {
                tmp[c * n1 + n1i] = *v;
            }
        }
        plan.fft1.process_with_scratch(&mut tmp[..g * n1], &mut scratch[..plan.fft1.get_inplace_scratch_len()]);
        // W_M^j = W_N^(2j)
        for c in 0..g {
            let nn2 = c0 + c;
            let col = &tmp[c * n1..(c + 1) * n1];
            for (k1, v) in col.iter().enumerate() {
                let tw = plan.w(t, 2 * nn2 * k1);
                buf[k1 * n2 + nn2] = *v * tw;
            }
        }
    } else {
        let r0 = (u - plan.col_units) * plan.rows;
        let r1 = (r0 + plan.rows).min(n1);
        let f2 = plan.fft2.as_ref().expect("four-step plan");
        f2.process_with_scratch(&mut buf[r0 * n2..r1 * n2], &mut scratch[..f2.get_inplace_scratch_len()]);
    }
}

/// The real spectrum X[k], k ∈ [k0, k1) ⊂ [0, M], from the complex FFT `buf` of the packed frame
/// (z[j] = x[2j] + i x[2j+1]): X[k] = E[k] + W_N^k O[k], E = (Z[k] + Z̄[M−k]) / 2,
/// O = −i (Z[k] − Z̄[M−k]) / 2. `f(k, X[k])` gets each bin.
pub fn post_twiddle(plan: &Plan, t: &Tables, buf: &[C32], k0: usize, k1: usize, mut f: impl FnMut(usize, C32)) {
    let m = plan.m;
    if k0 >= k1 {
        return;
    }
    // incremental transposed indices: k ascending, (M − k) mod M descending
    let mut up = Walk::new(plan, k0 % m);
    let mut dn = Walk::new(plan, (m - k0 % m) % m);
    for k in k0..k1 {
        let zk = buf[up.idx()];
        let zm = buf[dn.idx()].conj();
        let e = (zk + zm) * 0.5;
        let o = (zk - zm) * C32::new(0.0, -0.5);
        f(k, e + plan.w(t, k) * o);
        up.inc();
        dn.dec();
    }
}

/// A position k in the transposed layout, moved by ±1 without division (wraps at M).
struct Walk {
    k1: usize,
    k2: usize,
    n1: usize,
    n2: usize,
}

impl Walk {
    #[inline]
    fn new(plan: &Plan, k: usize) -> Self {
        Self { k1: k % plan.n1, k2: k / plan.n1, n1: plan.n1, n2: plan.n2 }
    }
    #[inline]
    fn idx(&self) -> usize {
        self.k1 * self.n2 + self.k2
    }
    #[inline]
    fn inc(&mut self) {
        self.k1 += 1;
        if self.k1 == self.n1 {
            self.k1 = 0;
            self.k2 += 1;
            if self.k2 == self.n2 {
                self.k2 = 0;
            }
        }
    }
    #[inline]
    fn dec(&mut self) {
        if self.k1 == 0 {
            self.k1 = self.n1 - 1;
            self.k2 = if self.k2 == 0 { self.n2 - 1 } else { self.k2 - 1 };
        } else {
            self.k1 -= 1;
        }
    }
}

/// The inverse's packed input for bin k (0 ≤ k < M), conjugated for the forward FFT that computes
/// it: conj(E + i O), E = Y[k] + Ȳ[M−k], O = W_N^(−k) (Y[k] − Ȳ[M−k]). The forward complex FFT of
/// these, read back by [`read_time`], gives N · x (the unnormalised inverse).
#[inline]
pub fn pre_twiddle(plan: &Plan, t: &Tables, k: usize, yk: C32, ymk: C32) -> C32 {
    let e = yk + ymk.conj();
    let o = plan.w(t, k).conj() * (yk - ymk.conj());
    (e + C32::new(-o.im, o.re)).conj()
}

/// The inverse's time samples x[r], r ∈ [r0, r1) ⊂ [0, N), from the forward FFT of the
/// [`pre_twiddle`] values: `f(r, x[r])` (N · x, unnormalised).
pub fn read_time(plan: &Plan, buf: &[C32], r0: usize, r1: usize, mut f: impl FnMut(usize, f32)) {
    if r0 >= r1 {
        return;
    }
    let mut w = Walk::new(plan, r0 / 2);
    let mut r = r0;
    if r % 2 == 1 {
        f(r, -buf[w.idx()].im);
        w.inc();
        r += 1;
    }
    while r < r1 {
        let z = buf[w.idx()];
        f(r, z.re);
        if r + 1 < r1 {
            f(r + 1, -z.im);
        }
        w.inc();
        r += 2;
    }
}

/// Views an f32 buffer of even length as complex numbers (re, im interleaved).
#[inline]
pub fn as_complex(x: &mut [f32]) -> &mut [C32] {
    let len = x.len() / 2;
    // SAFETY: Complex<f32> is #[repr(C)] { re: f32, im: f32 }: two f32, same alignment as f32.
    unsafe { std::slice::from_raw_parts_mut(x.as_mut_ptr() as *mut C32, len) }
}

/// Read-only view, as [`as_complex`].
#[inline]
pub fn as_complex_ref(x: &[f32]) -> &[C32] {
    let len = x.len() / 2;
    // SAFETY: as in `as_complex`.
    unsafe { std::slice::from_raw_parts(x.as_ptr() as *const C32, len) }
}

/// Xorshift32: the seeded source of the random phases (one draw per bin per hop).
#[derive(Clone, Copy, Debug)]
pub struct XorShift {
    state: u32,
    seed: u32,
}

impl XorShift {
    pub fn new(seed: u32) -> Self {
        let s = if seed == 0 { 0x9E37_79B9 } else { seed };
        Self { state: s, seed: s }
    }
    pub fn reset(&mut self) {
        self.state = self.seed;
    }
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }
    /// A 12-bit phase index.
    #[inline]
    pub fn phase(&mut self) -> usize {
        (self.next_u32() >> 20) as usize
    }
    /// Uniform in [−1, 1).
    #[inline]
    pub fn bipolar(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (2.0 / 16_777_216.0) - 1.0
    }
}

/// The unit phasors e^(2πi j / 4096).
pub struct Phasors {
    t: Vec<C32>,
}

impl Phasors {
    pub fn new() -> Self {
        Self {
            t: (0..PHASORS)
                .map(|j| {
                    let a = std::f64::consts::TAU * j as f64 / PHASORS as f64;
                    C32::new(a.cos() as f32, a.sin() as f32)
                })
                .collect(),
        }
    }
    #[inline]
    pub fn get(&self, j: usize) -> C32 {
        self.t[j & (PHASORS - 1)]
    }
    /// The table index of a phase in turns ×4096 (any sign).
    #[inline]
    pub fn index(turns4096: f32) -> usize {
        ((turns4096 + 0.5f32.copysign(turns4096)) as i32 as usize) & (PHASORS - 1)
    }
}

impl Default for Phasors {
    fn default() -> Self {
        Self::new()
    }
}

/// Finds the peaks of `m[0..nb]` (local maxima over ±2 bins, above `floor`) and gives every bin the
/// peak of its region (regions end at the minimum between two peaks). Bins under the floor and away
/// from every peak get `u32::MAX`. (gfx-glitch's, in one piece; [`RegionSweep`] is the resumable form.)
pub fn regions(m: &[f32], nb: usize, floor: f32, peaks: &mut Vec<u32>, owner: &mut [u32]) {
    peaks.clear();
    for k in 1..nb.saturating_sub(1) {
        if is_peak(m, nb, floor, k) {
            peaks.push(k as u32);
        }
    }
    owner[..nb].fill(u32::MAX);
    if peaks.is_empty() {
        return;
    }
    let mut start = 0usize;
    for i in 0..peaks.len() {
        let p = peaks[i] as usize;
        let end = if i + 1 < peaks.len() {
            let nx = peaks[i + 1] as usize;
            let mut kmin = p;
            for k in p..nx {
                if m[k] < m[kmin] {
                    kmin = k;
                }
            }
            kmin
        } else {
            nb - 1
        };
        for o in owner.iter_mut().take(end + 1).skip(start) {
            *o = p as u32;
        }
        start = end + 1;
    }
}

/// gfx-glitch's peak test: a local maximum over ±2 bins, above `floor`.
#[inline]
pub fn is_peak(m: &[f32], nb: usize, floor: f32, k: usize) -> bool {
    if k == 0 || k + 1 >= nb {
        return false;
    }
    let v = m[k];
    v > floor && v >= m[k - 1] && v > m[k + 1] && (k < 2 || v >= m[k - 2]) && (k + 2 >= nb || v > m[k + 2])
}

/// [`regions`] in two resumable sweeps with O(1) state: a forward sweep (bins ascending, any chunks
/// in order) records each bin's last peak and the minimum since it (exclusive); a backward sweep
/// (chunks descending) keeps the minimum up to the next peak and decides. Bin k between peaks a < b
/// belongs to a when min[a, k) > min[k, b): the region boundary is the first minimum, as in
/// [`regions`]. Same result, bit for bit.
#[derive(Clone, Copy, Debug)]
pub struct RegionSweep {
    last: u32,
    rmin: f32,
    next: u32,
    bmin: f32,
}

impl Default for RegionSweep {
    fn default() -> Self {
        Self::new()
    }
}

impl RegionSweep {
    pub fn new() -> Self {
        Self { last: u32::MAX, rmin: f32::INFINITY, next: u32::MAX, bmin: f32::INFINITY }
    }
    /// Forward over `[k0, k1)`: `owner[k]` = the last peak ≤ k (or MAX), `fmin[k]` = min of m over
    /// [that peak, k).
    #[allow(clippy::too_many_arguments)]
    pub fn forward(&mut self, m: &[f32], nb: usize, floor: f32, k0: usize, k1: usize, owner: &mut [u32], fmin: &mut [f32]) {
        for k in k0..k1 {
            if is_peak(m, nb, floor, k) {
                self.last = k as u32;
                self.rmin = f32::INFINITY;
            }
            owner[k] = self.last;
            fmin[k] = self.rmin;
            if self.last != u32::MAX {
                self.rmin = self.rmin.min(m[k]);
            }
        }
    }
    /// Backward over `[k0, k1)` (call with chunks in descending order): final owners.
    pub fn backward(&mut self, m: &[f32], k0: usize, k1: usize, owner: &mut [u32], fmin: &[f32]) {
        for k in (k0..k1).rev() {
            let a = owner[k];
            if a != u32::MAX && a as usize == k {
                self.next = k as u32;
                self.bmin = f32::INFINITY;
                continue;
            }
            if self.next == u32::MAX {
                continue; // after the last peak: its region (or none)
            }
            self.bmin = self.bmin.min(m[k]);
            if a == u32::MAX || fmin[k] <= self.bmin {
                owner[k] = self.next;
            }
        }
    }
}

/// e^(−x) for x ≥ 0 (relative error < 3e-7; 0 beyond x = 87).
#[inline]
pub fn exp_neg(x: f32) -> f32 {
    let y = -(x as f64) * std::f64::consts::LOG2_E;
    if y < -126.0 {
        return 0.0;
    }
    let i = y.round();
    let f = (y - i) as f32;
    // 2^f on [−0.5, 0.5]: the degree-6 Taylor series of e^(f ln 2)
    let p = 1.0 + f * (std::f32::consts::LN_2 + f * (0.240_226_5 + f * (0.055_504_11 + f * (0.009_618_129 + f * (0.001_333_355_8 + f * 0.000_154_035_3)))));
    p * f32::from_bits(((i as i32 + 127) as u32) << 23)
}

/// Wraps a phase to [−π, π] without a libm call (round half away from zero by an integer cast).
#[inline]
pub fn wrap_pi(x: f32) -> f32 {
    let y = x * (1.0 / std::f32::consts::TAU);
    let k = (y + 0.5f32.copysign(y)) as i32 as f32;
    x - std::f32::consts::TAU * k
}

/// atan2 by a polynomial (degree 13, odd) after octant reduction: error < 4e-7 rad, no libm call,
/// branch-free (selects), so random phases cost no mispredictions.
#[inline]
pub fn fast_atan2(y: f32, x: f32) -> f32 {
    use std::f32::consts::{FRAC_PI_2, PI};
    let (ax, ay) = (x.abs(), y.abs());
    let mx = ax.max(ay);
    let mn = ax.min(ay);
    let z = if mx > 0.0 { mn / mx } else { 0.0 };
    let t = z * z;
    let p = 0.999_996_6
        + t * (-0.333_183_03 + t * (0.198_132 + t * (-0.132_474_68 + t * (0.079_810_166 + t * (-0.033_725_016 + t * 0.006_842_313)))));
    let a = p * z;
    let a = if ay > ax { FRAC_PI_2 - a } else { a };
    let a = if x < 0.0 { PI - a } else { a };
    a.copysign(y)
}

/// (sin x, cos x) for x ∈ [−π, π] by polynomials (degree 13 / 12): error < 1e-6 (f32 rounding near ±π), no libm call.
#[inline]
pub fn fast_sin_cos(x: f32) -> (f32, f32) {
    let t = x * x;
    let s = x * (0.999_999_99 + t * (-0.166_666_65 + t * (0.008_333_31 + t * (-0.000_198_401_52 + t * (2.752_939_3e-6 + t * (-2.467_646_9e-8 + t * 1.344_990_8e-10))))));
    let c = 0.999_999_99 + t * (-0.499_999_9 + t * (0.041_666_49 + t * (-0.001_388_780_3 + t * (2.476_988e-5 + t * (-2.707_9e-7 + t * 1.724_497_4e-9)))));
    (s, c)
}

/// One pass of the constant-Q blur over bins `[k0, k1)` of the power spectrum `p`, in scatter form:
/// s ← s + P_k, P_k ← (1 − a_k) s, s ← a_k s, with a_k = e^(−1 / τ_k), τ_k = k · `c` and
/// `c` = ln 2 · σ / 2,400 (σ in cents). Forward passes walk up, backward passes down; chunks must
/// follow the same order with `s` carried. Each bin's energy is handed on, never copied, so the sum
/// is kept (what leaves at the band's ends is lost). Four passes (up, down, up, down) give a
/// near-Gaussian of standard deviation σ cents.
pub fn blur_pass(p: &mut [f32], k0: usize, k1: usize, forward: bool, c: f32, s: &mut f32) {
    let step = |k: usize, s: &mut f32, v: &mut f32| {
        let tau = k as f32 * c;
        let a = if tau > 1e-6 { exp_neg(1.0 / tau) } else { 0.0 };
        *s += *v;
        *v = (1.0 - a) * *s;
        *s *= a;
    };
    if forward {
        for k in k0..k1 {
            step(k, s, &mut p[k]);
        }
    } else {
        for k in (k0..k1).rev() {
            step(k, s, &mut p[k]);
        }
    }
}

/// The blur's coefficient c = ln 2 · σ / 2,400 for σ in cents.
#[inline]
pub fn blur_coef(sigma_cents: f32) -> f32 {
    std::f32::consts::LN_2 * sigma_cents / 2400.0
}

/// The crossfade from one engine to another after a size change: wait until the new engine's frames
/// overlap fully, then equal power over `len` samples (gfx-glitch's rule).
#[derive(Clone, Copy, Debug, Default)]
pub struct SizeFade {
    pub on: bool,
    wait: usize,
    pos: usize,
    len: usize,
}

impl SizeFade {
    pub fn start(&mut self, wait: usize, len: usize) {
        *self = Self { on: true, wait, pos: 0, len: len.max(1) };
    }
    /// The (old, new) gains for this sample, and whether the fade has just completed.
    #[inline]
    pub fn next(&mut self) -> (f32, f32, bool) {
        if !self.on {
            return (1.0, 0.0, false);
        }
        if self.wait > 0 {
            self.wait -= 1;
            return (1.0, 0.0, false);
        }
        let t = (self.pos as f32 + 0.5) / self.len as f32;
        let (s, c) = (t * std::f32::consts::FRAC_PI_2).sin_cos();
        self.pos += 1;
        let done = self.pos >= self.len;
        if done {
            self.on = false;
        }
        (c, s, done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use realfft::RealFftPlanner;

    fn lcg(s: &mut u64) -> f32 {
        *s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((*s >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
    }

    #[test]
    fn grid_is_half_octaves() {
        assert_eq!(size(0), 256);
        assert_eq!(size(1), 384);
        assert_eq!(size(NUM_SIZES - 1), MAX_N);
        assert_eq!(size(NUM_SIZES - 2), 196_608);
        for i in 1..NUM_SIZES {
            let r = size(i) as f64 / size(i - 1) as f64;
            assert!(r == 1.5 || (r - 4.0 / 3.0).abs() < 1e-12);
        }
        // 683 ms at 48 kHz is 32,768; 5 ms → 256; 5.5 s → 2^18
        assert_eq!(size(nearest_size(0.683 * 48000.0)), 32768);
        assert_eq!(size(nearest_size(0.005 * 48000.0)), 256);
        assert_eq!(size(nearest_size(5.5 * 48000.0)), MAX_N);
        assert_eq!(size(nearest_size(5.5 * 192000.0)), MAX_N);
    }

    /// Forward and inverse, every size, against realfft (f32): the four-step sizes within 1e-6
    /// relative RMS (README §5), the one-piece sizes too.
    #[test]
    fn real_fft_matches_realfft_every_size() {
        let t = Tables::new();
        let mut scratch = vec![C32::default(); t.scratch_len];
        let mut tmp = vec![C32::default(); t.tmp_len];
        let mut s = 99u64;
        for (i, plan) in t.plans.iter().enumerate() {
            let n = plan.n;
            let x: Vec<f32> = (0..n).map(|_| lcg(&mut s)).collect();
            // ours, forward
            let mut buf = x.clone();
            {
                let c = as_complex(&mut buf);
                for u in 0..plan.units {
                    cfft_unit(plan, &t, c, &mut tmp, &mut scratch, u);
                }
            }
            let mut ours = vec![C32::default(); plan.m + 1];
            post_twiddle(plan, &t, as_complex_ref(&buf), 0, plan.m + 1, |k, v| ours[k] = v);
            // reference
            let f = RealFftPlanner::<f32>::new().plan_fft_forward(n);
            let mut xr = x.clone();
            let mut want = f.make_output_vec();
            f.process(&mut xr, &mut want).unwrap();
            let (mut e, mut p) = (0.0f64, 0.0f64);
            for k in 0..=plan.m {
                e += (ours[k] - want[k]).norm_sqr() as f64;
                p += want[k].norm_sqr() as f64;
            }
            let rel = (e / p).sqrt();
            assert!(rel < 1e-6, "size {n} (#{i}) forward: {rel:.2e}");
            // inverse of the reference spectrum
            let mut b2 = vec![0.0f32; n];
            {
                let c = as_complex(&mut b2);
                for k in 0..plan.m {
                    c[k] = pre_twiddle(plan, &t, k, want[k], want[plan.m - k]);
                }
                for u in 0..plan.units {
                    cfft_unit(plan, &t, c, &mut tmp, &mut scratch, u);
                }
            }
            let mut back = vec![0.0f32; n];
            read_time(plan, as_complex_ref(&b2), 0, n, |r, v| back[r] = v / n as f32);
            let (mut e, mut p) = (0.0f64, 0.0f64);
            for r in 0..n {
                e += ((back[r] - x[r]) as f64).powi(2);
                p += (x[r] as f64).powi(2);
            }
            let rel = (e / p).sqrt();
            assert!(rel < 1e-6, "size {n} (#{i}) inverse: {rel:.2e}");
            // read_time in odd chunks gives the same
            let mut back2 = vec![0.0f32; n];
            let mut r = 0;
            while r < n {
                let r1 = (r + 777).min(n);
                read_time(plan, as_complex_ref(&b2), r, r1, |q, v| back2[q] = v / n as f32);
                r = r1;
            }
            assert_eq!(back, back2);
        }
    }

    #[test]
    fn four_step_units_are_small() {
        let t = Tables::new();
        for p in &t.plans {
            if p.n > FOUR_STEP_ABOVE {
                assert!(p.n1 <= 1024 && p.n2 <= 1024 && p.n1 * p.n2 == p.m, "{} = {} × {}", p.m, p.n1, p.n2);
                assert!(p.cols * p.n1 <= UNIT && p.rows * p.n2 <= UNIT);
            } else {
                assert_eq!(p.units, 1);
            }
        }
        assert!(t.plans[NUM_SIZES - 1].units <= 256);
    }

    #[test]
    fn hann_and_ola_constants() {
        let t = Tables::new();
        for p in [&t.plans[0], &t.plans[5], &t.plans[NUM_SIZES - 2]] {
            let n = p.n;
            let h = n / 4;
            for j in [0, 1, h / 3, h - 1] {
                let (mut s1, mut s2) = (0.0f64, 0.0f64);
                let mut i = j;
                while i < n {
                    let w = p.hann(&t, i) as f64;
                    s1 += w * w;
                    s2 += w;
                    i += h;
                }
                assert!((s1 - 1.5).abs() < 1e-5, "{n}: Σw² {s1}");
                assert!((s2 - 2.0).abs() < 1e-5, "{n}: Σw {s2}");
            }
            let mw2: f64 = (0..n).map(|i| (p.hann(&t, i) as f64).powi(2)).sum::<f64>() / n as f64;
            assert!((mw2 - 0.375).abs() < 1e-6);
        }
        assert!((cloud_gain(0.0) - 1.0 / 1.5).abs() < 1e-6);
        assert!((cloud_gain(1.0) - 1.0 / 0.75).abs() < 1e-5);
    }

    #[test]
    fn schedule_spreads_units_and_finishes_in_the_hop() {
        for (units, hop) in [(1usize, 64usize), (10, 64), (100, 64), (1600, 65536), (51, 8192)] {
            let mut prev = 0;
            let mut first = None;
            for d in 0..hop {
                let k = due(d, units, hop);
                assert!(k >= prev && k <= units);
                if k > 0 && first.is_none() {
                    first = Some(d);
                }
                prev = k;
            }
            assert_eq!(prev, units, "{units} units over {hop}");
            if units < hop {
                // unit i at ⌊(i + 1) hop / (U + 1)⌋
                assert_eq!(first, Some(hop / (units + 1)));
            }
        }
    }

    #[test]
    fn region_sweeps_equal_regions() {
        let mut s = 5u64;
        for trial in 0..60 {
            let nb = 50 + trial * 37;
            let mut m: Vec<f32> = (0..nb).map(|_| (lcg(&mut s) * 0.5 + 0.5).powi(4)).collect();
            if trial % 3 == 0 {
                // plateaus and ties
                for k in (0..nb).step_by(7) {
                    m[k] = 0.25;
                    if k + 1 < nb {
                        m[k + 1] = 0.25;
                    }
                }
            }
            let floor = if trial % 5 == 0 { 0.3 } else { 1e-6 };
            let mut peaks = Vec::new();
            let mut want = vec![0u32; nb];
            regions(&m, nb, floor, &mut peaks, &mut want);
            let mut got = vec![0u32; nb];
            let mut fmin = vec![0f32; nb];
            let mut sw = RegionSweep::new();
            let step = 1 + trial % 13;
            let mut k = 0;
            while k < nb {
                let k1 = (k + step).min(nb);
                sw.forward(&m, nb, floor, k, k1, &mut got, &mut fmin);
                k = k1;
            }
            let mut k1 = nb;
            while k1 > 0 {
                let k0 = k1.saturating_sub(step);
                sw.backward(&m, k0, k1, &mut got, &fmin);
                k1 = k0;
            }
            // bins before the first peak take it; with no peak, none
            assert_eq!(got, want, "trial {trial}");
        }
    }

    #[test]
    fn blur_keeps_energy_and_width_in_cents() {
        // one partial at 1 kHz, N = 32,768 at 48 kHz: σ within 2 % of the setting, energy kept
        let n = 32768usize;
        let sr = 48000.0f64;
        let nb = n / 2 + 1;
        let k0 = (1000.0 * n as f64 / sr).round() as usize;
        for sigma in [50.0f32, 200.0, 1200.0] {
            let mut p = vec![0.0f32; nb];
            p[k0] = 1.0;
            let c = blur_coef(sigma);
            for pass in 0..4 {
                let mut st = 0.0;
                let fwd = pass % 2 == 0;
                let u = chunks(nb);
                for q in 0..u {
                    let q = if fwd { q } else { u - 1 - q };
                    let (a, b) = chunk(nb, q);
                    blur_pass(&mut p, a, b, fwd, c, &mut st);
                }
            }
            let e: f64 = p.iter().map(|&v| v as f64).sum();
            let mean: f64 = p.iter().enumerate().skip(1).map(|(k, &v)| v as f64 * 1200.0 * (k as f64 / k0 as f64).log2()).sum::<f64>() / e;
            let var: f64 = p.iter().enumerate().skip(1).map(|(k, &v)| v as f64 * (1200.0 * (k as f64 / k0 as f64).log2() - mean).powi(2)).sum::<f64>() / e;
            let sd = var.sqrt();
            assert!((10.0 * e.log10()).abs() < 0.05, "σ {sigma}: energy {e}");
            assert!((sd / sigma as f64 - 1.0).abs() < 0.03, "σ {sigma}: measured {sd:.1} ¢");
            assert!(mean.abs() < 10.0, "σ {sigma}: centroid {mean:.1} ¢");
        }
    }

    #[test]
    fn exp_neg_is_accurate() {
        let mut x = 0.0f32;
        while x < 80.0 {
            let want = (-(x as f64)).exp();
            let got = exp_neg(x) as f64;
            assert!(((got - want) / want).abs() < 3e-7, "{x}: {got} vs {want}");
            x += 0.0137;
        }
    }

    #[test]
    fn fast_trig_is_accurate() {
        let mut worst = (0.0f64, 0.0f64, 0.0f64);
        let mut s = 7u64;
        for _ in 0..200_000 {
            let (y, x) = (lcg(&mut s) * 3.0, lcg(&mut s) * 3.0);
            let e = (fast_atan2(y, x) as f64 - (y as f64).atan2(x as f64)).abs();
            worst.0 = worst.0.max(e);
            let a = lcg(&mut s) * std::f32::consts::PI;
            let (sn, cs) = fast_sin_cos(a);
            worst.1 = worst.1.max((sn as f64 - (a as f64).sin()).abs());
            worst.2 = worst.2.max((cs as f64 - (a as f64).cos()).abs());
            let w = lcg(&mut s) * 100.0;
            let r = wrap_pi(w) as f64;
            assert!(r.abs() <= std::f64::consts::PI + 1e-5);
            assert!(((w as f64 - r) / std::f64::consts::TAU - ((w as f64 - r) / std::f64::consts::TAU).round()).abs() < 1e-4);
        }
        assert!(worst.0 < 6e-7, "atan2 {:.2e}", worst.0);
        assert!(worst.1 < 1e-6 && worst.2 < 1e-6, "sin {:.2e} cos {:.2e}", worst.1, worst.2);
        assert_eq!(fast_atan2(0.0, 0.0), 0.0);
        assert_eq!(Phasors::index(-1.0), PHASORS - 1);
        assert_eq!(Phasors::index(2.4), 2);
        assert!((fast_atan2(0.0, -1.0) - std::f32::consts::PI).abs() < 1e-6);
    }

    #[test]
    fn size_fade_is_equal_power() {
        let mut f = SizeFade::default();
        f.start(10, 100);
        let mut n = 0;
        loop {
            let (a, b, done) = f.next();
            assert!(((a * a + b * b) - 1.0).abs() < 1e-5);
            n += 1;
            if done {
                break;
            }
        }
        assert_eq!(n, 110);
    }
}
