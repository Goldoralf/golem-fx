// SPDX-License-Identifier: GPL-3.0-or-later
//! The spectral engine (README §2.2 – 2.7): one window size, two channels, up to two heads.
//!
//! A frame starts every hop H = N / 4. Its work is a list of **passes** (read and window, FFT, the
//! real-spectrum twiddle, regions, map, mix, modules, phases, the inverse, the overlap-add), each cut
//! into units of at most [`UNIT`] elements, and the units are spread evenly over the hop on sample
//! counts (gfx-dsp `stft::due`). The output is identical for any host block size, and no pass of
//! O(N) runs in one piece. Frame b's output is overlap-added from b + H on.
//!
//! Per head and channel, the analysis keeps the magnitudes (`a`), and in coherent mode the phases
//! (`b`), the instantaneous frequencies (`c`, from a second frame H / 4 earlier) and the output
//! phases (`q`). A frozen head keeps them and skips its reading; a coherent frozen head still
//! advances its peaks' phases.
//!
//! - **Random mode** (`cloud` 100 %): power mixed over the heads, |Y|² = Σ w_h |X_h|², then fshift and
//!   pitch (one power-preserving remap), spread (the constant-Q blur), a fresh phase per bin.
//! - **Coherent mode** (`cloud` < 100 %): each head's peak regions move as rigid units (pitch,
//!   fshift), their peaks' phases advance by ω · H from the head's own accumulators, and the heads
//!   are summed as complex spectra, √w_h · Y_h; spread rescales the magnitudes; the `cloud` offset
//!   and `width` follow. At `amount` ×1 with neutral modules the analysis phases are used as they are
//!   (identity).

use crate::memory::{Pool, Sample, Src};
use gfx_dsp::stft::{self, as_complex, as_complex_ref, chunk, chunks, Phasors, Plan, RegionSweep, Tables, XorShift, C32, MAX_N, UNIT};
use std::f32::consts::TAU;

pub const HEADS: usize = 2;
const MAX_K: usize = MAX_N / 2 + 1;

use stft::{fast_atan2, fast_sin_cos, wrap_pi as wrap};

/// Bins per unit of the passes with a transcendental per bin (coherent analysis, map, phases).
const HEAVY: usize = 512;

#[inline]
fn hchunks(len: usize) -> usize {
    len.div_ceil(HEAVY)
}

#[inline]
fn hchunk(len: usize, u: usize) -> (usize, usize) {
    ((u * HEAVY).min(len), ((u + 1) * HEAVY).min(len))
}

/// One head as a frame sees it (latched at the frame start).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeadSnap {
    pub active: bool,
    pub gen: u32,
    /// the frame's centre
    pub pos: f64,
    pub src: Src,
    /// mix weight (power): the heads' crossfade on the frame grid
    pub weight: f32,
    /// frozen (`freeze`, or held while its buffer records)
    pub frozen: bool,
    /// L = R over the frame (mono DI on a 2 / 2 lane): analysed once
    pub mono: bool,
}

/// Everything a frame needs, latched at its start.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameSet {
    pub coherent: bool,
    /// 0 – 1
    pub cloud: f32,
    /// 0 – 1
    pub width: f32,
    /// pitch ratio 2^(st / 12)
    pub ratio: f32,
    pub shift_hz: f32,
    /// blur σ in cents (0 = off)
    pub sigma: f32,
    /// compress exponent 0 – 1
    pub comp: f32,
    /// `amount` ×1, pitch and fshift at 0: identity phases
    pub identity: bool,
    pub heads: [HeadSnap; HEADS],
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Zero,
    Read2,
    Read1,
    Fft,
    Post2,
    Post1,
    Copy,
    RegF,
    RegB,
    Map,
    Mix,
    Remap,
    Coh2P,
    Blur(u8),
    Syn,
    Pack,
    IFft,
    Ola,
}

#[derive(Clone, Copy, Debug)]
struct Pass {
    kind: Kind,
    ch: u8,
    head: u8,
    units: u32,
}

const NO_PASS: Pass = Pass { kind: Kind::Zero, ch: 0, head: 0, units: 0 };
const MAX_PASSES: usize = 72;

struct HeadArr {
    a: Vec<f32>,
    b: Vec<f32>,
    c: Vec<f32>,
    q: Vec<f32>,
    /// Σ over the full spectrum of |X|² (the region's level, for `compress`)
    energy: f32,
    amax: f32,
}

impl HeadArr {
    fn new() -> Self {
        Self { a: crate::memory::touched(MAX_K, 0.0), b: crate::memory::touched(MAX_K, 0.0), c: crate::memory::touched(MAX_K, 0.0), q: crate::memory::touched(MAX_K, 0.0), energy: 0.0, amax: 0.0 }
    }
}

/// The map's current region (resumable across units).
#[derive(Clone, Copy, Debug)]
struct MapState {
    owner: u32,
    skip: bool,
    shift: f32,
    pk: f32,
    phi: f32,
}

impl Default for MapState {
    fn default() -> Self {
        Self { owner: u32::MAX, skip: true, shift: 0.0, pk: 0.0, phi: 0.0 }
    }
}

struct Chan {
    /// time frame / complex FFT buffer; between the passes also the regions' owner (bins) and best
    /// (bins after it), then the coherent spread's power
    buf: Vec<f32>,
    ola: Vec<f32>,
    mr: Vec<f32>,
    mi: Vec<f32>,
    h: [HeadArr; HEADS],
    shared: XorShift,
    own: XorShift,
    sweep: RegionSweep,
    blur_s: f32,
    map: MapState,
    p_in_mi: bool,
    bad: bool,
}

impl Chan {
    fn new(c: usize) -> Self {
        Self {
            buf: crate::memory::touched(MAX_N + 4, 0.0),
            ola: crate::memory::touched(MAX_N + MAX_N / 4, 0.0),
            mr: crate::memory::touched(MAX_K, 0.0),
            mi: crate::memory::touched(MAX_K, 0.0),
            h: [HeadArr::new(), HeadArr::new()],
            shared: XorShift::new(0x5EC7_0B5E),
            own: XorShift::new(0xC4A1_0011 + 7919 * c as u32),
            sweep: RegionSweep::new(),
            blur_s: 0.0,
            map: MapState::default(),
            p_in_mi: false,
            bad: false,
        }
    }
    fn reset_rng(&mut self) {
        self.shared.reset();
        self.own.reset();
    }
}

/// Reinterprets f32 storage as u32 (the regions' owner indices).
#[inline]
fn as_u32(x: &mut [f32]) -> &mut [u32] {
    // SAFETY: f32 and u32 have the same size and alignment; every bit pattern is valid for both.
    unsafe { std::slice::from_raw_parts_mut(x.as_mut_ptr() as *mut u32, x.len()) }
}

#[inline]
fn as_u32_ref(x: &[f32]) -> &[u32] {
    // SAFETY: as in `as_u32`.
    unsafe { std::slice::from_raw_parts(x.as_ptr() as *const u32, x.len()) }
}

#[derive(Clone, Copy, Debug, Default)]
struct Slot {
    gen: u32,
    /// the head's arrays hold an analysis (a frozen head may use them)
    held: bool,
    /// the coherent accumulators are not running: re-seed from the analysis phases
    fresh: bool,
    /// the last coherent analysis's frame start (its phases are in `b`)
    prev_start: i64,
    prev_ok: bool,
}

pub struct Engine {
    pub active: bool,
    pub si: usize,
    pub n: usize,
    m: usize,
    nb: usize,
    pub hop: usize,
    len: usize,
    k20: usize,
    fftu: u32,
    sr: f32,
    nch: usize,
    t: u64,
    frame: bool,
    b: u64,
    passes: [Pass; MAX_PASSES],
    np: usize,
    units: usize,
    done: usize,
    pi: usize,
    pu: usize,
    pub set: FrameSet,
    slots: [Slot; HEADS],
    frame_fresh: [bool; HEADS],
    /// samples between the two analyses that give the peaks' frequencies (H / 4, or the hop from
    /// the head's previous frame), and whether it is that previous frame
    frame_dlt: [usize; HEADS],
    frame_prev: [bool; HEADS],
    gain: f32,
    ch: Vec<Chan>,
    tmp: Vec<C32>,
    scratch: Vec<C32>,
    rtmp: Vec<f32>,
    /// the overlap-add rings hold data up to this length (cleared in units once inactive)
    pub dirty: usize,
    clear_pos: usize,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        Self {
            active: false,
            si: 0,
            n: 256,
            m: 128,
            nb: 129,
            hop: 64,
            len: 320,
            k20: 1,
            fftu: 1,
            sr: 48000.0,
            nch: 0,
            t: 0,
            frame: false,
            b: 0,
            passes: [NO_PASS; MAX_PASSES],
            np: 0,
            units: 0,
            done: 0,
            pi: 0,
            pu: 0,
            set: FrameSet::default(),
            slots: [Slot::default(); HEADS],
            frame_fresh: [true; HEADS],
            frame_dlt: [64; HEADS],
            frame_prev: [false; HEADS],
            gain: 0.0,
            ch: Vec::new(),
            tmp: Vec::new(),
            scratch: Vec::new(),
            rtmp: Vec::new(),
            dirty: 0,
            clear_pos: 0,
        }
    }

    /// Allocates (and writes) for the largest size and two heads. May allocate; nothing else does.
    pub fn prepare(&mut self, sr: f32, channels: usize, t: &Tables) {
        self.sr = sr;
        self.nch = channels.clamp(1, 2);
        if self.ch.len() != self.nch {
            self.ch = Vec::new();
            self.ch = (0..self.nch).map(Chan::new).collect();
        }
        self.tmp = crate::memory::touched(t.tmp_len, C32::default());
        self.scratch = crate::memory::touched(t.scratch_len, C32::default());
        self.rtmp = crate::memory::touched(UNIT, 0.0);
        for c in &mut self.ch {
            c.ola.fill(0.0);
        }
        self.dirty = 0;
        self.clear_pos = 0;
        self.active = false;
    }

    pub fn bytes(&self) -> usize {
        let per: usize = self
            .ch
            .iter()
            .map(|c| (c.buf.len() + c.ola.len() + c.mr.len() + c.mi.len() + c.h.iter().map(|h| h.a.len() + h.b.len() + h.c.len() + h.q.len()).sum::<usize>()) * 4)
            .sum();
        per + (self.tmp.len() + self.scratch.len()) * 8 + self.rtmp.len() * 4
    }

    /// Starts the engine at size `si` (O(1): the tables are read with a stride). The overlap-add
    /// rings must be clean (`is_clean`).
    pub fn configure(&mut self, si: usize, t: &Tables) {
        let plan = &t.plans[si];
        self.si = si;
        self.n = plan.n;
        self.m = plan.m;
        self.nb = plan.m + 1;
        self.hop = plan.n / 4;
        self.len = plan.n + self.hop;
        self.fftu = plan.units as u32;
        self.k20 = ((20.0 * plan.n as f32 / self.sr).ceil() as usize).max(1);
        self.t = 0;
        self.frame = false;
        self.slots = [Slot { gen: u32::MAX, held: false, fresh: true, prev_start: 0, prev_ok: false }; HEADS];
        self.dirty = self.dirty.max(self.len);
        self.active = true;
        for c in &mut self.ch {
            c.reset_rng();
            c.bad = false;
        }
    }

    /// Full clear (reset only: one piece).
    pub fn clear_all(&mut self) {
        for c in &mut self.ch {
            c.ola.fill(0.0);
            c.reset_rng();
            c.bad = false;
        }
        self.dirty = 0;
        self.clear_pos = 0;
        self.frame = false;
        self.active = false;
    }

    pub fn is_clean(&self) -> bool {
        self.dirty == 0
    }

    /// One unit of clearing an inactive engine's rings (README §2.4: never a `fill` in one piece).
    pub fn clear_step(&mut self) {
        if self.active || self.dirty == 0 {
            return;
        }
        let (a, b) = (self.clear_pos, (self.clear_pos + UNIT).min(self.dirty));
        for c in &mut self.ch {
            c.ola[a..b].fill(0.0);
        }
        self.clear_pos = b;
        if b >= self.dirty {
            self.dirty = 0;
            self.clear_pos = 0;
        }
    }

    /// Stops the engine (its rings are then cleared in units).
    pub fn deactivate(&mut self) {
        self.active = false;
        self.frame = false;
        self.clear_pos = 0;
    }

    /// Latency of the wet against the frame it reads, in samples.
    pub fn latency(&self) -> usize {
        self.n + self.hop
    }

    /// Whether the next `tick` starts a frame (the caller latches its `FrameSet` first).
    #[inline]
    pub fn frame_due(&self) -> bool {
        (self.t + 1).is_multiple_of(self.hop as u64)
    }

    /// The frame starting on the next tick.
    pub fn start_frame(&mut self, set: FrameSet) {
        self.set = set;
        self.b = self.t + 1;
        self.frame = true;
        self.done = 0;
        self.pi = 0;
        self.pu = 0;
        self.np = 0;
        let coh = set.coherent;
        for s in 0..HEADS {
            let hs = set.heads[s];
            if hs.active && hs.gen != self.slots[s].gen {
                self.slots[s] = Slot { gen: hs.gen, held: false, fresh: true, prev_start: 0, prev_ok: false };
            }
            if !coh {
                self.slots[s].fresh = true;
            }
        }
        let any = set.heads.iter().any(|h| h.active);
        if !any {
            // nothing to say: the rings drain on their own
            self.frame = false;
            return;
        }
        let nb_u = chunks(self.nb) as u32;
        let nb_h = hchunks(self.nb) as u32;
        let n_u = chunks(self.n) as u32;
        let mut list: [Pass; MAX_PASSES] = [NO_PASS; MAX_PASSES];
        let mut np = 0;
        let mut push = |kind: Kind, ch: usize, head: usize, units: u32| {
            if units > 0 {
                list[np] = Pass { kind, ch: ch as u8, head: head as u8, units };
                np += 1;
            }
        };
        let fftu = self.fftu;
        let mut analysed = [false; HEADS];
        let mut ident_any = [false; HEADS];
        // the peaks' frequencies: from the head's previous frame when it lies 0 < Δ ≤ N / 4 back
        // (unambiguous within ±2 bins of each peak: the standard phase vocoder's case, amount ≥ 1),
        // else from a second frame H / 4 earlier (any amount, README §2.5)
        let mut starts = [0i64; HEADS];
        for s in 0..HEADS {
            let hs = set.heads[s];
            starts[s] = (hs.pos - (self.n / 2) as f64 + 0.5).floor() as i64;
            let dl = starts[s] - self.slots[s].prev_start;
            let use_prev = coh && self.slots[s].prev_ok && dl > 0 && dl as usize <= self.n / 4;
            self.frame_prev[s] = use_prev;
            self.frame_dlt[s] = if use_prev { dl as usize } else { self.n / 16 };
        }
        for c in 0..self.nch {
            if coh {
                push(Kind::Zero, c, 0, nb_u);
            }
            for s in 0..HEADS {
                let hs = set.heads[s];
                if !hs.active {
                    continue;
                }
                let need = !hs.frozen || !self.slots[s].held;
                // identity frames use the analysis as it is: no second frame, no frequencies
                let ident = coh && set.identity && !hs.frozen;
                if need {
                    analysed[s] = true;
                    ident_any[s] = ident;
                    if c == 1 && hs.mono {
                        push(Kind::Copy, c, s, nb_u);
                    } else {
                        if coh && !ident && !self.frame_prev[s] {
                            push(Kind::Read2, c, s, n_u);
                            push(Kind::Fft, c, s, fftu);
                            push(Kind::Post2, c, s, nb_h);
                        }
                        push(Kind::Read1, c, s, n_u);
                        push(Kind::Fft, c, s, fftu);
                        push(Kind::Post1, c, s, if coh { nb_h } else { nb_u });
                    }
                }
                if coh {
                    if !ident {
                        push(Kind::RegF, c, s, nb_u);
                        push(Kind::RegB, c, s, nb_u);
                    }
                    push(Kind::Map, c, s, if ident { nb_u } else { nb_h });
                }
            }
            if !coh {
                push(Kind::Mix, c, 0, nb_u);
                if (set.ratio - 1.0).abs() > 1e-7 || set.shift_hz != 0.0 {
                    push(Kind::Remap, c, 0, nb_u);
                }
            }
            if set.sigma > 0.0 {
                if coh {
                    push(Kind::Coh2P, c, 0, nb_u);
                }
                for p in 0..4 {
                    push(Kind::Blur(p), c, 0, nb_u);
                }
            }
        }
        for c in 0..self.nch {
            push(Kind::Syn, c, 0, if coh { nb_h } else { nb_u });
            push(Kind::Pack, c, 0, chunks(self.m) as u32);
            push(Kind::IFft, c, 0, fftu);
            push(Kind::Ola, c, 0, n_u);
        }
        self.passes = list;
        self.np = np;
        self.units = self.passes[..np].iter().map(|p| p.units as usize).sum();
        for s in 0..HEADS {
            self.frame_fresh[s] = self.slots[s].fresh;
            if analysed[s] {
                // an identity analysis has no frequencies: a later frozen frame analyses again
                self.slots[s].held = !ident_any[s];
                self.slots[s].prev_ok = coh;
                self.slots[s].prev_start = starts[s];
            } else {
                self.slots[s].prev_ok = false;
            }
            if coh {
                self.slots[s].fresh = false;
            }
        }
    }

    /// Units in the current frame (for the CPU report).
    pub fn frame_units(&self) -> usize {
        self.units
    }

    /// One sample: the wet out, and the frame's due units.
    #[inline]
    pub fn tick<S: Sample>(&mut self, mem: &Pool<S>, t: &Tables, ph: &Phasors, y: &mut [f32; 2]) {
        let i = (self.t % self.len as u64) as usize;
        for (c, ch) in self.ch.iter_mut().enumerate() {
            y[c] = ch.ola[i];
            ch.ola[i] = 0.0;
        }
        self.t += 1;
        if self.frame {
            let d = (self.t - self.b) as usize;
            let target = stft::due(d, self.units, self.hop);
            while self.done < target {
                self.run_unit(mem, t, ph);
                self.done += 1;
            }
            if self.done >= self.units {
                self.frame = false;
            }
        }
    }

    fn run_unit<S: Sample>(&mut self, mem: &Pool<S>, t: &Tables, ph: &Phasors) {
        let pass = self.passes[self.pi];
        let u = self.pu;
        let (c, h) = (pass.ch as usize, pass.head as usize);
        let plan = &t.plans[self.si];
        match pass.kind {
            Kind::Zero => {
                let (a, b) = chunk(self.nb, u);
                let ch = &mut self.ch[c];
                ch.mr[a..b].fill(0.0);
                ch.mi[a..b].fill(0.0);
            }
            Kind::Read2 => self.read(mem, t, plan, c, h, u, true),
            Kind::Read1 => self.read(mem, t, plan, c, h, u, false),
            Kind::Fft | Kind::IFft => {
                let ch = &mut self.ch[c];
                stft::cfft_unit(plan, t, as_complex(&mut ch.buf[..self.n]), &mut self.tmp, &mut self.scratch, u);
            }
            Kind::Post2 => {
                // the earlier frame's phases (the map takes the frequency at the peaks only)
                let (a, b) = hchunk(self.nb, u);
                let ch = &mut self.ch[c];
                let hc = &mut ch.h[h].c;
                stft::post_twiddle(plan, t, as_complex_ref(&ch.buf[..self.n]), a, b, |k, x| {
                    hc[k] = fast_atan2(x.im, x.re);
                });
            }
            Kind::Post1 => self.post1(t, plan, c, h, u),
            Kind::Copy => {
                let (a, b) = chunk(self.nb, u);
                let coh = self.set.coherent;
                let (c0, c1) = self.ch.split_at_mut(1);
                let (src, dst) = (&c0[0].h[h], &mut c1[0].h[h]);
                dst.a[a..b].copy_from_slice(&src.a[a..b]);
                if coh {
                    dst.b[a..b].copy_from_slice(&src.b[a..b]);
                    dst.c[a..b].copy_from_slice(&src.c[a..b]);
                }
                if u == 0 {
                    dst.energy = src.energy;
                    dst.amax = src.amax;
                }
            }
            Kind::RegF => {
                let (a, b) = chunk(self.nb, u);
                let nb = self.nb;
                let ch = &mut self.ch[c];
                if u == 0 {
                    ch.sweep = RegionSweep::new();
                }
                let floor = ch.h[h].amax * 1e-6;
                let (own, rest) = ch.buf.split_at_mut(nb);
                ch.sweep.forward(&ch.h[h].a, nb, floor, a, b, as_u32(own), &mut rest[..nb]);
            }
            Kind::RegB => {
                let nu = pass.units as usize;
                let (a, b) = chunk(self.nb, nu - 1 - u);
                let nb = self.nb;
                let ch = &mut self.ch[c];
                let (own, rest) = ch.buf.split_at_mut(nb);
                ch.sweep.backward(&ch.h[h].a, a, b, as_u32(own), &rest[..nb]);
                // the minima are spent: the same bins become the map's "best" (zeroed)
                rest[a..b].fill(0.0);
            }
            Kind::Map => self.map(c, h, u),
            Kind::Mix => {
                let (a, b) = chunk(self.nb, u);
                let set = self.set;
                let ch = &mut self.ch[c];
                for k in a..b {
                    let mut p = 0.0;
                    for s in 0..HEADS {
                        if set.heads[s].active {
                            let v = ch.h[s].a[k];
                            p += set.heads[s].weight * v * v;
                        }
                    }
                    ch.mr[k] = p;
                    ch.mi[k] = 0.0;
                }
                ch.p_in_mi = false;
            }
            Kind::Remap => self.remap(c, u),
            Kind::Coh2P => {
                let (a, b) = chunk(self.nb, u);
                let ch = &mut self.ch[c];
                for k in a..b {
                    ch.buf[k] = ch.mr[k] * ch.mr[k] + ch.mi[k] * ch.mi[k];
                }
            }
            Kind::Blur(p) => {
                let nu = pass.units as usize;
                let fwd = p % 2 == 0;
                let (a, b) = chunk(self.nb, if fwd { u } else { nu - 1 - u });
                let coef = stft::blur_coef(self.set.sigma);
                let coh = self.set.coherent;
                let ch = &mut self.ch[c];
                if u == 0 {
                    ch.blur_s = 0.0;
                }
                let mut s = ch.blur_s;
                let arr = if coh {
                    &mut ch.buf[..]
                } else if ch.p_in_mi {
                    &mut ch.mi[..]
                } else {
                    &mut ch.mr[..]
                };
                stft::blur_pass(arr, a, b, fwd, coef, &mut s);
                ch.blur_s = s;
            }
            Kind::Syn => self.syn(ph, c, u),
            Kind::Pack => {
                let (a, b) = chunk(self.m, u);
                let m = self.m;
                let ch = &mut self.ch[c];
                let (mr, mi) = (&ch.mr, &ch.mi);
                let z = as_complex(&mut ch.buf[..self.n]);
                for k in a..b {
                    let yk = C32::new(mr[k], mi[k]);
                    let ym = C32::new(mr[m - k], mi[m - k]);
                    z[k] = stft::pre_twiddle(plan, t, k, yk, ym);
                }
            }
            Kind::Ola => {
                let (r0, r1) = chunk(self.n, u);
                let n = self.n;
                let half = n / 2;
                let base = self.b as usize + self.hop;
                let len = self.len;
                let ch = &mut self.ch[c];
                let ola = &mut ch.ola;
                let mut bad = false;
                stft::read_time(plan, as_complex_ref(&ch.buf[..n]), r0, r1, |r, v| {
                    // rotated back: time r of the inverse is frame sample (r + N / 2) mod N
                    let i = if r >= half { r - half } else { r + half };
                    let mut x = v * plan.hann(t, i);
                    if !x.is_finite() {
                        x = 0.0;
                        bad = true;
                    }
                    ola[(base + i) % len] += x;
                });
                if bad {
                    ch.bad = true;
                }
                if u + 1 == pass.units as usize && ch.bad {
                    // a non-finite frame: this channel's spectral state starts again
                    ch.bad = false;
                    for s in &mut self.slots {
                        s.held = false;
                        s.fresh = true;
                    }
                }
            }
        }
        self.pu += 1;
        if self.pu >= pass.units as usize {
            self.pi += 1;
            self.pu = 0;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn read<S: Sample>(&mut self, mem: &Pool<S>, t: &Tables, plan: &Plan, c: usize, h: usize, u: usize, second: bool) {
        let (i0, i1) = chunk(self.n, u);
        let n = self.n;
        let half = n / 2;
        let hs = self.set.heads[h];
        let mut start = (hs.pos - half as f64 + 0.5).floor() as i64;
        if second {
            start -= (n / 16) as i64;
        }
        let tmp = &mut self.rtmp[..i1 - i0];
        mem.read(c, hs.src, start + i0 as i64, tmp);
        let buf = &mut self.ch[c].buf;
        for i in i0..i1 {
            let r = if i < half { i + half } else { i - half };
            buf[r] = tmp[i - i0] * plan.hann(t, i);
        }
    }

    fn post1(&mut self, t: &Tables, plan: &Plan, c: usize, h: usize, u: usize) {
        let coh = self.set.coherent;
        let (a, b) = if coh { hchunk(self.nb, u) } else { chunk(self.nb, u) };
        let ident = self.set.identity && !self.set.heads[h].frozen;
        let k20 = if ident { 0 } else { self.k20 };
        let m = self.m;
        let ch = &mut self.ch[c];
        let hd = &mut ch.h[h];
        if u == 0 {
            hd.energy = 0.0;
            hd.amax = 0.0;
        }
        let (mut e, mut mx) = (hd.energy, hd.amax);
        let use_prev = self.frame_prev[h];
        let (ha_, hb, hc) = (&mut hd.a, &mut hd.b, &mut hd.c);
        stft::post_twiddle(plan, t, as_complex_ref(&ch.buf[..self.n]), a, b, |k, x| {
            let mut p = x.re * x.re + x.im * x.im;
            if !p.is_finite() {
                p = 0.0;
            }
            e += if k == 0 || k == m { p } else { 2.0 * p };
            let mag = if k < k20 || (k == m && !ident) { 0.0 } else { p.sqrt() };
            ha_[k] = mag;
            mx = mx.max(mag);
            if coh {
                if use_prev {
                    hc[k] = hb[k];
                }
                hb[k] = fast_atan2(x.im, x.re);
            }
        });
        hd.energy = e;
        hd.amax = mx;
    }

    fn map(&mut self, c: usize, h: usize, u: usize) {
        let set = self.set;
        let hs = set.heads[h];
        let (j0, j1) = if set.identity && !hs.frozen { chunk(self.nb, u) } else { hchunk(self.nb, u) };
        let wgt = hs.weight.max(0.0).sqrt();
        let nb = self.nb;
        let m = self.m;
        let hop = self.hop as f32;
        let bin_hz = self.sr / self.n as f32;
        let nyq = self.sr * 0.5 * 0.999;
        let fresh = self.frame_fresh[h];
        let mapped = (set.ratio - 1.0).abs() > 1e-7 || set.shift_hz != 0.0;
        let ha = self.frame_dlt[h] as f32;
        let exp_k = TAU * ha / self.n as f32;
        let ch = &mut self.ch[c];
        let (own, best) = ch.buf.split_at_mut(nb);
        let owner = as_u32_ref(own);
        let hd = &mut ch.h[h];
        let (mr, mi) = (&mut ch.mr, &mut ch.mi);
        if set.identity && !hs.frozen {
            for j in j0..j1 {
                let th = hd.b[j];
                hd.q[j] = th;
                let (s, co) = fast_sin_cos(th);
                let v = wgt * hd.a[j];
                mr[j] += v * co;
                mi[j] += v * s;
            }
            return;
        }
        if u == 0 {
            ch.map = MapState::default();
        }
        let mut st = ch.map;
        for j in j0..j1 {
            let o = owner[j];
            if o == u32::MAX {
                continue;
            }
            if o != st.owner {
                st.owner = o;
                let kp = o as usize;
                // the peak's fractional position (parabola on log magnitudes): only a remap needs it
                let pf = if mapped {
                    let lg = |k: usize| hd.a[k.min(nb - 1)].max(1e-20).ln();
                    let (a0, a1, a2) = (lg(kp - 1), lg(kp), lg(kp + 1));
                    let den = a0 - 2.0 * a1 + a2;
                    kp as f32 + if den.abs() > 1e-12 { (0.5 * (a0 - a2) / den).clamp(-0.5, 0.5) } else { 0.0 }
                } else {
                    kp as f32
                };
                let f1 = (pf * bin_hz + set.shift_hz) * set.ratio;
                if f1 <= 20.0 || f1 >= nyq {
                    st.skip = true;
                } else {
                    st.skip = false;
                    let pt = f1 / bin_hz;
                    st.shift = pt - pf;
                    let d0 = (pt.round() as usize).clamp(1, m - 1);
                    // the peak's frequency from the two frames H / 4 apart: unambiguous at any amount
                    let expected = exp_k * kp as f32;
                    let w0 = (expected + wrap(hd.b[kp] - hd.c[kp] - expected)) / ha;
                    let wnew = (w0 + TAU * set.shift_hz / self.sr) * set.ratio;
                    st.pk = if fresh { hd.b[kp] } else { wrap(hd.q[d0] + wnew * hop) };
                    st.phi = hd.b[kp];
                }
            }
            if st.skip {
                continue;
            }
            let th = wrap(st.pk + (hd.b[j] - st.phi));
            let (s, co) = fast_sin_cos(th);
            let mag = hd.a[j];
            if !mapped {
                // the region stays where it is
                if j >= 1 && j < m {
                    mr[j] += wgt * mag * co;
                    mi[j] += wgt * mag * s;
                    if mag > best[j] {
                        best[j] = mag;
                        hd.q[j] = th;
                    }
                }
                continue;
            }
            let d = j as f32 + st.shift;
            if d < 0.0 {
                continue;
            }
            let di = d as usize;
            let frac = d - di as f32;
            for (dd, part) in [(di, 1.0 - frac), (di + 1, frac)] {
                if dd >= 1 && dd < m && part > 0.0 {
                    // power split: the two bins carry the bin's power
                    let v = mag * part.sqrt();
                    mr[dd] += wgt * v * co;
                    mi[dd] += wgt * v * s;
                    if v > best[dd] {
                        best[dd] = v;
                        hd.q[dd] = th;
                    }
                }
            }
        }
        ch.map = st;
    }

    fn remap(&mut self, c: usize, u: usize) {
        let (a, b) = chunk(self.nb, u);
        let r = self.set.ratio;
        let dlt = self.set.shift_hz * self.n as f32 / self.sr;
        let m = self.m;
        let ch = &mut self.ch[c];
        let (src, dst) = (&ch.mr, &mut ch.mi);
        if r >= 1.0 {
            // gather: dest d reads (d / r − Δ), the density scaled by 1 / r
            let inv = 1.0 / r;
            for d in a..b {
                let s = d as f32 * inv - dlt;
                dst[d] = if s < 0.0 || s >= m as f32 {
                    0.0
                } else {
                    let i = s as usize;
                    let f = s - i as f32;
                    (src[i] + (src[(i + 1).min(m)] - src[i]) * f) * inv
                };
            }
        } else {
            // scatter: each bin's power lands on (k + Δ) · r, split between two bins
            for k in a..b {
                let d = (k as f32 + dlt) * r;
                if d < 0.0 || d >= m as f32 {
                    continue;
                }
                let i = d as usize;
                let f = d - i as f32;
                dst[i] += src[k] * (1.0 - f);
                if i < m {
                    dst[i + 1] += src[k] * f;
                }
            }
        }
        ch.p_in_mi = true;
    }

    fn syn(&mut self, ph: &Phasors, c: usize, u: usize) {
        let set = self.set;
        let (a, b) = if set.coherent { hchunk(self.nb, u) } else { chunk(self.nb, u) };
        if c == 0 && u == 0 {
            self.gain = self.frame_gain();
        }
        let g = self.gain;
        let m = self.m;
        let k20 = if set.identity { 0 } else { self.k20 };
        let width = set.width;
        let ch = &mut self.ch[c];
        if !set.coherent {
            let pin = ch.p_in_mi;
            for k in a..b {
                let p = if pin { ch.mi[k] } else { ch.mr[k] };
                let mut idx = ch.shared.phase();
                if c == 1 {
                    idx = idx.wrapping_add(Phasors::index(width * ch.own.bipolar() * 2048.0));
                }
                let z = if k < k20 || k >= m { C32::default() } else { ph.get(idx) * (p.max(0.0).sqrt() * g) };
                ch.mr[k] = z.re;
                ch.mi[k] = z.im;
            }
        } else {
            let spread = set.sigma > 0.0;
            let cl = set.cloud * 2048.0;
            for k in a..b {
                let mut z = C32::new(ch.mr[k], ch.mi[k]) * g;
                if spread {
                    let pb = ch.buf[k].max(0.0);
                    let p0 = z.norm_sqr();
                    let idx = ch.shared.phase();
                    let pbg = pb * g * g;
                    if p0 > 1e-30 {
                        z *= (pbg / p0).sqrt();
                    } else {
                        z = ph.get(idx) * pbg.sqrt();
                    }
                }
                if set.cloud > 0.0 {
                    let mut v = ch.shared.bipolar();
                    if c == 1 {
                        let o = ch.own.bipolar();
                        v += (o - v) * width;
                    }
                    z *= ph.get(Phasors::index(v * cl));
                }
                if k < k20 || (k >= m && !set.identity) {
                    z = C32::default();
                }
                ch.mr[k] = z.re;
                ch.mi[k] = z.im;
            }
        }
    }

    /// The frame's synthesis gain: the `cloud` law, `compress` (one gain per frame from the level of
    /// the region read, linked over the channels) and the inverse FFT's 1 / N.
    fn frame_gain(&self) -> f32 {
        let set = self.set;
        let mut e = 0.0f64;
        for c in 0..self.nch {
            for s in 0..HEADS {
                if set.heads[s].active {
                    e += set.heads[s].weight as f64 * self.ch[c].h[s].energy as f64;
                }
            }
        }
        e /= self.nch as f64;
        let n = self.n as f64;
        let level_db = 10.0 * (e / (0.375 * n * n)).max(1e-30).log10();
        let comp = comp_gain_db(level_db as f32, set.comp);
        let cg = stft::cloud_gain(if set.coherent { set.cloud } else { 1.0 });
        cg * 10f32.powf(comp / 20.0) / self.n as f32
    }
}

/// `compress`: g = (L_ref / L)^c in dB, L_ref = −21 dBFS, lift ≤ +24 dB, and the lift falls linearly
/// in dB to 0 between −60 and −72 dBFS (no noise pumped up in a gap, no step between two frames).
pub fn comp_gain_db(level_db: f32, c: f32) -> f32 {
    if c <= 0.0 {
        return 0.0;
    }
    let g = (c * (-21.0 - level_db)).min(24.0);
    let cap = (24.0 * (level_db + 72.0) / 12.0).clamp(0.0, 24.0);
    g.min(cap)
}
