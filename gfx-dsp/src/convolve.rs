// SPDX-License-Identifier: GPL-3.0-or-later
//! Zero-latency convolution: a direct-form head and non-uniformly partitioned overlap-save levels whose
//! work is spread over the audio callbacks (gfx-cab README §2.3 – §2.4). Everything runs on the calling
//! thread, so a render does not depend on the host's block size or on any thread's timing.
//!
//! - **Head**: taps [0, 64) as a direct FIR, per sample (8 lanes). Tap 0 multiplies the current input:
//!   the latency is 0 at every block size.
//! - **Levels** (real FFTs of 2B, overlap-save):
//!
//!   | Level | B | First tap | Partitions | Work |
//!   |---|---|---|---|---|
//!   | 1 | 64 | 64 | 7 | at once, when its input chunk completes |
//!   | 2 | 256 | 512 | 6 | spread over 4 chunks |
//!   | 3 | 1,024 | 2,048 | 6 | 16 chunks, starting 1 chunk late |
//!   | 4 | 4,096 | 8,192 | 6 | 63 chunks, starting 2 late; transforms cut in 4 |
//!   | 5 | 8,192 | 32,768 | to the class limit | 126 chunks, starting 3 late; transforms cut in 8 |
//!
//!   Level 5's offset is 4B: it runs as an offset of 2B with two empty leading partitions.
//! - **Internal chunk of 64 samples**, counted from `reset()`. At each chunk boundary level 1 runs and
//!   the levels above do their share of the cycle (forward transform, MACs, inverse transforms), balanced
//!   by a cost model. Kernels change only at chunk boundaries, so the output is bit-identical at every
//!   host block size.
//! - **Split transforms** (levels 4 and 5): the 2B-point real transform is a B-point complex one on the
//!   packed signal, cut by 2 or 3 radix-2 decimation-in-frequency passes into 4 or 8 sub-transforms of
//!   1,024 points, one unit per chunk; then the real-signal post-pass. No chunk carries a transform
//!   larger than a 1,024-point complex FFT (a 2,048-point real one).
//! - **One input FDL per level, many kernels.** The spectra of past input blocks are computed once and
//!   kept for the whole class layout, whatever the kernels use; a kernel is only its partition spectra.
//!   A kernel installed later finds the input history ready: no cold start.
//! - **Voices and per-level crossfades.** A [`Convolver`] plays several voices (gfx-cab's A and B) from
//!   one FDL. A new kernel for a voice is taken at the next chunk boundary by the head and level 1, and
//!   by each higher level at its next cycle start; each level then crossfades old → new linearly over
//!   30 ms from its first new block (≤ 2B after the request). Requests that arrive during a crossfade
//!   wait; only the latest is kept.
//! - Old kernels are never dropped here: they are handed back through [`Convolver::take_retired`] so
//!   the caller can release them off the audio thread.
//!
//! f32 data and FFTs. Nothing allocates after [`Convolver::prepare`].

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use rustfft::{Fft, FftPlanner};
use std::cell::RefCell;
use std::sync::Arc;

type C32 = Complex<f32>;

/// The internal chunk (samples) and the head's length (taps).
pub const CHUNK: usize = 64;
pub const HEAD: usize = 64;
/// Crossfade per level (s).
pub const FADE_S: f32 = 0.030;
const NLEV: usize = 5;
const RING: usize = 32768;
const RETIRED: usize = 32;

/// One level of the layout.
#[derive(Clone, Copy, Debug)]
pub struct LevelDef {
    /// Block (partition) size.
    pub b: usize,
    /// First tap of partition index 0 (the scheduling offset: B for level 1, 2B above).
    pub base: usize,
    /// First partition index that may hold taps (2 for level 5: its offset is 4B).
    pub q0: usize,
    /// Partition count limit (`usize::MAX`: up to the class limit).
    pub parts: usize,
    /// Chunks the cycle starts late (stagger).
    pub stagger: usize,
    /// Sub-transforms of a split level (1 = whole).
    pub split: usize,
}

pub const LEVELS: [LevelDef; NLEV] = [
    LevelDef { b: 64, base: 64, q0: 0, parts: 7, stagger: 0, split: 1 },
    LevelDef { b: 256, base: 512, q0: 0, parts: 6, stagger: 0, split: 1 },
    LevelDef { b: 1024, base: 2048, q0: 0, parts: 6, stagger: 1, split: 1 },
    LevelDef { b: 4096, base: 8192, q0: 0, parts: 6, stagger: 2, split: 4 },
    LevelDef { b: 8192, base: 16384, q0: 2, parts: usize::MAX, stagger: 3, split: 8 },
];

/// Partition count of level `l` needed to cover `len` taps.
pub fn parts_for(l: usize, len: usize) -> usize {
    let d = LEVELS[l];
    if len <= d.base + d.q0 * d.b {
        return 0;
    }
    (len - d.base).div_ceil(d.b).min(d.parts)
}

thread_local! {
    static PLANNER: RefCell<RealFftPlanner<f32>> = RefCell::new(RealFftPlanner::new());
}

/// A prepared impulse response: the reversed head and the partition spectra of every level (scaled
/// for the unnormalised inverse). Immutable; shared between instances through `Arc`.
pub struct Kernel {
    len: usize,
    head_rev: [f32; HEAD],
    head_nz: bool,
    /// per level: (partition index, spectrum of B + 1 bins)
    parts: [Vec<(usize, Vec<C32>)>; NLEV],
}

impl std::fmt::Debug for Kernel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kernel").field("len", &self.len).finish()
    }
}

impl Kernel {
    /// Partitions `ir` (any length; taps beyond a convolver's class limit are not played). Allocates:
    /// call it off the audio thread. Empty partitions are not stored.
    pub fn new(ir: &[f32]) -> Self {
        let len = ir.len();
        let mut head_rev = [0.0f32; HEAD];
        for (k, &h) in ir.iter().take(HEAD).enumerate() {
            head_rev[HEAD - 1 - k] = h;
        }
        let head_nz = head_rev.iter().any(|&h| h != 0.0);
        let mut parts: [Vec<(usize, Vec<C32>)>; NLEV] = Default::default();
        PLANNER.with(|p| {
            let mut planner = p.borrow_mut();
            for (l, d) in LEVELS.iter().enumerate() {
                let n = parts_for(l, len);
                if n == 0 {
                    continue;
                }
                let fft = planner.plan_fft_forward(2 * d.b);
                let mut buf = vec![0.0f32; 2 * d.b];
                let scale = 1.0 / (2 * d.b) as f32;
                for q in d.q0..n {
                    let s = d.base + q * d.b;
                    let e = (s + d.b).min(len);
                    if s >= e || ir[s..e].iter().all(|&v| v == 0.0) {
                        continue;
                    }
                    buf.iter_mut().for_each(|v| *v = 0.0);
                    buf[..e - s].copy_from_slice(&ir[s..e]);
                    let mut spec = fft.make_output_vec();
                    fft.process(&mut buf, &mut spec).expect("fft sizes");
                    spec.iter_mut().for_each(|c| *c *= scale);
                    parts[l].push((q, spec));
                }
            }
        });
        Self { len, head_rev, head_nz, parts }
    }

    /// Taps.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Memory held (bytes), for caches.
    pub fn bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.parts.iter().flatten().map(|(_, s)| s.len() * 8).sum::<usize>()
    }

    /// Stored (non-empty) partitions per level.
    pub fn partitions(&self, level: usize) -> usize {
        self.parts[level].len()
    }
}

/// Pieces of a radix-2 pass and of the real post / pre-pass, so that no step of a split level costs
/// more than about one sub-transform.
const PASS_PIECES: usize = 2;
const POST_PIECES: usize = 4;
const PACK_PIECES: usize = 4;

/// The split transform of a level (B-point complex on the packed signal, R sub-transforms). Its
/// steps: forward = the packing of the input in 4 pieces, P passes × 2 pieces, R sub-transforms, the
/// post-pass in 4 pieces; inverse = the pre-pass in 4 pieces, R sub-transforms, P passes × 2 pieces
/// (the last writes the output).
struct Split {
    b: usize,
    r: usize,
    passes: usize,
    m: usize,
    fwd: Arc<dyn Fft<f32>>,
    inv: Arc<dyn Fft<f32>>,
    /// e^{−2πik/B}, k < B / 2
    tw: Vec<C32>,
    /// e^{−iπk/B}, k ≤ B (the real post-pass)
    tw2: Vec<C32>,
    /// where bin k of the B-point transform lives after the passes and the sub-transforms
    perm: Vec<u32>,
    scratch: Vec<C32>,
}

impl Split {
    fn new(b: usize, r: usize) -> Self {
        let m = b / r;
        let passes = r.trailing_zeros() as usize;
        let mut pl = FftPlanner::<f32>::new();
        let fwd = pl.plan_fft_forward(m);
        let inv = pl.plan_fft_inverse(m);
        let tw = (0..b / 2)
            .map(|k| {
                let a = -2.0 * std::f64::consts::PI * k as f64 / b as f64;
                C32::new(a.cos() as f32, a.sin() as f32)
            })
            .collect();
        let tw2 = (0..=b)
            .map(|k| {
                let a = -std::f64::consts::PI * k as f64 / b as f64;
                C32::new(a.cos() as f32, a.sin() as f32)
            })
            .collect();
        let perm = (0..b)
            .map(|k| {
                let g = (k % r).reverse_bits() >> (usize::BITS as usize - passes);
                (g * m + k / r) as u32
            })
            .collect();
        let sl = fwd.get_inplace_scratch_len().max(inv.get_inplace_scratch_len());
        Self { b, r, passes, m, fwd, inv, tw, tw2, perm, scratch: vec![C32::new(0.0, 0.0); sl] }
    }

    fn fwd_steps(&self) -> usize {
        PACK_PIECES + self.passes * PASS_PIECES + self.r + POST_PIECES
    }

    fn inv_steps(&self) -> usize {
        POST_PIECES + self.r + self.passes * PASS_PIECES
    }

    /// Packs 2B real samples into B complex ones.
    fn pack(&self, x: &[f32], z: &mut [C32]) {
        for (n, c) in z.iter_mut().enumerate() {
            *c = C32::new(x[2 * n], x[2 * n + 1]);
        }
    }

    /// Piece `piece` of the forward decimation-in-frequency pass `p` (segments of B >> p).
    fn pass(&self, z: &mut [C32], p: usize, piece: usize) {
        let len = self.b >> p;
        let h = len / 2;
        let (n0, n1) = (h * piece / PASS_PIECES, h * (piece + 1) / PASS_PIECES);
        for seg in z.chunks_exact_mut(len) {
            let (a, b) = seg.split_at_mut(h);
            for n in n0..n1 {
                let (u, v) = (a[n], b[n]);
                a[n] = u + v;
                b[n] = (u - v) * self.tw[n << p];
            }
        }
    }

    fn sub_fwd(&mut self, z: &mut [C32], g: usize) {
        let m = self.m;
        self.fwd.process_with_scratch(&mut z[g * m..(g + 1) * m], &mut self.scratch);
    }

    fn sub_inv(&mut self, z: &mut [C32], g: usize) {
        let m = self.m;
        self.inv.process_with_scratch(&mut z[g * m..(g + 1) * m], &mut self.scratch);
    }

    /// Piece of the real post-pass: X[k], k ≤ B, from the packed transform.
    fn post(&self, z: &[C32], x: &mut [C32], piece: usize) {
        let b = self.b;
        let i = C32::new(0.0, 1.0);
        let (k0, k1) = ((b + 1) * piece / POST_PIECES, (b + 1) * (piece + 1) / POST_PIECES);
        for k in k0..k1 {
            let zk = z[self.perm[k & (b - 1)] as usize];
            let zc = z[self.perm[(b - k) & (b - 1)] as usize].conj();
            x[k] = (zk + zc) * 0.5 - i * self.tw2[k] * (zk - zc) * 0.5;
        }
    }

    /// Piece of the inverse pre-pass: the packed spectrum (× 2, so the result matches realfft's
    /// unnormalised inverse) from X[k], k ≤ B.
    fn pre(&self, x: &[C32], z: &mut [C32], piece: usize) {
        let b = self.b;
        let i = C32::new(0.0, 1.0);
        let (k0, k1) = (b * piece / POST_PIECES, b * (piece + 1) / POST_PIECES);
        for k in k0..k1 {
            let xk = x[k];
            let xc = x[b - k].conj();
            z[self.perm[k] as usize] = (xk + xc) + i * (xk - xc) * self.tw2[k].conj();
        }
    }

    /// Piece of the inverse pass `p` (segments of B >> p). The last one (p = 0) writes only the
    /// second half, the samples overlap-save keeps, unpacked into `out` (B real samples).
    fn ipass(&self, z: &mut [C32], p: usize, piece: usize, out: Option<&mut [f32]>) {
        let len = self.b >> p;
        let h = len / 2;
        let (n0, n1) = (h * piece / PASS_PIECES, h * (piece + 1) / PASS_PIECES);
        match out {
            None => {
                for seg in z.chunks_exact_mut(len) {
                    let (a, b) = seg.split_at_mut(h);
                    for n in n0..n1 {
                        let u = a[n];
                        let v = b[n] * self.tw[n << p].conj();
                        a[n] = u + v;
                        b[n] = u - v;
                    }
                }
            }
            Some(out) => {
                for n in n0..n1 {
                    let v = z[n] - z[n + h] * self.tw[n].conj();
                    out[2 * n] = v.re;
                    out[2 * n + 1] = v.im;
                }
            }
        }
    }

    /// Piece of the packing of input samples [start, start + 2B) from the convolver's ring.
    fn pack_ring(&self, ring: &[f32], start: i64, z: &mut [C32], piece: usize) {
        let (n0, n1) = (self.b * piece / PACK_PIECES, self.b * (piece + 1) / PACK_PIECES);
        let r = |t: i64| if t < 0 { 0.0 } else { ring[(t as usize) & (RING - 1)] };
        for n in n0..n1 {
            let t = start + 2 * n as i64;
            z[n] = C32::new(r(t), r(t + 1));
        }
    }

    /// Forward step `i` (see the struct docs); the input is read from `ring` (None: already packed).
    fn fwd_step(&mut self, z: &mut [C32], dst: &mut [C32], i: usize, ring: Option<(&[f32], i64)>) {
        if i < PACK_PIECES {
            if let Some((ring, start)) = ring {
                self.pack_ring(ring, start, z, i);
            }
            return;
        }
        let i = i - PACK_PIECES;
        let pp = self.passes * PASS_PIECES;
        if i < pp {
            self.pass(z, i / PASS_PIECES, i % PASS_PIECES);
        } else if i < pp + self.r {
            self.sub_fwd(z, i - pp);
        } else {
            self.post(z, dst, i - pp - self.r);
        }
    }

    /// Inverse step `i` (see the struct docs); the last pass writes `out`.
    fn inv_step(&mut self, spec: &[C32], z: &mut [C32], out: &mut [f32], i: usize) {
        if i < POST_PIECES {
            self.pre(spec, z, i);
        } else if i < POST_PIECES + self.r {
            self.sub_inv(z, i - POST_PIECES);
        } else {
            let j = i - POST_PIECES - self.r;
            let pass = self.passes - 1 - j / PASS_PIECES;
            if pass == 0 {
                self.ipass(z, 0, j % PASS_PIECES, Some(out));
            } else {
                self.ipass(z, pass, j % PASS_PIECES, None);
            }
        }
    }

    /// Model cost (flops) of forward step `i`.
    fn fwd_cost(&self, i: usize) -> f32 {
        let (b, m) = (self.b as f32, self.m as f32);
        if i < PACK_PIECES {
            return 4.0 * b / PACK_PIECES as f32;
        }
        let i = i - PACK_PIECES;
        let pp = self.passes * PASS_PIECES;
        if i < pp {
            6.0 * b / PASS_PIECES as f32
        } else if i < pp + self.r {
            5.0 * m * m.log2()
        } else {
            12.0 * b / POST_PIECES as f32
        }
    }

    /// Model cost (flops) of inverse step `i`.
    fn inv_cost(&self, i: usize) -> f32 {
        let (b, m) = (self.b as f32, self.m as f32);
        if i < POST_PIECES {
            12.0 * b / POST_PIECES as f32
        } else if i < POST_PIECES + self.r {
            5.0 * m * m.log2()
        } else {
            6.0 * b / PASS_PIECES as f32
        }
    }
}

/// The transform of a level: whole (realfft) or split.
enum Xform {
    Whole { fwd: Arc<dyn RealToComplex<f32>>, inv: Arc<dyn ComplexToReal<f32>>, tin: Vec<f32>, tspec: Vec<C32>, tout: Vec<f32>, sf: Vec<C32>, si: Vec<C32> },
    Split { s: Split, z: Vec<C32> },
}

/// A voice's state at one level: two kernel slots (current and the one fading out or coming in).
#[derive(Default)]
struct VSlot {
    k: [Option<Arc<Kernel>>; 2],
    active: usize,
    /// weight of `active` (1 = no crossfade)
    fade: f32,
    /// slot latched for a new kernel, active at the next swap
    latched: Option<usize>,
    pending: Option<Option<Arc<Kernel>>>,
}

impl VSlot {
    fn idle(&self) -> bool {
        self.fade >= 1.0 && self.latched.is_none() && self.pending.is_none()
    }
}

/// One step of a cycle (cursor).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Fwd(usize),
    Mac(usize, usize),
    Inv(usize, usize),
    Done,
}

struct Level {
    d: LevelDef,
    /// chunks per block
    n: usize,
    /// work units per cycle
    units: usize,
    depth: usize,
    fdl: Vec<C32>,
    /// FDL slot of the newest spectrum
    head_slot: usize,
    xf: Xform,
    /// per (voice, slot): MAC accumulators, output blocks (current, next) and their validity
    acc: Vec<Vec<C32>>,
    cur: Vec<Vec<f32>>,
    next: Vec<Vec<f32>>,
    valid_cur: Vec<bool>,
    valid_next: Vec<bool>,
    vs: Vec<VSlot>,
    // cycle
    compute: Vec<bool>,
    phase: Phase,
    total: f32,
    done: f32,
    unit: usize,
    started: bool,
    /// first input sample of the cycle's transform
    start: i64,
    /// forward step count
    fsteps: usize,
    /// inverse step count per kernel
    isteps: usize,
}

impl Level {
    fn cost_fwd(&self, i: usize) -> f32 {
        let b = self.d.b as f32;
        match &self.xf {
            Xform::Whole { .. } => 2.5 * 2.0 * b * (2.0 * b).log2(),
            Xform::Split { s, .. } => s.fwd_cost(i),
        }
    }

    fn cost_inv(&self, i: usize) -> f32 {
        let b = self.d.b as f32;
        match &self.xf {
            Xform::Whole { .. } => 2.5 * 2.0 * b * (2.0 * b).log2() + b,
            Xform::Split { s, .. } => s.inv_cost(i),
        }
    }

    fn cost_mac(&self) -> f32 {
        8.0 * (self.d.b + 1) as f32
    }
}

/// A multi-voice zero-latency convolver on one input (see the module docs).
pub struct Convolver {
    voices: usize,
    max_len: usize,
    t: u64,
    pos: usize,
    /// head history: 63 past samples + the current chunk
    xs: Vec<f32>,
    ring: Vec<f32>,
    levels: Vec<Level>,
    /// level 1's output for the current chunk is `levels[0].cur`; the head's slots are level 0's
    fade_step: f32,
    retired: Vec<Arc<Kernel>>,
    queued: Vec<Option<Option<Arc<Kernel>>>>,
    late: u64,
    /// per-chunk cost (model flops) of the last boundaries, for the spike check
    last_cost: f32,
}

impl Convolver {
    /// A convolver for `voices` voices and kernels up to `max_len` taps (the class limit). Allocates.
    pub fn new(voices: usize, max_len: usize, sample_rate: f32) -> Self {
        let mut levels = Vec::new();
        for (l, d) in LEVELS.iter().enumerate() {
            let depth = parts_for(l, max_len.max(1));
            if depth == 0 && l > 0 {
                break;
            }
            let depth = depth.max(1);
            let n = d.b / CHUNK;
            let units = if l == 0 { 1 } else if d.stagger <= 1 { n } else { n + 1 - d.stagger };
            let xf = if d.split == 1 {
                let mut pl = RealFftPlanner::<f32>::new();
                let fwd = pl.plan_fft_forward(2 * d.b);
                let inv = pl.plan_fft_inverse(2 * d.b);
                let sf = fwd.make_scratch_vec();
                let si = inv.make_scratch_vec();
                Xform::Whole { tin: fwd.make_input_vec(), tspec: fwd.make_output_vec(), tout: inv.make_output_vec(), fwd, inv, sf, si }
            } else {
                Xform::Split { s: Split::new(d.b, d.split), z: vec![C32::new(0.0, 0.0); d.b] }
            };
            let slots = voices * 2;
            let (fsteps, isteps) = match &xf {
                Xform::Whole { .. } => (1, 1),
                Xform::Split { s, .. } => (s.fwd_steps(), s.inv_steps()),
            };
            levels.push(Level {
                d: *d,
                n,
                units,
                depth,
                fdl: vec![C32::new(0.0, 0.0); depth * (d.b + 1)],
                head_slot: 0,
                xf,
                acc: (0..slots).map(|_| vec![C32::new(0.0, 0.0); d.b + 1]).collect(),
                cur: (0..slots).map(|_| vec![0.0; d.b]).collect(),
                next: (0..slots).map(|_| vec![0.0; d.b]).collect(),
                valid_cur: vec![false; slots],
                valid_next: vec![false; slots],
                vs: (0..voices).map(|_| VSlot { fade: 1.0, ..Default::default() }).collect(),
                compute: vec![false; slots],
                phase: Phase::Done,
                total: 0.0,
                done: 0.0,
                unit: 0,
                started: false,
                start: 0,
                fsteps,
                isteps,
            });
        }
        let mut c = Self {
            voices,
            max_len,
            t: 0,
            pos: 0,
            xs: vec![0.0; HEAD - 1 + CHUNK],
            ring: vec![0.0; RING],
            levels,
            fade_step: 1.0 / (FADE_S * sample_rate).max(1.0),
            retired: Vec::with_capacity(RETIRED),
            queued: (0..voices).map(|_| None).collect(),
            late: 0,
            last_cost: 0.0,
        };
        c.reset();
        c
    }

    pub fn voices(&self) -> usize {
        self.voices
    }

    /// The class limit (taps).
    pub fn max_len(&self) -> usize {
        self.max_len
    }

    /// Clears the input history, the FDL and every output; keeps the kernels (settled: a pending
    /// request is installed now, crossfades end).
    pub fn reset(&mut self) {
        self.t = 0;
        self.pos = 0;
        self.xs.iter_mut().for_each(|v| *v = 0.0);
        self.ring.iter_mut().for_each(|v| *v = 0.0);
        for v in 0..self.voices {
            let k = match self.queued[v].take() {
                Some(k) => k,
                None => {
                    let vs = &self.levels[0].vs[v];
                    match &vs.pending {
                        Some(p) => p.clone(),
                        None => vs.latched.map_or_else(|| vs.k[vs.active].clone(), |s| vs.k[s].clone()),
                    }
                }
            };
            self.set_now(v, k);
        }
        for lv in self.levels.iter_mut() {
            lv.fdl.iter_mut().for_each(|c| *c = C32::new(0.0, 0.0));
            lv.head_slot = 0;
            lv.cur.iter_mut().chain(lv.next.iter_mut()).for_each(|b| b.iter_mut().for_each(|v| *v = 0.0));
            lv.valid_cur.iter_mut().for_each(|v| *v = false);
            lv.valid_next.iter_mut().for_each(|v| *v = false);
            lv.phase = Phase::Done;
            lv.started = false;
            lv.unit = 0;
        }
        self.late = 0;
    }

    fn retire(&mut self, k: Option<Arc<Kernel>>) {
        if let Some(k) = k {
            if self.retired.len() < RETIRED {
                self.retired.push(k);
            } else {
                // never free on the audio thread: leak rather (the caller drains every block)
                std::mem::forget(k);
            }
        }
    }

    /// Installs a kernel for `voice` at once, in every level, without a crossfade (initialisation;
    /// the outputs start from the current input history). Real-time safe.
    pub fn set_now(&mut self, voice: usize, k: Option<Arc<Kernel>>) {
        let mut old = [None, None, None, None, None, None, None, None, None, None, None, None, None, None, None, None];
        let mut no = 0;
        for lv in self.levels.iter_mut() {
            let vs = &mut lv.vs[voice];
            for s in 0..2 {
                if let Some(x) = vs.k[s].take() {
                    if no < old.len() {
                        old[no] = Some(x);
                        no += 1;
                    }
                }
            }
            if let Some(Some(p)) = vs.pending.take() {
                if no < old.len() {
                    old[no] = Some(p);
                    no += 1;
                }
            }
            vs.k[0] = k.clone();
            vs.active = 0;
            vs.fade = 1.0;
            vs.latched = None;
            for s in 0..2 {
                lv.valid_cur[voice * 2 + s] = false;
                lv.valid_next[voice * 2 + s] = false;
            }
        }
        for o in old.iter_mut().take(no) {
            let x = o.take();
            self.retire(x);
        }
        let q = self.queued[voice].take();
        if let Some(q) = q {
            self.retire(q);
        }
        // every level holds a clone: this drop never frees
        drop(k);
    }

    /// Requests a kernel (or silence) for `voice`. Taken at the next chunk boundary when the voice is
    /// not crossfading, else queued (only the latest request is kept). Real-time safe.
    pub fn set_kernel(&mut self, voice: usize, k: Option<Arc<Kernel>>) {
        if let Some(Some(old)) = self.queued[voice].replace(k) {
            self.retire(Some(old));
        }
    }

    /// The kernel the voice plays or is moving to (the latest request).
    pub fn kernel(&self, voice: usize) -> Option<&Arc<Kernel>> {
        if let Some(q) = &self.queued[voice] {
            return q.as_ref();
        }
        let vs = &self.levels[0].vs[voice];
        vs.k[vs.active].as_ref()
    }

    /// True while a request is waiting or any level of the voice is still crossfading.
    pub fn busy(&self, voice: usize) -> bool {
        self.queued[voice].is_some() || self.levels.iter().any(|l| !l.vs[voice].idle())
    }

    /// A kernel the convolver no longer uses, for the caller to release off the audio thread.
    pub fn take_retired(&mut self) -> Option<Arc<Kernel>> {
        self.retired.pop()
    }

    /// Times a level's block was needed before its cycle finished (always 0: the schedule's proof).
    pub fn late_count(&self) -> u64 {
        self.late
    }

    /// Model cost (flops) of the last chunk boundary's work.
    pub fn last_boundary_cost(&self) -> f32 {
        self.last_cost
    }

    /// Convolves `input`; `out[v]` receives voice v's output (overwritten). Every slice has the
    /// input's length.
    pub fn process(&mut self, input: &[f32], out: &mut [&mut [f32]]) {
        let n = input.len();
        let mut i = 0;
        while i < n {
            let take = (CHUNK - self.pos).min(n - i);
            let p0 = self.pos;
            for k in 0..take {
                let x = input[i + k];
                self.xs[HEAD - 1 + p0 + k] = x;
                self.ring[(self.t as usize + k) & (RING - 1)] = x;
            }
            for v in 0..self.voices {
                self.render(v, p0, take, &mut out[v][i..i + take]);
            }
            self.pos += take;
            self.t += take as u64;
            i += take;
            if self.pos == CHUNK {
                self.pos = 0;
                let (a, b) = self.xs.split_at_mut(CHUNK);
                a[..HEAD - 1].copy_from_slice(&b[..HEAD - 1]);
                self.boundary();
            }
        }
    }

    /// Voice `v`'s output for samples [p0, p0 + len) of the current chunk.
    fn render(&mut self, v: usize, p0: usize, len: usize, out: &mut [f32]) {
        let step = self.fade_step;
        let t0 = self.t;
        // head + level 1 (level 0's slots)
        {
            let lv = &mut self.levels[0];
            let vs = &mut lv.vs[v];
            let (a, o) = (vs.active, 1 - vs.active);
            let ka = vs.k[a].as_deref();
            let ko = if vs.fade < 1.0 { vs.k[o].as_deref() } else { None };
            let la = if lv.valid_cur[2 * v + a] { Some(&lv.cur[2 * v + a][..]) } else { None };
            let lo = if ko.is_some() && lv.valid_cur[2 * v + o] { Some(&lv.cur[2 * v + o][..]) } else { None };
            let mut fade = vs.fade;
            for k in 0..len {
                let p = p0 + k;
                let mut ya = match ka {
                    Some(kk) if kk.head_nz => dot64(&kk.head_rev, &self.xs[p..p + HEAD]),
                    _ => 0.0,
                };
                if let Some(l) = la {
                    ya += l[p];
                }
                if fade < 1.0 {
                    let mut yo = match ko {
                        Some(kk) if kk.head_nz => dot64(&kk.head_rev, &self.xs[p..p + HEAD]),
                        _ => 0.0,
                    };
                    if let Some(l) = lo {
                        yo += l[p];
                    }
                    out[k] = fade * ya + (1.0 - fade) * yo;
                    fade = (fade + step).min(1.0);
                } else {
                    out[k] = ya;
                }
            }
            vs.fade = fade;
        }
        // levels 2 …
        for lv in self.levels.iter_mut().skip(1) {
            let b = lv.d.b;
            let vs = &mut lv.vs[v];
            let (a, o) = (vs.active, 1 - vs.active);
            let off = (t0 as usize) & (b - 1);
            let ca = if lv.valid_cur[2 * v + a] && vs.k[a].is_some() { Some(&lv.cur[2 * v + a][off..off + len]) } else { None };
            if vs.fade >= 1.0 {
                if let Some(c) = ca {
                    for (y, x) in out.iter_mut().zip(c) {
                        *y += x;
                    }
                }
                continue;
            }
            let co = if lv.valid_cur[2 * v + o] && vs.k[o].is_some() { Some(&lv.cur[2 * v + o][off..off + len]) } else { None };
            let mut fade = vs.fade;
            for k in 0..len {
                let ya = ca.map_or(0.0, |c| c[k]);
                let yo = co.map_or(0.0, |c| c[k]);
                out[k] += if fade < 1.0 { fade * ya + (1.0 - fade) * yo } else { ya };
                fade = (fade + step).min(1.0);
            }
            vs.fade = fade;
        }
    }

    /// The work of a chunk boundary (`self.t` is the first sample of the coming chunk).
    fn boundary(&mut self) {
        let bnd = (self.t / CHUNK as u64) as usize; // chunks completed
        let mut cost = 0.0f32;
        // requests: a voice idle at every level takes its queued kernel now
        for v in 0..self.voices {
            if self.queued[v].is_some() && self.levels.iter().all(|l| l.vs[v].idle()) {
                let k = self.queued[v].take().unwrap();
                for lv in self.levels.iter_mut() {
                    lv.vs[v].pending = Some(k.clone());
                }
                // every level holds a clone: this drop never frees
                drop(k);
            }
        }
        for l in 0..self.levels.len() {
            if l == 0 {
                cost += self.level1(bnd);
                continue;
            }
            if self.levels[l].d.stagger == 0 {
                self.swap(l, bnd);
                cost += self.work(l, bnd);
            } else {
                cost += self.work(l, bnd);
                self.swap(l, bnd);
            }
        }
        self.last_cost = cost;
    }

    /// Level 1 (and the head's slots): latch, then the whole cycle at once.
    fn level1(&mut self, bnd: usize) -> f32 {
        self.latch(0);
        self.start_cycle(0, bnd as u64 - 1);
        let mut c = 0.0;
        while self.levels[0].phase != Phase::Done {
            c += self.step(0);
        }
        let lv = &mut self.levels[0];
        std::mem::swap(&mut lv.cur, &mut lv.next);
        std::mem::swap(&mut lv.valid_cur, &mut lv.valid_next);
        for v in 0..self.voices {
            let vs = &mut lv.vs[v];
            if let Some(s) = vs.latched.take() {
                vs.active = s;
                vs.fade = 0.0;
            }
        }
        c
    }

    /// Takes pending kernels into free slots at a cycle start, retires finished ones.
    fn latch(&mut self, l: usize) {
        let mut out: [Option<Arc<Kernel>>; 8] = Default::default();
        let mut no = 0;
        let lv = &mut self.levels[l];
        for v in 0..lv.vs.len() {
            let vs = &mut lv.vs[v];
            let o = 1 - vs.active;
            if vs.fade >= 1.0 && vs.latched.is_none() {
                if let Some(k) = vs.k[o].take() {
                    if no < out.len() {
                        out[no] = Some(k);
                        no += 1;
                    }
                }
            }
            if vs.fade >= 1.0 && vs.latched.is_none() {
                if let Some(p) = vs.pending.take() {
                    vs.k[o] = p;
                    vs.latched = Some(o);
                }
            }
        }
        for x in out.iter_mut().take(no) {
            let k = x.take();
            self.retire(k);
        }
    }

    /// Sets the compute mask and the cycle's total cost for input block `m`.
    fn start_cycle(&mut self, l: usize, m: u64) {
        let lv = &mut self.levels[l];
        let mut total = 0.0;
        for i in 0..lv.fsteps {
            total += lv.cost_fwd(i);
        }
        let mac = lv.cost_mac();
        let mut inv = 0.0;
        for i in 0..lv.isteps {
            inv += lv.cost_inv(i);
        }
        for v in 0..lv.vs.len() {
            let vs = &lv.vs[v];
            for s in 0..2 {
                let use_it = match &vs.k[s] {
                    Some(k) => (s == vs.active || vs.fade < 1.0 || vs.latched == Some(s)) && !k.parts[l].is_empty(),
                    None => false,
                };
                lv.compute[2 * v + s] = use_it;
                lv.valid_next[2 * v + s] = false;
                if use_it {
                    let k = vs.k[s].as_ref().unwrap();
                    let np = k.parts[l].iter().filter(|(q, _)| *q < lv.depth).count();
                    total += np as f32 * mac + inv;
                }
            }
        }
        // the FDL slot of block m
        lv.head_slot = (m as usize) % lv.depth;
        // input [(m − 1)B, (m + 1)B) → transform input
        let b = lv.d.b;
        let start = (m as i64 - 1) * b as i64;
        match &mut lv.xf {
            Xform::Whole { tin, .. } => {
                for (k, x) in tin.iter_mut().enumerate() {
                    let t = start + k as i64;
                    *x = if t < 0 { 0.0 } else { self.ring[(t as usize) & (RING - 1)] };
                }
            }
            // packed from the ring by the first forward steps
            Xform::Split { .. } => {}
        }
        lv.start = start;
        lv.phase = Phase::Fwd(0);
        lv.total = total;
        lv.done = 0.0;
        lv.unit = 0;
        lv.started = true;
    }

    /// One step of level `l`'s cycle; returns its model cost.
    fn step(&mut self, l: usize) -> f32 {
        let ring = &self.ring[..];
        let lv = &mut self.levels[l];
        let start = lv.start;
        let b = lv.d.b;
        let slots = lv.compute.len();
        match lv.phase {
            Phase::Fwd(i) => {
                let c = lv.cost_fwd(i);
                let hs = lv.head_slot;
                let dst = &mut lv.fdl[hs * (b + 1)..(hs + 1) * (b + 1)];
                match &mut lv.xf {
                    Xform::Whole { fwd, tin, sf, .. } => {
                        fwd.process_with_scratch(tin, dst, sf).expect("fft sizes");
                    }
                    Xform::Split { s, z } => s.fwd_step(z, dst, i, Some((ring, start))),
                }
                lv.phase = if i + 1 < lv.fsteps { Phase::Fwd(i + 1) } else { next_mac(lv, 0, slots) };
                lv.done += c;
                c
            }
            Phase::Mac(slot, j) => {
                let c = lv.cost_mac();
                let (v, s) = (slot / 2, slot % 2);
                let k = lv.vs[v].k[s].as_ref().unwrap();
                let (q, h) = &k.parts[l][j];
                let acc = &mut lv.acc[slot];
                if j == 0 {
                    acc.iter_mut().for_each(|c| *c = C32::new(0.0, 0.0));
                }
                if *q < lv.depth {
                    let fs = (lv.head_slot + lv.depth - q) % lv.depth;
                    let f = &lv.fdl[fs * (b + 1)..(fs + 1) * (b + 1)];
                    cmac(acc, f, h);
                }
                let np = k.parts[l].len();
                lv.phase = if j + 1 < np { Phase::Mac(slot, j + 1) } else { next_mac(lv, slot + 1, slots) };
                lv.done += c;
                c
            }
            Phase::Inv(slot, i) => {
                let c = lv.cost_inv(i);
                let isteps = lv.isteps;
                let next = &mut lv.next[slot];
                let acc = &mut lv.acc[slot];
                match &mut lv.xf {
                    Xform::Whole { inv, tspec, tout, si, .. } => {
                        tspec.copy_from_slice(acc);
                        // bins 0 and B carry no imaginary part for real kernels; force it (rounding)
                        tspec[0].im = 0.0;
                        tspec[b].im = 0.0;
                        inv.process_with_scratch(tspec, tout, si).expect("fft sizes");
                        next.copy_from_slice(&tout[b..]);
                    }
                    Xform::Split { s, z } => s.inv_step(acc, z, next, i),
                }
                if i + 1 < isteps {
                    lv.phase = Phase::Inv(slot, i + 1);
                } else {
                    lv.valid_next[slot] = true;
                    lv.phase = next_inv(lv, slot + 1, slots);
                }
                lv.done += c;
                c
            }
            Phase::Done => 0.0,
        }
    }

    /// Level `l`'s share of work at boundary `bnd`.
    fn work(&mut self, l: usize, bnd: usize) -> f32 {
        let (n, s, units) = (self.levels[l].n, self.levels[l].d.stagger, self.levels[l].units);
        if bnd < s {
            return 0.0;
        }
        let j = (bnd - s) % n;
        let mut c = 0.0;
        if j == 0 && bnd - s >= n {
            // block m completed at boundary bnd − s
            let m = ((bnd - s) / n - 1) as u64;
            self.latch(l);
            self.start_cycle(l, m);
        }
        if !self.levels[l].started || j >= units {
            return 0.0;
        }
        let target = if j + 1 >= units { f32::INFINITY } else { self.levels[l].total * (j + 1) as f32 / units as f32 };
        loop {
            let lv = &self.levels[l];
            if lv.phase == Phase::Done {
                break;
            }
            let nc = match lv.phase {
                Phase::Fwd(i) => lv.cost_fwd(i),
                Phase::Mac(..) => lv.cost_mac(),
                Phase::Inv(_, i) => lv.cost_inv(i),
                Phase::Done => 0.0,
            };
            if lv.done + 0.5 * nc > target {
                break;
            }
            c += self.step(l);
        }
        c
    }

    /// At a block boundary of level `l`, the finished cycle's output becomes current.
    fn swap(&mut self, l: usize, bnd: usize) {
        let n = self.levels[l].n;
        if !bnd.is_multiple_of(n) || bnd < 2 * n || !self.levels[l].started {
            return;
        }
        if self.levels[l].phase != Phase::Done {
            self.late += 1;
            while self.levels[l].phase != Phase::Done {
                self.step(l);
            }
        }
        let lv = &mut self.levels[l];
        std::mem::swap(&mut lv.cur, &mut lv.next);
        std::mem::swap(&mut lv.valid_cur, &mut lv.valid_next);
        lv.valid_next.iter_mut().for_each(|v| *v = false);
        lv.started = false;
        for vs in lv.vs.iter_mut() {
            if let Some(s) = vs.latched.take() {
                vs.active = s;
                vs.fade = 0.0;
            }
        }
    }
}

/// The next slot's MACs, else the inverse transforms from the first computed slot.
fn next_mac(lv: &Level, from: usize, slots: usize) -> Phase {
    for slot in from..slots {
        if lv.compute[slot] {
            return Phase::Mac(slot, 0);
        }
    }
    next_inv(lv, 0, slots)
}

fn next_inv(lv: &Level, from: usize, slots: usize) -> Phase {
    for slot in from..slots {
        if lv.compute[slot] {
            return Phase::Inv(slot, 0);
        }
    }
    Phase::Done
}

/// acc += f · h (complex, per bin).
#[inline]
fn cmac(acc: &mut [C32], f: &[C32], h: &[C32]) {
    for ((a, x), y) in acc.iter_mut().zip(f).zip(h) {
        a.re += x.re * y.re - x.im * y.im;
        a.im += x.re * y.im + x.im * y.re;
    }
}

/// Σ h[j] · x[j] over 64 taps, 8 lanes (the same order at every block size).
#[inline]
fn dot64(h: &[f32; HEAD], x: &[f32]) -> f32 {
    let x = &x[..HEAD];
    let mut acc = [0.0f32; 8];
    for j in 0..HEAD / 8 {
        for l in 0..8 {
            acc[l] += h[8 * j + l] * x[8 * j + l];
        }
    }
    ((acc[0] + acc[1]) + (acc[2] + acc[3])) + ((acc[4] + acc[5]) + (acc[6] + acc[7]))
}

/// The split transform alone, for tests and the bench: forward (2B real → B + 1 bins) and inverse
/// (B + 1 bins → the last B of 2B real samples, scaled as realfft's unnormalised inverse).
pub struct SplitTransform {
    s: Split,
    z: Vec<C32>,
}

impl SplitTransform {
    pub fn new(b: usize, r: usize) -> Self {
        Self { s: Split::new(b, r), z: vec![C32::new(0.0, 0.0); b] }
    }

    pub fn forward(&mut self, x: &[f32], out: &mut [Complex<f32>]) {
        self.s.pack(x, &mut self.z);
        for i in 0..self.s.fwd_steps() {
            let z = &mut self.z;
            self.s.fwd_step(z, out, i, None);
        }
    }

    pub fn inverse_tail(&mut self, spec: &[Complex<f32>], out: &mut [f32]) {
        for i in 0..self.s.inv_steps() {
            let z = &mut self.z;
            self.s.inv_step(spec, z, out, i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rng(seed: u64) -> impl FnMut() -> f32 {
        let mut s = seed;
        move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            ((s >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
        }
    }

    fn direct(x: &[f32], h: &[f32]) -> Vec<f64> {
        let mut y = vec![0.0f64; x.len()];
        for (n, yn) in y.iter_mut().enumerate() {
            let mut s = 0.0f64;
            for (k, &hk) in h.iter().enumerate().take(n + 1) {
                s += hk as f64 * x[n - k] as f64;
            }
            *yn = s;
        }
        y
    }

    /// FFT-based f64 reference (fast enough for long IRs).
    fn fft_ref(x: &[f32], h: &[f32]) -> Vec<f64> {
        let n = (x.len() + h.len()).next_power_of_two();
        let mut pl = RealFftPlanner::<f64>::new();
        let f = pl.plan_fft_forward(n);
        let i = pl.plan_fft_inverse(n);
        let mut a = vec![0.0; n];
        let mut b = vec![0.0; n];
        for (d, s) in a.iter_mut().zip(x) {
            *d = *s as f64;
        }
        for (d, s) in b.iter_mut().zip(h) {
            *d = *s as f64;
        }
        let mut fa = f.make_output_vec();
        let mut fb = f.make_output_vec();
        f.process(&mut a, &mut fa).unwrap();
        f.process(&mut b, &mut fb).unwrap();
        for (p, q) in fa.iter_mut().zip(&fb) {
            *p *= *q / n as f64;
        }
        let mut y = vec![0.0; n];
        i.process(&mut fa, &mut y).unwrap();
        y.truncate(x.len());
        y
    }

    fn err_db(y: &[f32], r: &[f64]) -> f64 {
        let e: f64 = y.iter().zip(r).map(|(a, b)| (*a as f64 - b).powi(2)).sum();
        let p: f64 = r.iter().map(|b| b * b).sum();
        10.0 * (e / p.max(1e-300)).log10()
    }

    fn run(c: &mut Convolver, x: &[f32], blocks: &mut dyn FnMut() -> usize) -> Vec<Vec<f32>> {
        let nv = c.voices();
        let mut out = vec![vec![0.0f32; x.len()]; nv];
        let mut i = 0;
        while i < x.len() {
            let b = blocks().min(x.len() - i).max(1);
            let mut sl: Vec<&mut [f32]> = out.iter_mut().map(|o| &mut o[i..i + b]).collect();
            c.process(&x[i..i + b], &mut sl);
            i += b;
        }
        out
    }

    fn decaying(n: usize, seed: u64, tau: f32) -> Vec<f32> {
        let mut r = rng(seed);
        (0..n).map(|k| r() * (-(k as f32) / tau).exp()).collect()
    }

    #[test]
    fn dirac_is_exact() {
        let mut h = vec![0.0f32; 37 + 1];
        h[0] = 1.0;
        let mut c = Convolver::new(1, 96000, 48000.0);
        c.set_now(0, Some(Arc::new(Kernel::new(&h[..1]))));
        let mut r = rng(3);
        let x: Vec<f32> = (0..20000).map(|_| r()).collect();
        let y = run(&mut c, &x, &mut || 333);
        assert_eq!(y[0], x);
        // δ delayed by 37
        h[0] = 0.0;
        h[37] = 1.0;
        let mut c = Convolver::new(1, 96000, 48000.0);
        c.set_now(0, Some(Arc::new(Kernel::new(&h))));
        let y = run(&mut c, &x, &mut || 1000);
        for n in 0..x.len() {
            assert_eq!(y[0][n], if n >= 37 { x[n - 37] } else { 0.0 });
        }
    }

    #[test]
    fn matches_direct_convolution() {
        // lengths that end in every level, and an IR with a hole
        for (len, seed) in [(300usize, 1u64), (2000, 2), (9000, 3), (40000, 4), (70000, 5)] {
            let h = decaying(len, seed, len as f32 / 3.0);
            let mut r = rng(9);
            let x: Vec<f32> = (0..len + 50000).map(|_| r()).collect();
            let reference = fft_ref(&x, &h);
            let mut c = Convolver::new(1, 96000, 48000.0);
            c.set_now(0, Some(Arc::new(Kernel::new(&h))));
            let y = run(&mut c, &x, &mut || 512);
            let e = err_db(&y[0], &reference);
            eprintln!("convolve: len {len}: error {e:.1} dB re output");
            assert!(e < -110.0, "len {len}: error {e:.1} dB");
            assert_eq!(c.late_count(), 0);
        }
        let h: Vec<f32> = (0..300).map(|k| if k < 64 { 0.5 } else if k > 200 { -0.25 } else { 0.0 }).collect();
        let mut r = rng(11);
        let x: Vec<f32> = (0..5000).map(|_| r()).collect();
        let mut c = Convolver::new(1, 2000, 48000.0);
        c.set_now(0, Some(Arc::new(Kernel::new(&h))));
        let y = run(&mut c, &x, &mut || 64);
        assert!(err_db(&y[0], &direct(&x, &h)) < -120.0);
    }

    #[test]
    fn block_size_independent_with_two_voices_and_a_swap() {
        let ha = Arc::new(Kernel::new(&decaying(20000, 1, 5000.0)));
        let hb = Arc::new(Kernel::new(&decaying(40000, 2, 9000.0)));
        let hc = Arc::new(Kernel::new(&decaying(3000, 3, 900.0)));
        let mut r = rng(5);
        let x: Vec<f32> = (0..120000).map(|_| r()).collect();
        let render = |blocks: &mut dyn FnMut() -> usize| {
            let mut c = Convolver::new(2, 96000, 48000.0);
            c.set_now(0, Some(ha.clone()));
            c.set_now(1, Some(hb.clone()));
            let mut out = vec![vec![0.0f32; x.len()]; 2];
            let mut i = 0;
            let mut swapped = false;
            while i < x.len() {
                // the swap is requested at sample 50,000 (block start ≤ it): taken at the next chunk
                let b = blocks().min(x.len() - i).max(1);
                if !swapped && i + b > 50000 {
                    let first = 50000 - i;
                    if first > 0 {
                        let mut sl: Vec<&mut [f32]> = out.iter_mut().map(|o| &mut o[i..i + first]).collect();
                        c.process(&x[i..i + first], &mut sl);
                        i += first;
                    }
                    c.set_kernel(0, Some(hc.clone()));
                    swapped = true;
                    continue;
                }
                let mut sl: Vec<&mut [f32]> = out.iter_mut().map(|o| &mut o[i..i + b]).collect();
                c.process(&x[i..i + b], &mut sl);
                i += b;
            }
            while c.take_retired().is_some() {}
            out
        };
        let reference = render(&mut || 64);
        for bs in [1usize, 7, 1000, 4096] {
            assert!(render(&mut || bs) == reference, "block {bs}");
        }
        let mut s = rng(77);
        assert!(render(&mut || 1 + ((s() + 1.0) * 700.0) as usize) == reference, "random blocks");
    }

    #[test]
    fn no_cold_start_and_per_level_crossfades() {
        // a short kernel, then a long one: once every level has crossfaded, the output is the long
        // kernel's full convolution of the whole input history
        let short = decaying(4800, 4, 1000.0);
        let long = decaying(96000, 6, 20000.0);
        let mut r = rng(8);
        let x: Vec<f32> = (0..200000).map(|_| r()).collect();
        let reference = fft_ref(&x, &long);
        let mut c = Convolver::new(1, 96000, 48000.0);
        c.set_now(0, Some(Arc::new(Kernel::new(&short))));
        let mut y = vec![0.0f32; x.len()];
        let at = 60000;
        c.process(&x[..at], &mut [&mut y[..at]]);
        c.set_kernel(0, Some(Arc::new(Kernel::new(&long))));
        c.process(&x[at..], &mut [&mut y[at..]]);
        // the last level (8,192) is new within 2B, plus the 30 ms fade
        let settled = at + 2 * 8192 + 64 + (0.031 * 48000.0) as usize;
        let e = err_db(&y[settled..], &reference[settled..]);
        assert!(e < -110.0, "after the swap: {e:.1} dB");
        assert!(!c.busy(0));
        assert_eq!(c.take_retired().map(|k| k.len()), Some(4800));
    }

    #[test]
    fn crossfade_starts_per_level() {
        // impulse-probe: a kernel with one tap per level; after a swap to its negative, each tap flips
        // within its level's bound and every fade lasts 30 ms
        let sr = 48000.0;
        let taps = [10usize, 100, 1000, 4000, 20000, 40000];
        let mut h = vec![0.0f32; 40001];
        for &t in &taps {
            h[t] = 1.0;
        }
        let neg: Vec<f32> = h.iter().map(|v| -v).collect();
        let mut c = Convolver::new(1, 96000, sr);
        c.set_now(0, Some(Arc::new(Kernel::new(&h))));
        // constant input: each tap contributes 1 once it is past
        let n = 120000;
        let x = vec![1.0f32; n];
        let mut y = vec![0.0f32; n];
        let at = 50000;
        c.process(&x[..at], &mut [&mut y[..at]]);
        c.set_kernel(0, Some(Arc::new(Kernel::new(&neg))));
        c.process(&x[at..], &mut [&mut y[at..]]);
        assert!((y[at - 1] - 6.0).abs() < 1e-3);
        assert!((y[n - 1] + 6.0).abs() < 1e-3);
        // the whole switch ends by 2 · 8,192 + one fade after the request
        let end = at + 2 * 8192 + 64 + (FADE_S * sr) as usize + 64;
        assert!((y[end] + 6.0).abs() < 1e-3, "{}", y[end]);
        // the head and level 1 start within one chunk
        assert!(y[at + 64 + 10] < 6.0);
    }

    #[test]
    fn split_transform_matches_whole() {
        for (b, r) in [(4096usize, 4usize), (8192, 8)] {
            let mut rr = rng(b as u64);
            let x: Vec<f32> = (0..2 * b).map(|_| rr()).collect();
            let mut pl = RealFftPlanner::<f32>::new();
            let f = pl.plan_fft_forward(2 * b);
            let mut xin = x.clone();
            let mut whole = f.make_output_vec();
            f.process(&mut xin, &mut whole).unwrap();
            let mut st = SplitTransform::new(b, r);
            let mut split = vec![C32::new(0.0, 0.0); b + 1];
            st.forward(&x, &mut split);
            let e: f64 = whole.iter().zip(&split).map(|(a, c)| ((a - c).norm() as f64).powi(2)).sum();
            let p: f64 = whole.iter().map(|a| (a.norm() as f64).powi(2)).sum();
            assert!(10.0 * (e / p).log10() < -130.0, "forward {b}: {:.1}", 10.0 * (e / p).log10());
            // inverse: realfft's unnormalised inverse, last B samples = 2B · x
            let mut tail = vec![0.0f32; b];
            st.inverse_tail(&split, &mut tail);
            let e: f64 = tail.iter().zip(&x[b..]).map(|(a, c)| (*a as f64 / (2 * b) as f64 - *c as f64).powi(2)).sum();
            let p: f64 = x[b..].iter().map(|c| (*c as f64).powi(2)).sum();
            assert!(10.0 * (e / p).log10() < -130.0, "inverse {b}: {:.1}", 10.0 * (e / p).log10());
        }
    }

    #[test]
    fn worst_chunk_cost_is_bounded() {
        // the model cost of every boundary over 2 s, a 2 s kernel at 48 kHz: worst ≤ 4× the mean
        let sr = 48000.0;
        let h = decaying(96000, 3, 20000.0);
        let mut c = Convolver::new(1, 96000, sr);
        c.set_now(0, Some(Arc::new(Kernel::new(&h))));
        let x = vec![0.1f32; 64];
        let mut y = vec![0.0f32; 64];
        let (mut sum, mut worst, mut n) = (0.0f64, 0.0f32, 0);
        for k in 0..3000 {
            c.process(&x, &mut [&mut y]);
            if k > 300 {
                let cst = c.last_boundary_cost();
                sum += cst as f64;
                worst = worst.max(cst);
                n += 1;
            }
        }
        let mean = sum / n as f64;
        eprintln!("convolve: worst chunk {worst:.0} flop, mean {mean:.0} ({:.2}×)", worst as f64 / mean);
        assert!((worst as f64) <= 4.0 * mean, "worst {worst} vs mean {mean:.0}");
    }

    /// Timed chunks (not a check: `cargo test -p gfx-dsp --release -- --ignored --nocapture timed`).
    #[test]
    #[ignore]
    fn timed_chunks() {
        for (sr, secs, voices) in [(48000.0f32, 0.2f32, 1usize), (48000.0, 2.0, 1), (192000.0, 0.2, 2), (192000.0, 2.0, 1)] {
            let len = (sr * secs) as usize;
            let h = Arc::new(Kernel::new(&decaying(len, 3, len as f32 / 5.0)));
            let mut c = Convolver::new(voices, (2.0 * sr) as usize, sr);
            for v in 0..voices {
                c.set_now(v, Some(h.clone()));
            }
            let x = vec![0.1f32; 64];
            let mut ys = vec![vec![0.0f32; 64]; voices];
            let mut t = Vec::new();
            for k in 0..20000 {
                let mut sl: Vec<&mut [f32]> = ys.iter_mut().map(|y| &mut y[..]).collect();
                let s = std::time::Instant::now();
                c.process(&x, &mut sl);
                let e = s.elapsed().as_secs_f64();
                if k > 2000 {
                    t.push((e, k + 1));
                }
            }
            let mean = t.iter().map(|p| p.0).sum::<f64>() / t.len() as f64;
            t.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
            let top: Vec<String> = t[..8].iter().map(|(e, b)| format!("{:.1}µs@{}", e * 1e6, b % 128)).collect();
            let med = t[t.len() / 2].0;
            eprintln!("timed {sr} {secs}s ×{voices}: mean {:.2} µs, median {:.2} µs, p99.9 {:.1} µs; top {}", mean * 1e6, med * 1e6, t[t.len() / 1000].0 * 1e6, top.join(" "));
        }
    }
}
