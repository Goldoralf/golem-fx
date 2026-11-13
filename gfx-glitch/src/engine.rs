// SPDX-License-Identifier: GPL-3.0-or-later
//! The spectral engine: a symmetric STFT (size N, hop N / 4) whose frame work is scheduled on sample
//! counts, never on host blocks.
//!
//! A frame analysed at sample B (it covers the inputs B − N … B − 1) is worked in sixteen steps per
//! channel (FFT, polar conversion in quarters, bins in six, cartesian in quarters, inverse FFT + overlap-add),
//! each at its own sample after B, spread evenly over the hop, channel 0 then channel 1; its output is
//! overlap-added from B + hop on. The wet is therefore late by exactly N + hop samples, identical for
//! any block size, and a block carries one step unless it is longer than hop / 33.
//!
//! The bins of a frame go through: sustain (max-hold with a T60, phases advanced at each region's peak
//! frequency) → frequency map (pitch, spread and the Move slots, each peak region moved as a rigid unit
//! with its relative phases kept) → cloud (random phases) → slots A, B, C → density → synthesis.
//! With every stage neutral the output is the input delayed (the synthesis window is normalised so the
//! analysis × synthesis products sum to one).

use crate::slots::{self, SlotState};
use gfx_dsp::lfo::Rng;
use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::f32::consts::PI;
use std::sync::Arc;

type C = Complex<f32>;

pub const SIZES: [usize; 7] = [512, 1024, 2048, 4096, 8192, 16384, 32768];
pub const MAX_N: usize = 32768;
pub const MAX_BINS: usize = MAX_N / 2 + 1;
const RING: usize = 2 * MAX_N;
const MASK: usize = RING - 1;
const TAU: f32 = 2.0 * PI;
/// Work steps of one channel's frame: FFT, 4 × polar, 6 × bins, 4 × cartesian, inverse FFT + overlap-add.
const STEPS: usize = 16;

/// Wraps a phase to (−π, π].
#[inline]
pub fn wrap(x: f32) -> f32 {
    x - TAU * (x / TAU).round()
}

/// One effect slot, latched per frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SlotSet {
    pub fx: u8,
    /// 0 – 1
    pub amt: f32,
    /// octaves above 20 Hz, lo ≤ hi
    pub lo: f32,
    pub hi: f32,
}

/// Everything a frame needs, latched when the frame is analysed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameSet {
    pub sustain_s: f32,
    /// 0 – 1
    pub damp: f32,
    /// 0 – 1
    pub cloud: f32,
    pub pitch_st: f32,
    /// −1 – 1
    pub spread: f32,
    /// −1 – 1
    pub density: f32,
    /// 0 – 1: how much the channels' randoms differ
    pub width: f32,
    pub slots: [SlotSet; 3],
}

/// Random source of one channel: a part shared by every channel (same seed) and a part of its own,
/// blended by `width`.
#[derive(Clone, Debug)]
pub struct Rand {
    shared: Rng,
    own: Rng,
    pub width: f32,
}

impl Rand {
    fn new(channel: usize) -> Self {
        Self { shared: Rng::new(0x5EC7_0B5E_ED00_0001), own: Rng::new(0xC4A1_0000_0000_0011 + 7919 * channel as u64), width: 0.0 }
    }
    fn reset(&mut self) {
        self.shared.reset();
        self.own.reset();
    }
    /// Uniform-ish in [0, 1).
    #[inline]
    pub fn unit(&mut self) -> f32 {
        let a = self.shared.next_f64() as f32;
        let b = self.own.next_f64() as f32;
        a + (b - a) * self.width
    }
    /// In [−1, 1).
    #[inline]
    pub fn bipolar(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
}

/// What the slots see of the engine.
pub struct Ctx<'a> {
    pub nb: usize,
    pub hop: f32,
    pub sr: f32,
    /// seconds per frame
    pub dt: f32,
    /// the frame's loudest magnitude
    pub peak: f32,
    /// each bin's position in octaves above 20 Hz
    pub oct: &'a [f32],
}

struct Chan {
    ring: Vec<f32>,
    acc: Vec<f32>,
    buf: Vec<f32>,
    spec: Vec<C>,
    // analysis
    mag: Vec<f32>,
    ph: Vec<f32>,
    pprev: Vec<f32>,
    w: Vec<f32>,
    // sustain field
    sm: Vec<f32>,
    sph: Vec<f32>,
    sw: Vec<f32>,
    // output field
    om: Vec<f32>,
    oph: Vec<f32>,
    ow: Vec<f32>,
    /// the map's output phases of the previous frame
    q: Vec<f32>,
    owner: Vec<u32>,
    peaks: Vec<u32>,
    best: Vec<f32>,
    /// the frame's loudest magnitude after the map (the slots' reference)
    peak: f32,
    scratch: Vec<f64>,
    dg: Vec<f32>,
    slots: [SlotState; 3],
    rand: Rand,
}

impl Chan {
    fn new(channel: usize) -> Self {
        Self {
            ring: vec![0.0; RING],
            acc: vec![0.0; RING],
            buf: vec![0.0; MAX_N],
            spec: vec![C::default(); MAX_BINS],
            mag: vec![0.0; MAX_BINS],
            ph: vec![0.0; MAX_BINS],
            pprev: vec![0.0; MAX_BINS],
            w: vec![0.0; MAX_BINS],
            sm: vec![0.0; MAX_BINS],
            sph: vec![0.0; MAX_BINS],
            sw: vec![0.0; MAX_BINS],
            om: vec![0.0; MAX_BINS],
            oph: vec![0.0; MAX_BINS],
            ow: vec![0.0; MAX_BINS],
            q: vec![0.0; MAX_BINS],
            owner: vec![0; MAX_BINS],
            peaks: Vec::with_capacity(MAX_BINS),
            best: vec![0.0; MAX_BINS],
            peak: 1e-20,
            scratch: vec![0.0; MAX_BINS + 1],
            dg: vec![0.0; MAX_BINS],
            slots: [SlotState::new(), SlotState::new(), SlotState::new()],
            rand: Rand::new(channel),
        }
    }

    fn clear(&mut self) {
        for v in [&mut self.ring, &mut self.acc, &mut self.buf] {
            v.fill(0.0);
        }
        for v in [&mut self.mag, &mut self.ph, &mut self.pprev, &mut self.w, &mut self.sm, &mut self.sph, &mut self.sw, &mut self.om, &mut self.oph, &mut self.ow, &mut self.q, &mut self.best, &mut self.dg] {
            v.fill(0.0);
        }
        self.spec.fill(C::default());
        self.scratch.fill(0.0);
        self.owner.fill(0);
        for s in &mut self.slots {
            s.clear();
        }
        self.rand.reset();
    }
}

/// Finds the peaks of `m[0..nb]` (local maxima over ±2 bins, above `floor`) and gives every bin the
/// peak of its region (regions end at the minimum between two peaks). Bins under the floor and away
/// from every peak get `u32::MAX`.
pub fn regions(m: &[f32], nb: usize, floor: f32, peaks: &mut Vec<u32>, owner: &mut [u32]) {
    peaks.clear();
    for k in 1..nb.saturating_sub(1) {
        let v = m[k];
        if v > floor && v >= m[k - 1] && v > m[k + 1] && (k < 2 || v >= m[k - 2]) && (k + 2 >= nb || v > m[k + 2]) {
            peaks.push(k as u32);
        }
    }
    owner[..nb].fill(u32::MAX);
    if peaks.is_empty() {
        return;
    }
    // boundaries: the minimum between consecutive peaks
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

pub struct Engine {
    pub n: usize,
    pub hop: usize,
    nb: usize,
    pub size_idx: usize,
    pub window: usize,
    fwd: Vec<Arc<dyn RealToComplex<f32>>>,
    inv: Vec<Arc<dyn ComplexToReal<f32>>>,
    fscratch: Vec<C>,
    iscratch: Vec<C>,
    wa: Vec<f32>,
    ws: Vec<f32>,
    oct: Vec<f32>,
    sr: f32,
    nch: usize,
    ch: Vec<Chan>,
    t: u64,
    frame_b: Option<u64>,
    next_step: usize,
    /// sustain's per-bin factor, for (sustain, damp, size)
    sus_a: Vec<f32>,
    sus_key: (f32, f32, usize),
    pub want_latch: bool,
    pub set: FrameSet,
    pub active: bool,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            n: 2048,
            hop: 512,
            nb: 1025,
            size_idx: 2,
            window: 0,
            fwd: Vec::new(),
            inv: Vec::new(),
            fscratch: Vec::new(),
            iscratch: Vec::new(),
            wa: Vec::new(),
            ws: Vec::new(),
            oct: Vec::new(),
            sr: 48000.0,
            nch: 0,
            ch: Vec::new(),
            t: 0,
            frame_b: None,
            next_step: 0,
            sus_a: Vec::new(),
            sus_key: (-1.0, -1.0, 0),
            want_latch: false,
            set: FrameSet::default(),
            active: false,
        }
    }

    /// Allocates for the largest size. May allocate; nothing else does.
    pub fn prepare(&mut self, sr: f32, channels: usize) {
        self.sr = sr;
        let mut planner = RealFftPlanner::<f32>::new();
        self.fwd = SIZES.iter().map(|&n| planner.plan_fft_forward(n)).collect();
        self.inv = SIZES.iter().map(|&n| planner.plan_fft_inverse(n)).collect();
        let fl = self.fwd.iter().map(|f| f.get_scratch_len()).max().unwrap_or(0);
        let il = self.inv.iter().map(|f| f.get_scratch_len()).max().unwrap_or(0);
        self.fscratch = vec![C::default(); fl];
        self.iscratch = vec![C::default(); il];
        self.wa = vec![0.0; MAX_N];
        self.ws = vec![0.0; MAX_N];
        self.oct = vec![0.0; MAX_BINS];
        self.sus_a = vec![0.0; MAX_BINS];
        self.nch = channels.clamp(1, 2);
        self.ch = (0..self.nch).map(Chan::new).collect();
        let (s, w) = (self.size_idx, self.window);
        self.configure(s, w);
    }

    /// Size and window. Clears the state (the caller crossfades).
    pub fn configure(&mut self, size_idx: usize, window: usize) {
        self.size_idx = size_idx.min(SIZES.len() - 1);
        self.window = window.min(2);
        let n = SIZES[self.size_idx];
        self.n = n;
        self.hop = n / 4;
        self.nb = n / 2 + 1;
        if self.wa.len() < n {
            return; // not prepared yet
        }
        let hann = |i: usize| 0.5 - 0.5 * (TAU * i as f32 / n as f32).cos();
        for i in 0..n {
            let (a, s) = match self.window {
                0 => (hann(i), hann(i)),
                1 => (1.0, hann(i)),
                _ => (1.0, 1.0),
            };
            self.wa[i] = a;
            self.ws[i] = s;
        }
        // normalise: Σ over the overlapping frames of wa · ws = 1, with the inverse FFT's 1 / N
        let hop = self.hop;
        for j in 0..hop {
            let mut s = 0.0f64;
            let mut i = j;
            while i < n {
                s += self.wa[i] as f64 * self.ws[i] as f64;
                i += hop;
            }
            let g = if s > 1e-12 { 1.0 / (s * n as f64) } else { 0.0 };
            let mut i = j;
            while i < n {
                self.ws[i] = (self.ws[i] as f64 * g) as f32;
                i += hop;
            }
        }
        for k in 0..self.nb {
            let f = (k as f32 * self.sr / n as f32).max(1e-3);
            self.oct[k] = (f / 20.0).log2();
        }
        self.clear();
    }

    pub fn clear(&mut self) {
        for c in &mut self.ch {
            c.clear();
        }
        self.t = 0;
        self.frame_b = None;
        self.next_step = 0;
        self.want_latch = false;
        self.sus_key = (-1.0, -1.0, 0);
    }

    /// Latency of the wet in samples.
    pub fn latency(&self) -> usize {
        self.n + self.hop
    }

    /// One sample in, one out, every channel.
    #[inline]
    pub fn tick(&mut self, x: &[f32; 2], y: &mut [f32; 2]) {
        let i = (self.t as usize) & MASK;
        for (c, ch) in self.ch.iter_mut().enumerate() {
            ch.ring[i] = x[c];
            y[c] = ch.acc[i];
            ch.acc[i] = 0.0;
        }
        self.t += 1;
        let hop = self.hop as u64;
        if self.t.is_multiple_of(hop) {
            self.frame_b = Some(self.t);
            self.next_step = 0;
            self.want_latch = true;
        }
        if let Some(b) = self.frame_b {
            let d = (self.t - b) as usize;
            // STEPS steps per channel, spread evenly over the hop (they end before b + hop)
            let total = STEPS * self.nch;
            let at = (self.next_step + 1) * self.hop / (STEPS * 2 + 1);
            if d == at {
                let (c, s) = (self.next_step / STEPS, self.next_step % STEPS);
                match s {
                    0 => self.analyse_fft(c, b),
                    1..=4 => self.analyse_polar(c, s - 1),
                    5..=10 => self.bins(c, s - 5),
                    11..=14 => self.synth_cart(c, s - 11),
                    _ => self.synth_ola(c, b),
                }
                self.next_step += 1;
                if self.next_step >= total {
                    self.frame_b = None;
                }
            }
        }
    }

    fn analyse_fft(&mut self, c: usize, b: u64) {
        let n = self.n;
        let nb = self.nb;
        let ch = &mut self.ch[c];
        let start = (b as usize).wrapping_sub(n) & MASK;
        // rotated by N / 2: phases referenced to the frame's centre (a partial's bins share its phase)
        let h = n / 2;
        for i in 0..n {
            ch.buf[(i + h) & (n - 1)] = ch.ring[(start + i) & MASK] * self.wa[i];
        }
        let _ = self.fwd[self.size_idx].process_with_scratch(&mut ch.buf[..n], &mut ch.spec[..nb], &mut self.fscratch);
    }

    /// Bins of quarter `part` (0 – 3).
    fn quarter(&self, part: usize) -> std::ops::Range<usize> {
        let q = self.nb.div_ceil(4);
        (part * q).min(self.nb)..((part + 1) * q).min(self.nb)
    }

    fn analyse_polar(&mut self, c: usize, part: usize) {
        let n = self.n;
        let hop = self.hop as f32;
        let range = self.quarter(part);
        let ch = &mut self.ch[c];
        for k in range {
            let z = ch.spec[k];
            let m = (z.re * z.re + z.im * z.im).sqrt();
            let p = if m > 0.0 { z.im.atan2(z.re) } else { 0.0 };
            let expected = TAU * k as f32 * hop / n as f32;
            let d = wrap(p - ch.pprev[k] - expected);
            ch.w[k] = (expected + d) / hop;
            ch.pprev[k] = p;
            ch.mag[k] = m;
            ch.ph[k] = p;
        }
    }

    /// The bins, in six steps: sustain, map + cloud, slot A, slot B, slot C, density.
    fn bins(&mut self, c: usize, part: usize) {
        match part {
            0 => self.bins_sustain(c),
            1 => self.bins_map(c),
            2..=4 => self.bins_slot(c, part - 2),
            _ => self.bins_density(c),
        }
    }

    fn bins_sustain(&mut self, c: usize) {
        let n = self.n;
        let nb = self.nb;
        let hop = self.hop as f32;
        let sr = self.sr;
        let set = self.set;
        let _ = hop;
        self.ch[c].rand.width = set.width;
        // ---- sustain: max-hold decaying at the T60, phases running at the region's peak frequency
        if set.sustain_s > 0.0 {
            let key = (set.sustain_s, set.damp, n);
            if key != self.sus_key {
                self.sus_key = key;
                let kk = (1e-3f32).ln() * hop / (set.sustain_s * sr);
                for k in 0..nb {
                    let f = k as f32 * sr / n as f32;
                    let div = 1.0 + 7.0 * set.damp * (f / 10000.0).min(1.0);
                    self.sus_a[k] = (kk * div).exp();
                }
            }
            let ch = &mut self.ch[c];
            let mut mx = 0.0f32;
            for k in 0..nb {
                mx = mx.max(ch.mag[k]);
            }
            regions(&ch.mag, nb, mx * 1e-6, &mut ch.peaks, &mut ch.owner);
            for k in 0..nb {
                let a = self.sus_a[k];
                let held = ch.sm[k] * a;
                if ch.mag[k] >= held {
                    ch.sm[k] = ch.mag[k];
                    ch.sph[k] = ch.ph[k];
                    let o = ch.owner[k];
                    ch.sw[k] = if o != u32::MAX { ch.w[o as usize] } else { ch.w[k] };
                } else {
                    ch.sm[k] = if held > 1e-12 { held } else { 0.0 };
                    ch.sph[k] = wrap(ch.sph[k] + ch.sw[k] * hop);
                }
            }
        } else {
            let ch = &mut self.ch[c];
            ch.sm[..nb].copy_from_slice(&ch.mag[..nb]);
            ch.sph[..nb].copy_from_slice(&ch.ph[..nb]);
            ch.sw[..nb].copy_from_slice(&ch.w[..nb]);
        }

    }

    fn bins_map(&mut self, c: usize) {
        let n = self.n;
        let nb = self.nb;
        let hop = self.hop as f32;
        let sr = self.sr;
        let set = self.set;
        let _ = n;
        // ---- frequency map: pitch, spread, Move slots
        let ch = &mut self.ch[c];
        let ratio = 2f32.powf(set.pitch_st / 12.0);
        let moves = set.slots.iter().any(|s| s.amt > 0.0 && (18..=20).contains(&s.fx));
        let mapped = (ratio - 1.0).abs() > 1e-6 || set.spread.abs() > 1e-6 || moves;
        if !mapped {
            ch.om[..nb].copy_from_slice(&ch.sm[..nb]);
            ch.oph[..nb].copy_from_slice(&ch.sph[..nb]);
            ch.ow[..nb].copy_from_slice(&ch.sw[..nb]);
            ch.q[..nb].copy_from_slice(&ch.sph[..nb]);
        } else {
            let mut mx = 0.0f32;
            for k in 0..nb {
                mx = mx.max(ch.sm[k]);
            }
            regions(&ch.sm, nb, mx * 1e-6, &mut ch.peaks, &mut ch.owner);
            ch.om[..nb].fill(0.0);
            ch.best[..nb].fill(0.0);
            let bin_hz = sr / n as f32;
            let nyq = sr * 0.5 * 0.999;
            // walk the regions in order (owner is monotonic between peaks)
            let mut k = 0usize;
            while k < nb {
                let o = ch.owner[k];
                if o == u32::MAX {
                    k += 1;
                    continue;
                }
                let mut e = k;
                while e + 1 < nb && ch.owner[e + 1] == o {
                    e += 1;
                }
                let kp = o as usize;
                // the peak's fractional position (parabola on log magnitudes)
                let (a0, a1, a2) = (ch.sm[kp - 1].max(1e-20).ln(), ch.sm[kp].max(1e-20).ln(), ch.sm[(kp + 1).min(nb - 1)].max(1e-20).ln());
                let den = a0 - 2.0 * a1 + a2;
                let delta = if den.abs() > 1e-12 { (0.5 * (a0 - a2) / den).clamp(-0.5, 0.5) } else { 0.0 };
                let pf = kp as f32 + delta;
                let f0 = pf * bin_hz;
                if let Some(f1) = map_freq(f0, ratio, set.spread, &set.slots, nyq) {
                    let pt = f1 / bin_hz;
                    let shift = pt - pf;
                    let fr = if pf > 1e-3 { pt / pf } else { 1.0 };
                    let d0 = pt.round() as isize;
                    if d0 >= 1 && (d0 as usize) < nb - 1 {
                        let wnew = ch.sw[kp] * fr;
                        let pk_phase = wrap(ch.q[d0 as usize] + wnew * hop);
                        for j in k..=e {
                            let d = j as f32 + shift;
                            if d < 0.0 {
                                continue;
                            }
                            let di = d.floor() as usize;
                            let frac = d - di as f32;
                            let m = ch.sm[j];
                            let phase = wrap(pk_phase + (ch.sph[j] - ch.sph[kp]));
                            for (dd, part) in [(di, 1.0 - frac), (di + 1, frac)] {
                                if dd < nb && part > 0.0 {
                                    let v = m * part;
                                    ch.om[dd] += v;
                                    if v > ch.best[dd] {
                                        ch.best[dd] = v;
                                        ch.oph[dd] = phase;
                                        ch.ow[dd] = wnew;
                                    }
                                }
                            }
                        }
                    }
                }
                k = e + 1;
            }
            for k in 0..nb {
                if ch.best[k] > 0.0 {
                    ch.q[k] = ch.oph[k];
                } else {
                    ch.oph[k] = ch.q[k];
                    ch.ow[k] = 0.0;
                }
            }
        }

        // ---- cloud: phases blended towards random values (fresh each frame)
        if set.cloud > 0.0 {
            let amt = set.cloud * PI;
            for k in 0..nb {
                ch.oph[k] += amt * ch.rand.bipolar();
            }
        }

        let ch = &mut self.ch[c];
        let mut peak = 0.0f32;
        for k in 0..nb {
            peak = peak.max(ch.om[k]);
        }
        ch.peak = peak.max(1e-20);
    }

    fn bins_slot(&mut self, c: usize, s: usize) {
        let nb = self.nb;
        let hop = self.hop as f32;
        let sr = self.sr;
        let ss = self.set.slots[s];
        let ch = &mut self.ch[c];
        let st = &mut ch.slots[s];
        if ss.fx == 0 || ss.amt <= 0.0 || (18..=20).contains(&ss.fx) {
            if st.fx != 0 {
                st.clear();
            }
            return;
        }
        let ctx = Ctx { nb, hop, sr, dt: hop / sr, peak: ch.peak, oct: &self.oct[..nb] };
        slots::apply(ss, &ctx, &mut ch.om[..nb], &mut ch.oph[..nb], &mut ch.ow[..nb], st, &mut ch.scratch, &mut ch.rand);
    }

    fn bins_density(&mut self, c: usize) {
        let nb = self.nb;
        let hop = self.hop as f32;
        let sr = self.sr;
        let d = self.set.density;
        let ch = &mut self.ch[c];
        // a compander per bin towards the frame's mean level
        if d.abs() > 1e-4 {
            density(d, nb, hop, sr, &mut ch.om, &mut ch.dg, &mut ch.scratch);
        } else {
            ch.dg[..nb].fill(0.0);
        }
    }

    fn synth_cart(&mut self, c: usize, part: usize) {
        let nb = self.nb;
        let range = self.quarter(part);
        let ch = &mut self.ch[c];
        for k in range {
            let (s, co) = ch.oph[k].sin_cos();
            let m = ch.om[k];
            ch.spec[k] = C::new(m * co, m * s);
        }
        if part == 3 {
            ch.spec[0].im = 0.0;
            ch.spec[nb - 1].im = 0.0;
        }
    }

    fn synth_ola(&mut self, c: usize, b: u64) {
        let n = self.n;
        let nb = self.nb;
        let ch = &mut self.ch[c];
        let _ = self.inv[self.size_idx].process_with_scratch(&mut ch.spec[..nb], &mut ch.buf[..n], &mut self.iscratch);
        let base = b as usize + self.hop;
        let h = n / 2;
        let mut bad = false;
        for i in 0..n {
            let v = ch.buf[(i + h) & (n - 1)] * self.ws[i];
            if !v.is_finite() {
                bad = true;
            }
            ch.acc[(base + i) & MASK] += v;
        }
        if bad {
            // guard: a non-finite frame resets this channel's spectral state (the output stage fades)
            ch.acc.fill(0.0);
            for v in [&mut ch.sm, &mut ch.sph, &mut ch.sw, &mut ch.q, &mut ch.dg] {
                v.fill(0.0);
            }
            for s in &mut ch.slots {
                s.clear();
            }
        }
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

/// Where the frequency `f` goes: pitch, then spread (an inharmonic stretch around 100 Hz), then each
/// Move slot on its region. `None`: discarded.
pub fn map_freq(f: f32, ratio: f32, spread: f32, slots: &[SlotSet; 3], nyq: f32) -> Option<f32> {
    let mut f = f * ratio;
    if spread != 0.0 && f > 0.0 {
        f = 100.0 * (f / 100.0).powf(1.0 + 0.25 * spread);
    }
    for s in slots {
        if s.amt <= 0.0 || !(18..=20).contains(&s.fx) {
            continue;
        }
        let flo = 20.0 * 2f32.powf(s.lo);
        let fhi = 20.0 * 2f32.powf(s.hi);
        if f < flo || f > fhi || fhi - flo < 1.0 {
            continue;
        }
        match s.fx {
            18 => {
                f = flo + (f - flo) * (1.0 + 0.5 * s.amt);
                if f > fhi {
                    return None;
                }
            }
            19 => f = flo + (f - flo) * (1.0 - 0.33 * s.amt),
            _ => {
                let w = fhi - flo;
                f = flo + ((f - flo) + s.amt * w).rem_euclid(w);
            }
        }
    }
    if f > 0.0 && f < nyq {
        Some(f)
    } else {
        None
    }
}

/// The compander per bin: gains computed on the spectrum smoothed over ±2 bins, towards (density > 0)
/// or away from (< 0) the mean level of the bins above −60 dB re the peak; attack 10 ms, release
/// 200 ms; never louder than the frame came in.
fn density(d: f32, nb: usize, hop: f32, sr: f32, om: &mut [f32], dg: &mut [f32], pre: &mut [f64]) {
    let mut peak = 0.0f32;
    let mut p_in = 0.0f64;
    pre[0] = 0.0;
    for k in 0..nb {
        peak = peak.max(om[k]);
        p_in += (om[k] as f64) * (om[k] as f64);
        pre[k + 1] = pre[k] + om[k] as f64;
    }
    if peak <= 1e-12 {
        dg[..nb].fill(0.0);
        return;
    }
    let lp = |k: usize, pre: &[f64]| -> f32 {
        let a = k.saturating_sub(2);
        let b = (k + 2).min(nb - 1);
        let s = (pre[b + 1] - pre[a]) / (b + 1 - a) as f64;
        20.0 * ((s as f32 / peak).max(1e-9)).log10()
    };
    let mut sum = 0.0f32;
    let mut cnt = 0usize;
    for k in 0..nb {
        let l = lp(k, pre);
        if l > -60.0 {
            sum += l;
            cnt += 1;
        }
    }
    if cnt == 0 {
        return;
    }
    let mean = sum / cnt as f32;
    let att = 1.0 - (-hop / (0.010 * sr)).exp();
    let rel = 1.0 - (-hop / (0.200 * sr)).exp();
    for k in 0..nb {
        let l = lp(k, pre);
        let target = if l > -60.0 {
            let lt = if d > 0.0 { mean + (l - mean) / (1.0 + 7.0 * d) } else { mean + (l - mean) * (1.0 - 2.0 * d) };
            (lt - l).clamp(-40.0, 24.0)
        } else {
            0.0
        };
        let c = if target < dg[k] { att } else { rel };
        dg[k] += c * (target - dg[k]);
    }
    let mut p_out = 0.0f64;
    for k in 0..nb {
        om[k] *= 10f32.powf(dg[k] / 20.0);
        p_out += (om[k] as f64) * (om[k] as f64);
    }
    if p_out > p_in && p_out > 0.0 {
        let g = (p_in / p_out).sqrt() as f32;
        for v in om.iter_mut().take(nb) {
            *v *= g;
        }
    }
}
