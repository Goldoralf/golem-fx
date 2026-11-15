// SPDX-License-Identifier: GPL-3.0-or-later
//! Linkwitz-Riley crossovers in three layers, one set of names for the suite (gfx-bassdi §2.9,
//! gfx-grit §2.2, gfx-mbcomp §2.1):
//!
//! 1. [`Split`]: a two-way Linkwitz-Riley split of order 2, 4 or 8 at one frequency. One tick gives
//!    the low band, the high band and their sum, the **twin**: an all-pass with gain +1 at DC. The
//!    two bands are in phase with the twin at every frequency: low = M_lo · A, high = M_hi · A with
//!    M_lo, M_hi real, positive, summing to 1 (Linkwitz 1976; for LR2 the high band is the inverted
//!    high-pass, as that order needs).
//! 2. [`BandGain`]: a band-gain section built on a split, twin + (g − 1) · band. Its phase is the
//!    twin's for every g ≥ 0 (g = 0 is a pure low- or high-pass), its magnitude runs monotonically
//!    from g to 1, and at g = 1 it is bit-identical to the twin. Its stand-alone [`Twin`] keeps a
//!    parallel path (a clean blend) aligned with it.
//! 3. [`Tree`]: N bands from N − 1 splits, each band compensated by the twins of the splits it did
//!    not pass (Prisma's topology): band 0 = LP(x1) · A(x2) · A(x3), band 1 = HP(x1) · LP(x2) · A(x3),
//!    band 2 = HP(x1) · HP(x2) · LP(x3), band 3 = HP(x1) · HP(x2) · HP(x3). Every band carries the
//!    same all-pass A(x1) · A(x2) · A(x3) times a real gain, and the bands sum to that all-pass.
//!
//! Sections are TPT (bilinear, prewarped at the crossover): one-poles for LR2, state-variable
//! sections for LR4 (one Butterworth pair, Q 1/√2) and LR8 (Q 0.541 and 1.307). The bilinear map
//! keeps the identities above exactly (warped). Each split shares one coefficient set between its
//! sections; the high band is computed as twin − low, so a split costs 2 one-pole ticks (LR2), 2 SVF
//! ticks (LR4) or 5 (LR8), a twin 1, 1 or 2.
//!
//! The frequency glides in log-Hz (one-pole, `glide_ms`), recomputed per sample while it moves. A
//! split and its twins given the same targets on the same samples follow the same coefficients, so
//! the identities hold while they move. Audio in and out is f64 (the callers' chains stay in f64);
//! state in f64.

use std::f64::consts::{PI, SQRT_2};

/// Linkwitz-Riley order (slope 12, 24 or 48 dB / oct).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    Lr2,
    Lr4,
    Lr8,
}

/// Damping k = 1 / Q of the Butterworth sections: LR4 one √2; LR8 2 cos(π/8), 2 cos(3π/8).
const K_LR4: f64 = SQRT_2;
const K_LR8: [f64; 2] = [1.847_759_065_022_573_5, 0.765_366_864_730_179_6];

/// One TPT state-variable section's coefficients (Simper / Zavalishin).
#[derive(Clone, Copy, Debug, Default)]
struct SvfC {
    a1: f64,
    a2: f64,
    a3: f64,
}

impl SvfC {
    fn new(g: f64, k: f64) -> Self {
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        Self { a1, a2, a3: g * a2 }
    }

    /// One tick: returns (band-pass, low-pass) and advances the state.
    #[inline]
    fn tick(&self, s: &mut [f64; 2], x: f64) -> (f64, f64) {
        let v3 = x - s[1];
        let v1 = self.a1 * s[0] + self.a2 * v3;
        let v2 = s[1] + self.a2 * s[0] + self.a3 * v3;
        s[0] = 2.0 * v1 - s[0];
        s[1] = 2.0 * v2 - s[1];
        (v1, v2)
    }
}

/// The coefficient set of one crossover frequency.
#[derive(Clone, Copy, Debug, Default)]
struct Coefs {
    /// One-pole G = g / (1 + g) (LR2).
    op: f64,
    /// SVF sections: [k √2] for LR4, [k1, k2] for LR8.
    svf: [SvfC; 2],
}

impl Coefs {
    fn new(order: Order, freq: f64, sample_rate: f64) -> Self {
        let g = (PI * freq / sample_rate).tan();
        let mut c = Coefs::default();
        match order {
            Order::Lr2 => c.op = g / (1.0 + g),
            Order::Lr4 => c.svf[0] = SvfC::new(g, K_LR4),
            Order::Lr8 => {
                c.svf[0] = SvfC::new(g, K_LR8[0]);
                c.svf[1] = SvfC::new(g, K_LR8[1]);
            }
        }
        c
    }

    /// TPT one-pole low-pass tick.
    #[inline]
    fn one_pole(&self, s: &mut f64, x: f64) -> f64 {
        let v = (x - *s) * self.op;
        let y = v + *s;
        *s = y + v;
        y
    }
}

/// The crossover frequency: a log-Hz one-pole glide and the coefficients it gives.
#[derive(Clone, Debug)]
struct Glide {
    order: Order,
    sr: f64,
    ln_f: f64,
    ln_target: f64,
    coef: f64,
    glide_ms: f64,
    c: Coefs,
}

impl Glide {
    fn new(order: Order) -> Self {
        let ln = 1000f64.ln();
        let mut g = Self { order, sr: 48000.0, ln_f: ln, ln_target: ln, coef: 0.0, glide_ms: 20.0, c: Coefs::default() };
        g.recompute();
        g
    }

    fn clamp(&self, f: f64) -> f64 {
        f.clamp(1.0, 0.45 * self.sr)
    }

    fn prepare(&mut self, sample_rate: f64, glide_ms: f64) {
        self.sr = sample_rate;
        self.glide_ms = glide_ms;
        self.coef = crate::util::one_pole_coef(glide_ms.max(0.0) * 0.001 * sample_rate);
        // the clamp depends on the rate
        self.ln_target = self.clamp(self.ln_target.exp()).ln();
        self.ln_f = self.ln_target;
        self.recompute();
    }

    fn recompute(&mut self) {
        self.c = Coefs::new(self.order, self.ln_f.exp(), self.sr);
    }

    fn set(&mut self, freq: f64) {
        self.ln_target = self.clamp(freq).ln();
    }

    fn set_now(&mut self, freq: f64) {
        self.set(freq);
        self.settle();
    }

    fn settle(&mut self) {
        if self.ln_f != self.ln_target {
            self.ln_f = self.ln_target;
            self.recompute();
        }
    }

    #[inline]
    fn step(&mut self) {
        if self.ln_f != self.ln_target {
            self.ln_f = self.ln_target + (self.ln_f - self.ln_target) * self.coef;
            if (self.ln_f - self.ln_target).abs() < 1e-7 {
                self.ln_f = self.ln_target;
            }
            self.recompute();
        }
    }
}

/// The three outputs of one split tick: low + high = twin.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bands {
    pub low: f64,
    pub high: f64,
    pub twin: f64,
}

/// Layer 1: a two-way Linkwitz-Riley split (gfx-grit's `LrSplit`).
#[derive(Clone, Debug)]
pub struct Split {
    glide: Glide,
    /// LR2: [one-pole 1, one-pole 2]; LR4: sections A (first, gives the twin), B; LR8: A, B, C, D
    /// (the low chain, A gives the first all-pass) and E (the twin's second all-pass).
    s: [[f64; 2]; 5],
}

impl Split {
    pub fn new(order: Order) -> Self {
        Self { glide: Glide::new(order), s: [[0.0; 2]; 5] }
    }

    pub fn order(&self) -> Order {
        self.glide.order
    }

    /// Sample rate and the glide's time constant (ms; 0 = jumps).
    pub fn prepare(&mut self, sample_rate: f32, glide_ms: f32) {
        self.glide.prepare(sample_rate as f64, glide_ms as f64);
    }

    /// The crossover frequency to glide to (Hz).
    pub fn set_freq(&mut self, freq: f32) {
        self.glide.set(freq as f64);
    }

    /// Jumps to a crossover frequency (setup, reset).
    pub fn set_freq_now(&mut self, freq: f32) {
        self.glide.set_now(freq as f64);
    }

    /// The current (gliding) crossover frequency.
    pub fn freq(&self) -> f32 {
        self.glide.ln_f.exp() as f32
    }

    /// Silence in the sections; the frequency lands on its target.
    pub fn reset(&mut self) {
        self.s = [[0.0; 2]; 5];
        self.glide.settle();
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> Bands {
        self.glide.step();
        let c = &self.glide.c;
        let s = &mut self.s;
        let (low, twin) = match self.glide.order {
            Order::Lr2 => {
                // p = one-pole: LP2 = p², twin = LP2 − HP2 = 2p − 1
                let p1 = c.one_pole(&mut s[0][0], x);
                let p2 = c.one_pole(&mut s[0][1], p1);
                (p2, 2.0 * p1 - x)
            }
            Order::Lr4 => {
                let (bp, lp) = c.svf[0].tick(&mut s[0], x);
                let (_, low) = c.svf[0].tick(&mut s[1], lp);
                (low, x - 2.0 * K_LR4 * bp)
            }
            Order::Lr8 => {
                let (bp, lp) = c.svf[0].tick(&mut s[0], x);
                let (_, lp) = c.svf[1].tick(&mut s[1], lp);
                let (_, lp) = c.svf[0].tick(&mut s[2], lp);
                let (_, low) = c.svf[1].tick(&mut s[3], lp);
                let t1 = x - 2.0 * K_LR8[0] * bp;
                let (bp2, _) = c.svf[1].tick(&mut s[4], t1);
                (low, t1 - 2.0 * K_LR8[1] * bp2)
            }
        };
        Bands { low, high: twin - low, twin }
    }
}

/// The all-pass twin of a split (the sum of its bands), alone: what a parallel path runs to stay in
/// phase with a [`Split`] or a [`BandGain`] of the same order and frequency. Bit-identical to the
/// split's own twin on the same input.
#[derive(Clone, Debug)]
pub struct Twin {
    glide: Glide,
    s: [[f64; 2]; 2],
}

impl Twin {
    pub fn new(order: Order) -> Self {
        Self { glide: Glide::new(order), s: [[0.0; 2]; 2] }
    }

    pub fn order(&self) -> Order {
        self.glide.order
    }

    pub fn prepare(&mut self, sample_rate: f32, glide_ms: f32) {
        self.glide.prepare(sample_rate as f64, glide_ms as f64);
    }

    pub fn set_freq(&mut self, freq: f32) {
        self.glide.set(freq as f64);
    }

    pub fn set_freq_now(&mut self, freq: f32) {
        self.glide.set_now(freq as f64);
    }

    pub fn freq(&self) -> f32 {
        self.glide.ln_f.exp() as f32
    }

    pub fn reset(&mut self) {
        self.s = [[0.0; 2]; 2];
        self.glide.settle();
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        self.glide.step();
        let c = &self.glide.c;
        let s = &mut self.s;
        match self.glide.order {
            Order::Lr2 => {
                let p1 = c.one_pole(&mut s[0][0], x);
                2.0 * p1 - x
            }
            Order::Lr4 => {
                let (bp, _) = c.svf[0].tick(&mut s[0], x);
                x - 2.0 * K_LR4 * bp
            }
            Order::Lr8 => {
                let (bp, _) = c.svf[0].tick(&mut s[0], x);
                let t1 = x - 2.0 * K_LR8[0] * bp;
                let (bp2, _) = c.svf[1].tick(&mut s[1], t1);
                t1 - 2.0 * K_LR8[1] * bp2
            }
        }
    }
}

/// Which band of the split a [`BandGain`] scales.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Band {
    /// A low shelf (g < 1 cuts the lows; g = 0 is a high-pass).
    Low,
    /// A high shelf (g = 0 is a low-pass).
    High,
}

/// Layer 2: a band-gain section, twin + (g − 1) · band. The gain is the caller's to smooth: it
/// scales one branch only, no filter state is disturbed.
#[derive(Clone, Debug)]
pub struct BandGain {
    split: Split,
    band: Band,
    gm1: f64,
}

impl BandGain {
    pub fn new(order: Order, band: Band) -> Self {
        Self { split: Split::new(order), band, gm1: 0.0 }
    }

    /// A twin of the same order (frequency and glide still to be set on it, like on this section).
    pub fn new_twin(&self) -> Twin {
        Twin::new(self.split.order())
    }

    pub fn prepare(&mut self, sample_rate: f32, glide_ms: f32) {
        self.split.prepare(sample_rate, glide_ms);
    }

    pub fn set_freq(&mut self, freq: f32) {
        self.split.set_freq(freq);
    }

    pub fn set_freq_now(&mut self, freq: f32) {
        self.split.set_freq_now(freq);
    }

    pub fn freq(&self) -> f32 {
        self.split.freq()
    }

    /// Linear gain of the band, ≥ 0.
    pub fn set_gain(&mut self, g: f64) {
        self.gm1 = g.max(0.0) - 1.0;
    }

    /// Gain of the band in dB (exactly 1 at 0 dB).
    pub fn set_gain_db(&mut self, db: f64) {
        self.set_gain(if db == 0.0 { 1.0 } else { 10f64.powf(db / 20.0) });
    }

    pub fn reset(&mut self) {
        self.split.reset();
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let b = self.split.process(x);
        let branch = match self.band {
            Band::Low => b.low,
            Band::High => b.high,
        };
        b.twin + self.gm1 * branch
    }
}

/// Most bands a [`Tree`] holds.
pub const MAX_BANDS: usize = 6;

/// Layer 3: an N-band Linkwitz-Riley tree with compensation all-passes (gfx-grit's `LrTree<N>`).
/// Crossovers are ascending: split k sits between band k and band k + 1.
#[derive(Clone, Debug)]
pub struct Tree {
    bands: usize,
    splits: [Split; MAX_BANDS - 1],
    /// twins[b][k]: band b's copy of split k's all-pass (k > b).
    twins: [[Twin; MAX_BANDS - 1]; MAX_BANDS - 1],
}

impl Tree {
    /// `bands`: 2 – [`MAX_BANDS`].
    pub fn new(order: Order, bands: usize) -> Self {
        Self {
            bands: bands.clamp(2, MAX_BANDS),
            splits: std::array::from_fn(|_| Split::new(order)),
            twins: std::array::from_fn(|_| std::array::from_fn(|_| Twin::new(order))),
        }
    }

    pub fn bands(&self) -> usize {
        self.bands
    }

    pub fn prepare(&mut self, sample_rate: f32, glide_ms: f32) {
        for s in self.splits.iter_mut() {
            s.prepare(sample_rate, glide_ms);
        }
        for t in self.twins.iter_mut().flatten() {
            t.prepare(sample_rate, glide_ms);
        }
    }

    /// Crossover `k` (0 – bands − 2), gliding.
    pub fn set_freq(&mut self, k: usize, freq: f32) {
        self.splits[k].set_freq(freq);
        for b in 0..k {
            self.twins[b][k].set_freq(freq);
        }
    }

    pub fn set_freq_now(&mut self, k: usize, freq: f32) {
        self.splits[k].set_freq_now(freq);
        for b in 0..k {
            self.twins[b][k].set_freq_now(freq);
        }
    }

    pub fn freq(&self, k: usize) -> f32 {
        self.splits[k].freq()
    }

    pub fn reset(&mut self) {
        for s in self.splits.iter_mut() {
            s.reset();
        }
        for t in self.twins.iter_mut().flatten() {
            t.reset();
        }
    }

    /// Splits `x` into `out[0 .. bands]`, low to high.
    #[inline]
    pub fn process(&mut self, x: f64, out: &mut [f64]) {
        let n = self.bands;
        let mut h = x;
        for k in 0..n - 1 {
            let b = self.splits[k].process(h);
            let mut v = b.low;
            for j in k + 1..n - 1 {
                v = self.twins[k][j].process(v);
            }
            out[k] = v;
            h = b.high;
        }
        out[n - 1] = h;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATES: [f32; 4] = [44100.0, 48000.0, 96000.0, 192000.0];
    const ORDERS: [Order; 3] = [Order::Lr2, Order::Lr4, Order::Lr8];

    /// DTFT of `h` at `freqs` (phasor recurrence, renormalised).
    fn dtft(h: &[f64], freqs: &[f64], sr: f64) -> Vec<(f64, f64)> {
        freqs
            .iter()
            .map(|&f| {
                let w = 2.0 * PI * f / sr;
                let (cw, sw) = (w.cos(), -w.sin());
                let (mut pr, mut pi) = (1.0f64, 0.0f64);
                let (mut re, mut im) = (0.0, 0.0);
                for (n, &v) in h.iter().enumerate() {
                    re += v * pr;
                    im += v * pi;
                    let t = pr * cw - pi * sw;
                    pi = pr * sw + pi * cw;
                    pr = t;
                    if n % 1024 == 0 {
                        let m = (pr * pr + pi * pi).sqrt();
                        pr /= m;
                        pi /= m;
                    }
                }
                (re, im)
            })
            .collect()
    }

    fn impulse(n: usize, mut f: impl FnMut(f64) -> f64) -> Vec<f64> {
        (0..n).map(|i| f(if i == 0 { 1.0 } else { 0.0 })).collect()
    }

    fn log_freqs(lo: f64, hi: f64, n: usize) -> Vec<f64> {
        (0..n).map(|i| lo * (hi / lo).powf(i as f64 / (n - 1) as f64)).collect()
    }

    fn noise(n: usize, seed: u64) -> Vec<f64> {
        let mut r = crate::lfo::Rng::new(seed);
        (0..n).map(|_| r.bipolar() as f64).collect()
    }

    fn phase_diff(a: (f64, f64), b: (f64, f64)) -> f64 {
        // arg(a / b)
        let re = a.0 * b.0 + a.1 * b.1;
        let im = a.1 * b.0 - a.0 * b.1;
        im.atan2(re).abs()
    }

    #[test]
    fn bands_sum_to_the_twin_and_twin_matches_split() {
        for order in ORDERS {
            for sr in RATES {
                let mut s = Split::new(order);
                let mut t = Twin::new(order);
                s.prepare(sr, 0.0);
                t.prepare(sr, 0.0);
                s.set_freq_now(330.0);
                t.set_freq_now(330.0);
                for x in noise(4000, 3) {
                    let b = s.process(x);
                    assert_eq!(b.twin, t.process(x), "{order:?} {sr}");
                    assert!((b.low + b.high - b.twin).abs() < 1e-14);
                }
            }
        }
    }

    #[test]
    fn crossover_slopes_and_corner() {
        // −6 dB for both bands at the crossover (LR), the order's slope an octave or two away
        let sr = 48000.0;
        for (order, slope) in [(Order::Lr2, 12.0), (Order::Lr4, 24.0), (Order::Lr8, 48.0)] {
            let mut s = Split::new(order);
            s.prepare(sr as f32, 0.0);
            s.set_freq_now(1000.0);
            let h_lo = impulse(16384, |x| s.process(x).low);
            s.reset();
            let h_hi = impulse(16384, |x| s.process(x).high);
            let fs = [1000.0, 4000.0, 250.0];
            let lo = dtft(&h_lo, &fs, sr);
            let hi = dtft(&h_hi, &fs, sr);
            let db = |c: (f64, f64)| 10.0 * (c.0 * c.0 + c.1 * c.1).log10();
            assert!((db(lo[0]) + 6.02).abs() < 0.01 && (db(hi[0]) + 6.02).abs() < 0.01, "{order:?}");
            // two octaves away: |M| = 1 / (1 + 4^n) (bilinear-warped a little at 4 kHz)
            assert!((db(lo[1]) + 2.0 * slope).abs() < 2.5, "{order:?} low at 4 kHz {}", db(lo[1]));
            assert!((db(hi[2]) + 2.0 * slope).abs() < 1.0, "{order:?} high at 250 Hz {}", db(hi[2]));
        }
    }

    /// The gfx-bassdi bench row: LR2 / LR4 (and LR8), 44.1 – 192 kHz, g from 0 to +12 dB: phase of
    /// the section = phase of its twin within 1e-5 rad; magnitude monotone from g to 1; bit-identical
    /// to the twin at g = 1.
    #[test]
    fn band_gain_keeps_the_twins_phase() {
        let gains = [0.0, 0.25, 0.5, 1.0, 2.0, 10f64.powf(12.0 / 20.0)];
        let mut worst = 0.0f64;
        for order in ORDERS {
            for sr in RATES {
                let srd = sr as f64;
                let freqs = log_freqs(20.0, 0.49 * srd, 48);
                for fc in [120.0f32, 1500.0, 12000.0] {
                    let mut t = Twin::new(order);
                    t.prepare(sr, 0.0);
                    t.set_freq_now(fc);
                    // long enough for the slowest section (LR8's Q 1.3) to decay below 1e-15
                    let n = ((20.0 * srd / fc as f64) as usize).max(16384);
                    let ht = impulse(n, |x| t.process(x));
                    let tw = dtft(&ht, &freqs, srd);
                    for band in [Band::Low, Band::High] {
                        for &g in &gains {
                            let mut s = BandGain::new(order, band);
                            s.prepare(sr, 0.0);
                            s.set_freq_now(fc);
                            s.set_gain(g);
                            let hs = impulse(n, |x| s.process(x));
                            if g == 1.0 {
                                assert_eq!(hs, ht, "{order:?} {band:?} bit-identical at g = 1");
                            }
                            let sec = dtft(&hs, &freqs, srd);
                            let mut prev = None;
                            for k in 0..freqs.len() {
                                let m = (sec[k].0.powi(2) + sec[k].1.powi(2)).sqrt();
                                if m > 1e-6 {
                                    let d = phase_diff(sec[k], tw[k]);
                                    worst = worst.max(d);
                                    assert!(d < 1e-5, "{order:?} {band:?} g {g} fc {fc} sr {sr} at {:.0} Hz: {d:e} rad", freqs[k]);
                                }
                                // monotone: towards 1 from g (low band: rising with f if g < 1)
                                if let Some(p) = prev {
                                    let rising = (band == Band::Low) == (g < 1.0);
                                    let step: f64 = if rising { m - p } else { p - m };
                                    assert!(step > -1e-9, "{order:?} {band:?} g {g} not monotone at {:.0} Hz", freqs[k]);
                                }
                                prev = Some(m);
                            }
                        }
                    }
                }
            }
        }
        eprintln!("worst section / twin phase difference: {worst:e} rad");
    }

    #[test]
    fn shelves_reach_their_gain() {
        // LR2 low shelf −12 dB at 200 Hz: −7.9 dB at 100 Hz, −4.1 at 200, −1.4 at 400 (gfx-bassdi §2.2)
        let sr = 48000.0;
        let mut s = BandGain::new(Order::Lr2, Band::Low);
        s.prepare(sr as f32, 0.0);
        s.set_freq_now(200.0);
        s.set_gain_db(-12.0);
        let h = impulse(32768, |x| s.process(x));
        let r = dtft(&h, &[100.0, 200.0, 400.0, 10.0], sr);
        let db = |c: (f64, f64)| 10.0 * (c.0 * c.0 + c.1 * c.1).log10();
        let want = [-7.9, -4.1, -1.4];
        for k in 0..3 {
            assert!((db(r[k]) - want[k]).abs() < 0.1, "{k}: {}", db(r[k]));
        }
        assert!((db(r[3]) + 12.0).abs() < 0.2, "{}", db(r[3]));
    }

    #[test]
    fn tree_sums_to_its_all_pass_with_equal_band_phases() {
        for order in [Order::Lr4, Order::Lr8, Order::Lr2] {
            for (bands, xs) in [(3usize, vec![120.0f32, 2500.0]), (4, vec![112.0, 632.0, 3557.0]), (4, vec![100.0, 115.0, 133.0])] {
                for sr in [48000.0f32, 96000.0] {
                    let srd = sr as f64;
                    let mut tree = Tree::new(order, bands);
                    tree.prepare(sr, 0.0);
                    let mut twins: Vec<Twin> = xs.iter().map(|_| Twin::new(order)).collect();
                    for (k, &f) in xs.iter().enumerate() {
                        tree.set_freq_now(k, f);
                        twins[k].prepare(sr, 0.0);
                        twins[k].set_freq_now(f);
                    }
                    let n = 32768;
                    let mut hb = vec![vec![0.0; n]; bands];
                    let mut out = [0.0; MAX_BANDS];
                    let mut ha = vec![0.0; n];
                    for i in 0..n {
                        let x = if i == 0 { 1.0 } else { 0.0 };
                        tree.process(x, &mut out);
                        let mut sum = 0.0;
                        for b in 0..bands {
                            hb[b][i] = out[b];
                            sum += out[b];
                        }
                        let mut a = x;
                        for t in twins.iter_mut() {
                            a = t.process(a);
                        }
                        ha[i] = a;
                        assert!((sum - a).abs() < 1e-12, "{order:?} {bands} sum vs all-pass at {i}");
                    }
                    let freqs = log_freqs(20.0, 0.45 * srd, 32);
                    let ap = dtft(&ha, &freqs, srd);
                    for b in 0..bands {
                        let hbd = dtft(&hb[b], &freqs, srd);
                        for k in 0..freqs.len() {
                            let m = (hbd[k].0.powi(2) + hbd[k].1.powi(2)).sqrt();
                            if m > 1e-6 {
                                let d = phase_diff(hbd[k], ap[k]);
                                assert!(d < 1e-5, "{order:?} band {b} at {:.0} Hz: {d:e}", freqs[k]);
                            }
                        }
                    }
                    // the all-pass is flat
                    for c in &ap {
                        assert!(((c.0 * c.0 + c.1 * c.1).sqrt() - 1.0).abs() < 1e-9);
                    }
                }
            }
        }
    }

    #[test]
    fn gliding_frequency_keeps_split_and_twin_together() {
        for order in ORDERS {
            let mut s = Split::new(order);
            let mut t = Twin::new(order);
            s.prepare(96000.0, 30.0);
            t.prepare(96000.0, 30.0);
            s.set_freq_now(80.0);
            t.set_freq_now(80.0);
            let x = noise(96000, 9);
            for (i, &v) in x.iter().enumerate() {
                if i % 4800 == 0 {
                    let f = if (i / 4800) % 2 == 0 { 8000.0 } else { 40.0 };
                    s.set_freq(f);
                    t.set_freq(f);
                }
                let b = s.process(v);
                assert!(b.low.is_finite() && b.high.is_finite() && b.low.abs() < 10.0);
                assert_eq!(b.twin, t.process(v));
            }
            for _ in 0..48000 {
                assert_eq!(s.process(0.0).twin, t.process(0.0));
            }
            assert!((s.freq() - 40.0).abs() < 0.01, "{}", s.freq());
        }
    }

    #[test]
    fn silence_after_a_burst_decays() {
        for order in ORDERS {
            let mut tree = Tree::new(order, 4);
            tree.prepare(192000.0, 0.0);
            for (k, f) in [30.0f32, 300.0, 3000.0].iter().enumerate() {
                tree.set_freq_now(k, *f);
            }
            let mut out = [0.0; MAX_BANDS];
            for x in noise(19200, 4) {
                tree.process(x, &mut out);
            }
            for _ in 0..192000 * 3 {
                tree.process(0.0, &mut out);
            }
            assert!(out.iter().all(|v| v.abs() < 1e-30), "{order:?} {out:?}");
        }
    }
}
