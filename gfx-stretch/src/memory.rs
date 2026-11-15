// SPDX-License-Identifier: GPL-3.0-or-later
//! The memory pool (README §2.10): per channel, 2 × (60 s + 0.25 s) at the host rate.
//!
//! - **Capture**: two take buffers, A = [0, T) and B = [T, 2T); a take is read circularly over its
//!   length.
//! - **Rolling**: one ring over the whole pool (2T). Positions are absolute sample counts since the
//!   mode started; only [w − 2T, w) is valid, anything else reads as 0.
//! - Stored as **f16** (the `half` crate, F16C at run time where the CPU has it); the pool is generic
//!   over its sample so the tests also run it in f32.
//! - Allocated and **written once** in `prepare` (no page is first touched in `process`). Nothing in
//!   `process` ever clears it: buffers carry their valid lengths.

use half::f16;
use half::slice::HalfFloatSliceExt;

/// A stored sample.
pub trait Sample: Copy + Default + Send + Sync + 'static {
    fn enc(x: f32) -> Self;
    fn dec(self) -> f32;
    /// Decodes a run (F16C where available).
    fn dec_into(src: &[Self], dst: &mut [f32]);
}

impl Sample for f32 {
    #[inline]
    fn enc(x: f32) -> Self {
        x
    }
    #[inline]
    fn dec(self) -> f32 {
        self
    }
    #[inline]
    fn dec_into(src: &[Self], dst: &mut [f32]) {
        dst.copy_from_slice(src);
    }
}

impl Sample for f16 {
    #[inline]
    fn enc(x: f32) -> Self {
        f16::from_f32(x)
    }
    #[inline]
    fn dec(self) -> f32 {
        self.to_f32()
    }
    #[inline]
    fn dec_into(src: &[Self], dst: &mut [f32]) {
        src.convert_to_f32_slice(dst);
    }
}

/// Where a head reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Src {
    /// A committed take: slot 0 / 1, read circularly over `len` samples.
    Take { slot: usize, len: usize },
    /// The live ring, `w` samples written so far.
    Ring { w: u64 },
}

impl Default for Src {
    fn default() -> Self {
        Src::Ring { w: 0 }
    }
}

/// A vector of `n` copies of `v`, every page written now (not left to the first write in `process`).
pub fn touched<T: Copy>(n: usize, v: T) -> Vec<T> {
    let mut x = Vec::with_capacity(n);
    x.resize(n, v);
    // a volatile write per 4 KiB page: the allocator cannot hand back untouched (calloc'd) pages
    let step = (4096 / std::mem::size_of::<T>().max(1)).max(1);
    let p = x.as_mut_ptr();
    let mut i = 0;
    while i < n {
        // SAFETY: i < n = len; the element is initialised (resize).
        unsafe { std::ptr::write_volatile(p.add(i), v) };
        i += step;
    }
    x
}

pub struct Pool<S: Sample> {
    data: Vec<Vec<S>>,
    /// T: one take's capacity in samples (60.25 s)
    pub half: usize,
}

impl<S: Sample> Pool<S> {
    pub fn new() -> Self {
        Self { data: Vec::new(), half: 0 }
    }

    /// Allocates (and writes) 2 × 60.25 s per channel. May allocate; nothing else does.
    pub fn prepare(&mut self, sr: f32, channels: usize) {
        let half = (60.25 * sr as f64).ceil() as usize;
        if self.half != half || self.data.len() != channels {
            self.data = Vec::new();
            self.data = (0..channels).map(|_| touched(2 * half, S::default())).collect();
            self.half = half;
        }
    }

    /// The whole ring's capacity (Rolling).
    #[inline]
    pub fn cap(&self) -> usize {
        2 * self.half
    }

    pub fn bytes(&self) -> usize {
        self.data.iter().map(|d| d.len() * std::mem::size_of::<S>()).sum()
    }

    #[inline]
    pub fn write_take(&mut self, ch: usize, slot: usize, i: usize, x: f32) {
        self.data[ch][slot * self.half + i] = S::enc(x);
    }

    #[inline]
    pub fn read_take(&self, ch: usize, slot: usize, i: usize) -> f32 {
        self.data[ch][slot * self.half + i].dec()
    }

    /// Writes the ring at absolute position `w`.
    #[inline]
    pub fn write_ring(&mut self, ch: usize, w: u64, x: f32) {
        let cap = self.cap() as u64;
        self.data[ch][(w % cap) as usize] = S::enc(x);
    }

    /// `dst.len()` samples from position `start` of `src` (decoded in runs).
    pub fn read(&self, ch: usize, src: Src, start: i64, dst: &mut [f32]) {
        let d = &self.data[ch];
        let n = dst.len();
        match src {
            Src::Take { slot, len } => {
                if len == 0 {
                    dst.fill(0.0);
                    return;
                }
                let base = slot * self.half;
                let mut i = 0;
                let mut q = start.rem_euclid(len as i64) as usize;
                while i < n {
                    let run = (len - q).min(n - i);
                    S::dec_into(&d[base + q..base + q + run], &mut dst[i..i + run]);
                    i += run;
                    q = 0;
                }
            }
            Src::Ring { w } => {
                let cap = self.cap() as i64;
                let lo = (w as i64 - cap).max(0);
                let hi = w as i64;
                let mut i = 0usize;
                while i < n {
                    let q = start + i as i64;
                    if q < lo || q >= hi {
                        // not written (yet, or any more): silence until the valid range
                        let until = if q < lo { lo - q } else { n as i64 };
                        let run = (until.max(1) as usize).min(n - i);
                        dst[i..i + run].fill(0.0);
                        i += run;
                        continue;
                    }
                    let at = q.rem_euclid(cap) as usize;
                    let run = ((hi - q) as usize).min(cap as usize - at).min(n - i);
                    S::dec_into(&d[at..at + run], &mut dst[i..i + run]);
                    i += run;
                }
            }
        }
    }
}

impl<S: Sample> Default for Pool<S> {
    fn default() -> Self {
        Self::new()
    }
}
