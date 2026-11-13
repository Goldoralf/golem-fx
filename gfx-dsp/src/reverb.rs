// SPDX-License-Identifier: GPL-3.0-or-later
//! The reverb engine of gfx-reverb and gfx-shimmer (design: gfx-reverb/README.md).
//!
//! Building blocks:
//! - [`Diffuser`]: 8 channels × 4 steps of delay, shuffle, polarity and Hadamard. Density before the tail
//!   (Signalsmith, *Let's write a reverb*).
//! - [`Fdn`]: a 16-line feedback delay network.
//!   - Hadamard matrix (fast Walsh–Hadamard).
//!   - A 3-band attenuation per line, from that line's own length (Jot): the decay holds whatever the
//!     size.
//!   - Seeded random-walk modulation on a quarter of the lines (Lexicon's spin and wander).
//! - [`Plate`]: Dattorro's tank (JAES 1997) with an extra all-pass in each half.
//! - [`Spring`]: dispersive all-pass chains in a low-passed loop (the drip).
//! - [`Taps`]: early reflections, gated and reverse bursts.
//!
//! Everything random comes from fixed seeds reset by `reset()`. Control values are recomputed on a
//! 16-sample grid counted from reset, so the output does not depend on the host's block size.

use crate::delay::{DelayLine, Interp, MultiLine};
use crate::lfo::Rng;

const TAU: f64 = std::f64::consts::TAU;

// ---- diffuser ------------------------------------------------------------------------------------------

#[inline]
fn hadamard8(x: &mut [f32; 8]) {
    let mut h = 1;
    while h < 8 {
        let mut i = 0;
        while i < 8 {
            for j in i..i + h {
                let (a, b) = (x[j], x[j + h]);
                x[j] = a + b;
                x[j + h] = a - b;
            }
            i += 2 * h;
        }
        h *= 2;
    }
    let s = 1.0 / 8f32.sqrt();
    for v in x.iter_mut() {
        *v *= s;
    }
}

#[inline]
fn hadamard16(x: &mut [f32; 16]) {
    let mut h = 1;
    while h < 16 {
        let mut i = 0;
        while i < 16 {
            for j in i..i + h {
                let (a, b) = (x[j], x[j + h]);
                x[j] = a + b;
                x[j + h] = a - b;
            }
            i += 2 * h;
        }
        h *= 2;
    }
    for v in x.iter_mut() {
        *v *= 0.25;
    }
}

#[derive(Clone, Default)]
pub struct Diffuser {
    lines: Vec<MultiLine<8>>,
    /// per step: delay per channel (samples at the current size), polarity flips, shuffle
    len: Vec<[f64; 8]>,
    flip: Vec<[bool; 8]>,
    perm: Vec<[usize; 8]>,
    base: Vec<[f64; 8]>,
    /// whole-sample targets the lengths glide to (interpolated reads while they move)
    target: Vec<[f64; 8]>,
    /// every length is on its target: the fast path reads these whole-sample delays directly
    settled: bool,
    taps: Vec<[usize; 8]>,
    /// the running rate over the prepared one (≤ 1: a reduced internal rate fits the prepared lines)
    rate_k: f64,
}

/// Length glide per sample (≈ 20 ms).
const GLIDE_K: f64 = 0.001;

impl Diffuser {
    /// `steps` steps, each up to `max_ms` long at the largest size; delays drawn from `seed`.
    pub fn prepare(&mut self, sample_rate: f32, steps: usize, max_ms: f64, seed: u64) {
        let mut rng = Rng::new(seed);
        self.lines = (0..steps).map(|_| Default::default()).collect();
        self.len = vec![[0.0; 8]; steps];
        self.target = vec![[0.0; 8]; steps];
        self.taps = vec![[0; 8]; steps];
        self.base = vec![[0.0; 8]; steps];
        self.flip = vec![[false; 8]; steps];
        self.perm = vec![[0; 8]; steps];
        let max = (max_ms * 0.001 * sample_rate as f64 * 2.5) as usize + 8;
        for s in 0..steps {
            // each step twice as long as the previous one (Signalsmith): density grows geometrically
            let span = max_ms * 0.5f64.powi((steps - 1 - s) as i32);
            for c in 0..8 {
                let r = rng.next_f64();
                self.base[s][c] = span * (c as f64 + r) / 8.0 * 0.001 * sample_rate as f64;
                self.flip[s][c] = rng.next_u64() & 1 == 1;
            }
            self.lines[s].prepare(max);
            // a fixed shuffle
            let mut p: [usize; 8] = core::array::from_fn(|i| i);
            for i in (1..8).rev() {
                let j = (rng.next_u64() % (i as u64 + 1)) as usize;
                p.swap(i, j);
            }
            self.perm[s] = p;
        }
        self.rate_k = 1.0;
        self.set_scale(1.0);
        self.settle();
    }

    /// Runs at `rate` × the prepared sample rate (`rate` ≤ 1). Takes effect on the next `set_scale`.
    pub fn set_rate(&mut self, rate: f64) {
        self.rate_k = rate.min(1.0);
    }

    pub fn reset(&mut self) {
        for st in self.lines.iter_mut() {
            st.reset();
        }
    }

    /// Scales every delay (size).
    pub fn set_scale(&mut self, k: f64) {
        for (s, (t, base)) in self.target.iter_mut().zip(self.base.iter()).enumerate() {
            for c in 0..8 {
                t[c] = (base[c] * k * self.rate_k).round().max(1.0);
                self.taps[s][c] = t[c] as usize;
            }
        }
        self.settled = self.len == self.target;
    }

    /// Jumps the lengths to their targets (reset, prepare).
    pub fn settle(&mut self) {
        for (l, t) in self.len.iter_mut().zip(self.target.iter()) {
            *l = *t;
        }
        self.settled = true;
    }

    #[inline]
    pub fn process(&mut self, x: &mut [f32; 8]) {
        for s in 0..self.lines.len() {
            let mut y = [0.0f32; 8];
            let l = &mut self.lines[s];
            l.push(x);
            if self.settled {
                let taps = &self.taps[s];
                for c in 0..8 {
                    y[c] = l.tap(c, taps[c]);
                }
            } else {
            for c in 0..8 {
                let (t, d) = (self.target[s][c], &mut self.len[s][c]);
                y[c] = if *d != t {
                    *d += (t - *d) * GLIDE_K;
                    if (t - *d).abs() < 1e-3 {
                        *d = t;
                    }
                    l.read(c, *d, Interp::Linear)
                } else {
                    l.tap(c, t as usize)
                };
            }
            }
            let mut z = [0.0f32; 8];
            for c in 0..8 {
                let v = y[self.perm[s][c]];
                z[c] = if self.flip[s][c] { -v } else { v };
            }
            hadamard8(&mut z);
            *x = z;
        }
        if !self.settled {
            self.settled = self.len == self.target;
        }
    }
}

// ---- FDN ------------------------------------------------------------------------------------------------

/// The output sign patterns (the same decorrelation as before, as tables).
const OUT_L: [f32; 16] = [1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0];
const OUT_R: [f32; 16] = [1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0];

#[derive(Clone, Default)]
pub struct Fdn {
    lines: MultiLine<16>,
    base: [f64; 16],
    len: [f64; 16],
    /// per-line 3-band attenuation, structure of arrays (the loops vectorise)
    lp_b: [f32; 16],
    lp_h: [f32; 16],
    g_lo: [f32; 16],
    g_mid: [f32; 16],
    g_hi: [f32; 16],
    kb: f32,
    kh: f32,
    rng: [Rng; 4],
    walk: [f64; 4],
    walk_t: [f64; 4],
    walk_ph: f64,
    mod_depth: f64,
    /// the depth asked for; `mod_depth` glides to it (a jump would move the modulated read heads)
    mod_target: f64,
    mod_rate: f64,
    sr: f64,
    /// the prepared rate: the lines hold that much
    sr_max: f64,
    pub frozen: bool,
    /// how frozen (0 … 1, glides ~40 ms): attenuation and modulation fade with it, so freezing and
    /// thawing never jump
    fz: f32,
    /// whole-sample targets the lengths glide to: interpolated reads while gliding, whole samples once
    /// there (an interpolator in a loop is a low-pass that shortens the decay at every pass)
    target: [f64; 16],
}

/// Line lengths at scale 1 (ms): mutually prime-ish, spread 31 … 97 ms.
const FDN_MS: [f64; 16] = [31.1, 34.7, 37.9, 41.3, 44.9, 48.7, 52.1, 55.9, 60.1, 64.3, 68.9, 73.1, 78.7, 83.3, 89.9, 97.1];

impl Fdn {
    pub fn prepare(&mut self, sample_rate: f32, max_scale: f64) {
        self.sr_max = sample_rate as f64;
        let max = (97.1 * max_scale * 0.001 * self.sr_max) as usize + 64;
        self.lines.prepare(max);
        self.set_rate(sample_rate);
        self.rng = [Rng::new(0xF1), Rng::new(0xF2), Rng::new(0xF3), Rng::new(0xF4)];
        self.set_scale(1.0);
        self.settle();
    }

    /// Runs at `sample_rate` (at most the prepared one). Then `set_scale`, `set_decay` and `set_mod`.
    pub fn set_rate(&mut self, sample_rate: f32) {
        self.sr = (sample_rate as f64).min(self.sr_max);
        for i in 0..16 {
            self.base[i] = FDN_MS[i] * 0.001 * self.sr;
        }
    }

    pub fn reset(&mut self) {
        self.lines.reset();
        self.lp_b = [0.0; 16];
        self.lp_h = [0.0; 16];
        for r in self.rng.iter_mut() {
            r.reset();
        }
        self.walk = [0.0; 4];
        self.walk_t = [0.0; 4];
        self.walk_ph = 0.0;
        self.fz = if self.frozen { 1.0 } else { 0.0 };
    }

    pub fn set_scale(&mut self, k: f64) {
        for i in 0..16 {
            self.target[i] = (self.base[i] * k).round();
        }
    }

    /// Jumps the lengths to their targets.
    pub fn settle(&mut self) {
        self.len = self.target;
        self.mod_depth = self.mod_target;
    }

    /// Decay per band from the current lengths (Jot): RT60 low / mid / high, and the split frequencies.
    pub fn set_decay(&mut self, rt_low: f64, rt_mid: f64, rt_high: f64, f_bass: f64, f_high: f64) {
        for i in 0..16 {
            // the length the line glides to: the gains hold once it is there (they are not recomputed
            // while nothing else moves)
            let l = if self.target[i] > 0.0 { self.target[i] } else { self.len[i] };
            let g = |rt: f64| 10f64.powf(-3.0 * l / (rt.max(0.05) * self.sr)) as f32;
            self.g_lo[i] = g(rt_low);
            self.g_mid[i] = g(rt_mid);
            self.g_hi[i] = g(rt_high);
        }
        self.kb = (1.0 - (-TAU * f_bass / self.sr).exp()) as f32;
        self.kh = (1.0 - (-TAU * f_high.min(0.45 * self.sr) / self.sr).exp()) as f32;
    }

    pub fn set_mod(&mut self, depth_ms: f64, rate_hz: f64) {
        self.mod_target = depth_ms * 0.001 * self.sr;
        self.mod_rate = rate_hz;
    }

    /// Jumps the modulation depth to its target (reset).
    pub fn settle_mod(&mut self) {
        self.mod_depth = self.mod_target;
    }

    /// One sample: `inp` 16 inputs; returns (left, right).
    #[inline]
    pub fn process(&mut self, inp: &[f32; 16]) -> (f32, f32) {
        // random walk (four independent, cosine-interpolated): the modulated lines' offsets
        self.walk_ph += self.mod_rate / self.sr;
        if self.walk_ph >= 1.0 {
            self.walk_ph -= 1.0;
            for k in 0..4 {
                self.walk[k] = self.walk_t[k];
                self.walk_t[k] = self.rng[k].bipolar() as f64;
            }
        }
        let c = 0.5 - 0.5 * (std::f64::consts::PI * self.walk_ph).cos();
        let fzt = if self.frozen { 1.0 } else { 0.0 };
        if self.fz != fzt {
            self.fz += (fzt - self.fz) * 0.0005;
            if (fzt - self.fz).abs() < 1e-4 {
                self.fz = fzt;
            }
        }
        let mod_amt = (1.0 - self.fz) as f64;
        if self.mod_depth != self.mod_target {
            self.mod_depth += (self.mod_target - self.mod_depth) * 0.0005;
            if (self.mod_target - self.mod_depth).abs() < 1e-4 {
                self.mod_depth = self.mod_target;
            }
        }
        let mut y = [0.0f32; 16];
        for i in 0..16 {
            let gliding = self.len[i] != self.target[i];
            if gliding {
                self.len[i] += (self.target[i] - self.len[i]) * GLIDE_K;
                if (self.target[i] - self.len[i]).abs() < 1e-3 {
                    self.len[i] = self.target[i];
                }
            }
            let modulated = i % 4 == 0 && mod_amt > 0.0 && self.mod_depth > 0.0;
            let d = if modulated {
                let k = i / 4;
                self.len[i] + mod_amt * self.mod_depth * (1.0 + self.walk[k] + (self.walk_t[k] - self.walk[k]) * c)
            } else {
                self.len[i]
            };
            // frozen: whole-sample reads only (an interpolator would slowly darken the held tail)
            y[i] = if modulated || gliding {
                self.lines.read(i, d.max(2.0), Interp::Lagrange3)
            } else {
                self.lines.tap(i, d as usize)
            };
        }
        // decorrelated outputs: fixed sign patterns (dot products)
        let (mut l, mut r) = (0.0f32, 0.0f32);
        for i in 0..16 {
            l += OUT_L[i] * y[i];
            r += OUT_R[i] * y[i];
        }
        // attenuation (branch-free loops over arrays), then the matrix
        let mut fb = y;
        if self.fz < 1.0 {
            let (kb, kh, fz) = (self.kb, self.kh, self.fz);
            for i in 0..16 {
                let v = y[i];
                self.lp_b[i] += kb * (v - self.lp_b[i]);
                let rest = v - self.lp_b[i];
                self.lp_h[i] += kh * (rest - self.lp_h[i]);
                let hi = rest - self.lp_h[i];
                let att = self.g_lo[i] * self.lp_b[i] + self.g_mid[i] * self.lp_h[i] + self.g_hi[i] * hi;
                fb[i] = att + (v - att) * fz;
            }
        }
        hadamard16(&mut fb);
        for i in 0..16 {
            fb[i] += inp[i];
        }
        self.lines.push(&fb);
        (0.25 * l, 0.25 * r)
    }
}

// ---- plate ----------------------------------------------------------------------------------------------

#[derive(Clone, Default)]
struct Ap {
    line: DelayLine,
    len: f64,
}

impl Ap {
    fn prepare(&mut self, len: f64, extra: usize) {
        self.len = len;
        self.line.prepare(len as usize + extra + 8);
    }
    #[inline]
    fn process(&mut self, x: f32, g: f32, d: f64) -> f32 {
        let z = self.line.read(d.max(1.0), Interp::Lagrange3);
        let w = x + g * z;
        self.line.push(w);
        z - g * w
    }
}

/// Dattorro's plate tank at any sample rate and size (delays at 29,761 Hz in his paper).
#[derive(Clone, Default)]
pub struct Plate {
    k: f64,
    bw: f32,
    in_ap: [Ap; 4],
    mod_ap: [Ap; 2],
    d1: [DelayLine; 2],
    damp: [f32; 2],
    bass: [f32; 2],
    dec_ap: [Ap; 2],
    extra_ap: [Ap; 2],
    d2: [DelayLine; 2],
    len: [f64; 6],
    lfo: f64,
    pub decay: f32,
    pub kd: f32,
    pub kb: f32,
    pub bass_g: f32,
    pub excursion: f64,
    pub rate: f64,
    pub density: f32,
    pub frozen: bool,
    sr: f64,
    sr_max: f64,
    fb: [f32; 2],
}

/// Dattorro's input bandwidth (0.9995 at 29,761 Hz: y += 0.9995 (x − y)) as the one-pole's pole at `sr`.
fn plate_bw_pole(sr: f64) -> f32 {
    0.0005f64.powf(29761.0 / sr) as f32
}

const PLATE_IN: [f64; 4] = [142.0, 107.0, 379.0, 277.0];
const PLATE_LEN: [f64; 6] = [672.0, 908.0, 4453.0, 4217.0, 3720.0, 3163.0];
const PLATE_DEC: [f64; 2] = [1800.0, 2656.0];
const PLATE_EXTRA: [f64; 2] = [503.0, 389.0];

impl Plate {
    pub fn prepare(&mut self, sample_rate: f32, max_scale: f64) {
        self.sr = sample_rate as f64;
        self.k = self.sr / 29761.0;
        let k = self.k * max_scale;
        for (a, l) in self.in_ap.iter_mut().zip(PLATE_IN) {
            a.prepare(l * self.k, 4);
        }
        for i in 0..2 {
            self.mod_ap[i].prepare(PLATE_LEN[i] * k, 64);
            self.d1[i].prepare((PLATE_LEN[2 + i] * k) as usize + 8);
            self.dec_ap[i].prepare(PLATE_DEC[i] * k, 8);
            self.extra_ap[i].prepare(PLATE_EXTRA[i] * k, 8);
            self.d2[i].prepare((PLATE_LEN[4 + i] * k) as usize + 8);
        }
        self.sr_max = self.sr;
        self.set_scale(1.0);
        self.bw = plate_bw_pole(self.sr);
    }

    /// Runs at `sample_rate` (at most the prepared one). Then `set_scale` and the decay fields.
    pub fn set_rate(&mut self, sample_rate: f32) {
        self.sr = (sample_rate as f64).min(self.sr_max);
        self.k = self.sr / 29761.0;
        for (a, l) in self.in_ap.iter_mut().zip(PLATE_IN) {
            a.len = l * self.k;
        }
        self.bw = plate_bw_pole(self.sr);
    }

    pub fn reset(&mut self) {
        for a in self.in_ap.iter_mut().chain(self.mod_ap.iter_mut()).chain(self.dec_ap.iter_mut()).chain(self.extra_ap.iter_mut()) {
            a.line.reset();
        }
        for d in self.d1.iter_mut().chain(self.d2.iter_mut()) {
            d.reset();
        }
        self.damp = [0.0; 2];
        self.bass = [0.0; 2];
        self.fb = [0.0; 2];
        self.lfo = 0.0;
    }

    pub fn set_scale(&mut self, s: f64) {
        for i in 0..6 {
            self.len[i] = PLATE_LEN[i] * self.k * s;
        }
        for i in 0..2 {
            self.dec_ap[i].len = PLATE_DEC[i] * self.k * s;
            self.extra_ap[i].len = PLATE_EXTRA[i] * self.k * s;
        }
    }

    /// One half's loop time (s), for the decay gain.
    pub fn half_loop_s(&self) -> f64 {
        (self.len[0] + self.len[2] + self.dec_ap[0].len + self.extra_ap[0].len + self.len[4]) / self.sr
    }

    #[inline]
    pub fn process(&mut self, x: f32, prev_in: &mut f32) -> (f32, f32) {
        // input: bandwidth, four diffusers (the density control scales their gains)
        *prev_in = x + self.bw * (*prev_in - x);
        let mut v = *prev_in;
        let (g1, g2) = (0.75 * self.density, 0.625 * self.density);
        for (k, a) in self.in_ap.iter_mut().enumerate() {
            let l = a.len;
            v = a.process(v, if k < 2 { g1 } else { g2 }, l);
        }
        self.lfo = (self.lfo + self.rate / self.sr).fract();
        let m = self.excursion * self.k;
        let mods = [(TAU * self.lfo).sin() * m, (TAU * self.lfo).cos() * m];
        let mut taps = [[0.0f32; 4]; 2];
        let mut outs = [0.0f32; 2];
        for h in 0..2 {
            let inp = v + self.fb[1 - h];
            let mut a = self.mod_ap[h].process(inp, -0.7, self.len[h] + mods[h]);
            self.d1[h].push(a);
            a = self.d1[h].read(self.len[2 + h], Interp::Linear);
            // damping (high) and a low split for the bass decay
            self.damp[h] = a + self.kd * (self.damp[h] - a);
            let mut b = self.damp[h];
            self.bass[h] += self.kb * (b - self.bass[h]);
            b += (self.bass_g - 1.0) * self.bass[h];
            if !self.frozen {
                b *= self.decay;
            }
            let l = self.dec_ap[h].len;
            b = self.dec_ap[h].process(b, 0.5, l);
            let l2 = self.extra_ap[h].len;
            b = self.extra_ap[h].process(b, 0.45, l2);
            self.d2[h].push(b);
            let mut c = self.d2[h].read(self.len[4 + h], Interp::Linear);
            if !self.frozen {
                c *= self.decay;
            }
            self.fb[h] = c;
            outs[h] = c;
            taps[h] = [
                self.d1[h].read(266.0 * self.k, Interp::Linear),
                self.d1[h].read(2974.0 * self.k, Interp::Linear),
                self.d2[h].read(1996.0 * self.k, Interp::Linear),
                self.dec_ap[h].line.read(1913.0 * self.k, Interp::Linear),
            ];
        }
        let _ = outs;
        // Dattorro's output taps (simplified to four per side, decorrelated across the halves)
        let l = taps[0][0] + taps[0][1] - taps[0][3] + taps[0][2] - taps[1][2] - taps[1][3];
        let r = taps[1][0] + taps[1][1] - taps[1][3] + taps[1][2] - taps[0][2] - taps[0][3];
        (0.5 * l, 0.5 * r)
    }
}

// ---- spring ---------------------------------------------------------------------------------------------

/// One spring: a chain of first-order all-passes (dispersion: lows arrive later, the drip) in a loop.
#[derive(Clone, Default)]
pub struct Spring {
    ap: Vec<(f32, f32)>,
    line: DelayLine,
    len: f64,
    lp: f32,
    rng: Rng,
    walk: f64,
}

impl Spring {
    pub fn prepare(&mut self, sample_rate: f32, stages: usize, seed: u64) {
        self.ap = vec![(0.0, 0.0); stages];
        self.line.prepare((0.2 * sample_rate as f64) as usize);
        self.rng = Rng::new(seed);
    }

    pub fn reset(&mut self) {
        for a in self.ap.iter_mut() {
            *a = (0.0, 0.0);
        }
        self.line.reset();
        self.lp = 0.0;
        self.rng.reset();
        self.walk = 0.0;
    }

    pub fn set_len(&mut self, samples: f64) {
        self.len = samples;
    }

    #[inline]
    pub fn process(&mut self, x: f32, coef: f32, n_ap: usize, fb: f32, klp: f32) -> f32 {
        self.walk += (self.rng.bipolar() as f64 - self.walk) * 0.0005;
        let d = (self.len * (1.0 + 0.002 * self.walk)).max(4.0);
        let y = self.line.read(d, Interp::Linear);
        let mut v = x + fb * y;
        for a in self.ap.iter_mut().take(n_ap) {
            let o = coef * v + a.0 - coef * a.1;
            a.0 = v;
            a.1 = o;
            v = o;
        }
        self.lp += klp * (v - self.lp);
        self.line.push(self.lp);
        y
    }
}

// ---- taps -----------------------------------------------------------------------------------------------

/// A multi-tap reader on a shared line: early reflections, gated and reverse bursts.
#[derive(Clone, Default)]
pub struct Taps {
    pub line: DelayLine,
    pos: Vec<f64>,
    gain: Vec<f32>,
    /// whole-sample targets; positions glide to them per control tick
    target: Vec<f64>,
}

impl Taps {
    pub fn prepare(&mut self, max_samples: usize, n: usize) {
        self.line.prepare(max_samples + 8);
        self.pos = vec![0.0; n];
        self.target = vec![0.0; n];
        self.gain = vec![0.0; n];
    }

    pub fn reset(&mut self) {
        self.line.reset();
        self.pos.copy_from_slice(&self.target);
    }

    pub fn set(&mut self, k: usize, pos: f64, gain: f32) {
        self.target[k] = pos.round().max(1.0);
        self.gain[k] = gain;
    }

    /// Jumps the positions to their targets.
    pub fn settle(&mut self) {
        self.pos.copy_from_slice(&self.target);
    }

    /// Call once per control tick: the positions glide towards their targets.
    pub fn tick(&mut self, k: f64) {
        for (p, t) in self.pos.iter_mut().zip(self.target.iter()) {
            if *p != *t {
                *p += (*t - *p) * k;
                if (*t - *p).abs() < 1e-3 {
                    *p = *t;
                }
            }
        }
    }

    pub fn len(&self) -> usize {
        self.pos.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pos.is_empty()
    }

    #[inline]
    pub fn read(&self, from: usize, step: usize) -> f32 {
        let mut y = 0.0;
        let mut k = from;
        while k < self.pos.len() {
            if self.gain[k] != 0.0 {
                let p = self.pos[k];
                y += self.gain[k] * if p != self.target[k] { self.line.read(p.max(1.0), Interp::Linear) } else { self.line.tap(p as usize) };
            }
            k += step;
        }
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RT60 from the energy decay of an impulse response (Schroeder integral, −5 … −25 dB fit).
    pub fn rt60(ir: &[f32], sr: f64) -> f64 {
        let mut e: Vec<f64> = ir.iter().map(|v| (*v as f64).powi(2)).collect();
        for i in (0..e.len() - 1).rev() {
            e[i] += e[i + 1];
        }
        let e0 = e[0];
        let db: Vec<f64> = e.iter().map(|v| 10.0 * (v / e0).max(1e-30).log10()).collect();
        let t5 = db.iter().position(|v| *v < -5.0).unwrap_or(0) as f64;
        let t25 = db.iter().position(|v| *v < -25.0).unwrap_or(db.len() - 1) as f64;
        (t25 - t5) / sr * 3.0
    }

    #[test]
    fn fdn_decays_as_asked() {
        for rt in [2.0f64, 8.0, 0.5] {
            let mut f = Fdn::default();
            f.prepare(48000.0, 2.0);
            f.reset();
            f.set_decay(rt, rt, rt, 300.0, 20000.0);
            let mut ir = Vec::new();
            for n in 0..(48000.0 * rt * 1.5) as usize {
                let mut x = [0.0f32; 16];
                if n == 0 {
                    x = [1.0; 16];
                }
                let (l, _) = f.process(&x);
                ir.push(l);
            }
            let got = rt60(&ir, 48000.0);
            assert!((got / rt - 1.0).abs() < 0.1, "RT60 {rt}: {got:.3}");
        }
    }

    #[test]
    fn hadamard_keeps_energy() {
        let mut x = [1.0f32, -2.0, 0.5, 3.0, 0.0, 1.5, -1.0, 2.0];
        let e0: f32 = x.iter().map(|v| v * v).sum();
        hadamard8(&mut x);
        let e1: f32 = x.iter().map(|v| v * v).sum();
        assert!((e0 - e1).abs() < 1e-4);
    }
}
