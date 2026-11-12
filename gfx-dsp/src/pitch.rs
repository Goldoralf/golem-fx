// SPDX-License-Identifier: GPL-3.0-or-later
//! Pitch and frequency shifters for feedback loops (gfx-delay, gfx-shimmer).
//!
//! - [`GrainShifter`]: delay-line pitch shifter. Two read heads sweep a window at the speed the ratio
//!   asks, half a window apart, crossfaded with sin² windows (Dattorro, *Effect Design* part 2). Inside a
//!   loop its window delay is part of the repeat, so it reports no latency.
//! - [`FreqShifter`]: single-sideband frequency shifter. An IIR Hilbert pair of four all-pass sections
//!   per path (Niemitalo's coefficients, 90° ± 0.7° from 20 Hz to 20 kHz at 44.1 kHz), times a
//!   quadrature oscillator. Zero latency.

use crate::delay::{DelayLine, Interp};

#[derive(Clone, Debug, Default)]
pub struct GrainShifter {
    line: DelayLine,
    /// grain lifetime (samples)
    life: f64,
    /// splice search range (samples) and correlation length
    range: usize,
    corr: usize,
    /// per head: delay (samples) and age (samples)
    d: [f64; 2],
    age: [f64; 2],
    ratio: f64,
}

impl GrainShifter {
    pub fn new() -> Self {
        Self::default()
    }

    /// `window_ms` sets the grain (longer = smoother, more smear).
    pub fn prepare(&mut self, sample_rate: f32, window_ms: f32) {
        self.life = (window_ms as f64 * 0.001 * sample_rate as f64).max(64.0);
        self.range = (0.010 * sample_rate as f64) as usize;
        self.corr = (0.003 * sample_rate as f64) as usize;
        // the longest delay: start of a grain at ratio 4 plus the search range and the correlation span
        self.line.prepare((self.life * 3.0) as usize + 2 * self.range + self.corr + 16);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.line.reset();
        self.ratio = 1.0;
        let s = self.start_delay();
        self.d = [s, s];
        self.age = [0.0, 0.5 * self.life];
    }

    /// Pitch ratio (2 = an octave up).
    pub fn set_ratio(&mut self, ratio: f64) {
        self.ratio = ratio.clamp(0.25, 4.0);
    }

    pub fn set_semitones(&mut self, st: f64) {
        self.set_ratio((st / 12.0).exp2());
    }

    /// Where a grain starts: up-shifting grains start late and run towards the write head.
    fn start_delay(&self) -> f64 {
        let base = 2.0 + self.range as f64;
        if self.ratio > 1.0 {
            base + self.life * (self.ratio - 1.0)
        } else {
            base
        }
    }

    /// The delay near `nominal` (± range) whose recent past best matches what head `other` plays now.
    fn best_splice(&self, nominal: f64, other: f64) -> f64 {
        let n = self.corr;
        let o = other.round() as usize;
        let lo = (nominal as isize - self.range as isize).max(2) as usize;
        let hi = nominal as usize + self.range;
        let (mut best, mut best_v) = (nominal, f64::MIN);
        let mut d = lo;
        while d <= hi {
            let mut c = 0.0f64;
            let mut e = 1e-12f64;
            for k in 0..n {
                let a = self.line.tap(d + k) as f64;
                c += a * self.line.tap(o + k) as f64;
                e += a * a;
            }
            let v = c / e.sqrt();
            if v > best_v {
                best_v = v;
                best = d as f64 + (other - o as f64);
            }
            d += 1;
        }
        best
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.line.push(x);
        let mut y = 0.0f32;
        for k in 0..2 {
            if self.age[k] >= self.life {
                // restart this grain where it continues the other one's waveform
                let other = self.d[1 - k];
                self.d[k] = self.best_splice(self.start_delay(), other);
                self.age[k] -= self.life;
            }
            let w = (std::f64::consts::PI * self.age[k] / self.life).sin();
            y += (w * w) as f32 * self.line.read(self.d[k].max(2.0), Interp::Lagrange3);
            self.d[k] += 1.0 - self.ratio;
            self.age[k] += 1.0;
        }
        y
    }
}

/// Correlation stride of the splice search (every second sample: half the cost; a full-rate search gave
/// the same tuning).
const STEP: usize = 2;

/// A splice-searching delay-line shifter centred on a fixed delay (gfx-pitch, gfx-detune).
///
/// - **Fixed mean delay**: the heads travel around `centre`, from centre + T/2 down to centre − T/2
///   (up-shifts) or the reverse, so the delay a host must compensate never moves with the ratio.
/// - **Per-sample ratio**: an EXP sweep is a smooth glide.
/// - **Splices**: each new head starts where its waveform best continues the playing head's, by
///   correlation over a short span (WSOLA; the H3000's "deglitch"). Heads are faded by sin², which is
///   amplitude-complementary for the in-phase splices the search finds.
/// - **Grain length**: the longest that keeps the travel inside the centre (capped by `max_life`).
#[derive(Clone, Debug, Default)]
pub struct CenteredShifter {
    line: DelayLine,
    centre: f64,
    search: usize,
    corr: usize,
    min_life: f64,
    max_life: f64,
    life: [f64; 2],
    age: [f64; 2],
    d: [f64; 2],
    ratio: f64,
}

impl CenteredShifter {
    pub fn new() -> Self {
        Self::default()
    }

    /// `centre_ms`: the fixed mean delay; `search_ms`: the splice search range (± around the nominal
    /// start); `max_life_ms`: the longest grain.
    pub fn prepare(&mut self, sample_rate: f32, centre_ms: f32, search_ms: f32, max_life_ms: f32) {
        self.allocate(sample_rate, centre_ms, search_ms);
        self.configure(sample_rate, centre_ms, search_ms, max_life_ms);
    }

    /// Sizes the line for the largest centre and search this shifter will ever be configured with.
    pub fn allocate(&mut self, sample_rate: f32, max_centre_ms: f32, max_search_ms: f32) {
        let ms = sample_rate as f64 * 0.001;
        let corr = ((3.0 * ms) as usize).max(16);
        self.line.prepare((2.0 * max_centre_ms as f64 * ms) as usize + 2 * (max_search_ms as f64 * ms) as usize + corr + 16);
    }

    /// Sets the shape within the allocated size and resets (no allocation: real-time safe).
    pub fn configure(&mut self, sample_rate: f32, centre_ms: f32, search_ms: f32, max_life_ms: f32) {
        let ms = sample_rate as f64 * 0.001;
        self.corr = ((3.0 * ms) as usize).max(16);
        let cap = self.line.max_delay() as f64 - self.corr as f64 - 16.0;
        self.centre = (centre_ms as f64 * ms).min(0.5 * cap);
        self.search = ((search_ms as f64 * ms) as usize).min((0.25 * cap) as usize);
        self.min_life = 2.5 * ms;
        self.max_life = max_life_ms as f64 * ms;
        self.reset();
    }

    pub fn reset(&mut self) {
        self.line.reset();
        self.ratio = 1.0;
        let l = self.max_life;
        self.life = [l, l];
        self.age = [0.0, 0.5 * l];
        self.d = [self.centre, self.centre];
    }

    /// The reported latency of this shifter (samples): its centre.
    pub fn latency(&self) -> f64 {
        self.centre
    }

    pub fn set_ratio(&mut self, ratio: f64) {
        self.ratio = ratio.clamp(0.25, 4.0);
    }

    pub fn set_semitones(&mut self, st: f64) {
        self.set_ratio((st / 12.0).exp2());
    }

    fn grain(&self) -> f64 {
        let dev = (self.ratio - 1.0).abs();
        let room = 2.0 * (self.centre - 2.0 - self.search as f64);
        if dev < 1e-9 {
            self.max_life
        } else {
            (room / dev).clamp(self.min_life, self.max_life)
        }
    }

    /// Normalised correlation of the span at delay `d` with the span at `o`.
    #[inline]
    fn corr_at(&self, d: usize, o: usize) -> f64 {
        let (mut c, mut e) = (0.0f64, 1e-12f64);
        let mut k = 0;
        while k < self.corr {
            let a = self.line.tap(d + k) as f64;
            c += a * self.line.tap(o + k) as f64;
            e += a * a;
            k += STEP;
        }
        c / e.sqrt()
    }

    fn best_splice(&self, nominal: f64, other: f64) -> f64 {
        if self.search == 0 {
            return nominal;
        }
        let o = other.round().max(2.0) as usize;
        let lo = (nominal - self.search as f64).max(3.0) as usize;
        let hi = (nominal + self.search as f64) as usize;
        let (mut best, mut best_v) = (lo, f64::MIN);
        for d in lo..=hi {
            let v = self.corr_at(d, o);
            if v > best_v {
                best_v = v;
                best = d;
            }
        }
        // sub-sample refinement: a parabola through the peak and its neighbours
        let (a, b, c) = (self.corr_at(best - 1, o), best_v, self.corr_at(best + 1, o));
        let den = a - 2.0 * b + c;
        let frac = if den.abs() > 1e-12 { (0.5 * (a - c) / den).clamp(-0.5, 0.5) } else { 0.0 };
        best as f64 + frac + (other - o as f64)
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.line.push(x);
        let max = self.line.max_delay() as f64 - 4.0;
        let mut y = 0.0f32;
        for k in 0..2 {
            if self.age[k] >= self.life[k] {
                let life = self.grain();
                let travel = life * (self.ratio - 1.0);
                let nominal = self.centre + 0.5 * travel;
                self.d[k] = self.best_splice(nominal, self.d[1 - k]);
                self.age[k] = 0.0;
                self.life[k] = life;
            }
            let w = (std::f64::consts::PI * self.age[k] / self.life[k]).sin();
            y += (w * w) as f32 * self.line.read(self.d[k].clamp(2.0, max), Interp::Lagrange3);
            self.d[k] += 1.0 - self.ratio;
            self.age[k] += 1.0;
        }
        y
    }
}

/// The shifter for a reverb's feedback loop (gfx-shimmer).
///
/// Inside a reverb the phase is already scrambled, so splice search finds nothing better than chance.
/// The random splices are the sound (Costello, Valhalla DSP), which is why this shifter has none:
/// - two heads half a window apart, **equal-power** sin / cos weights (a linear crossfade loses
///   ~1.8 dB per pass on decorrelated content and skews the decay);
/// - window lengths jittered from a fixed seed;
/// - an optional reverse-grain mode (each grain plays backwards: smoother, organ-like).
#[derive(Clone, Debug, Default)]
pub struct LoopShifter {
    line: DelayLine,
    win: f64,
    cur_win: f64,
    psi: [f64; 2],
    d: [f64; 2],
    ratio: f64,
    reverse: bool,
    rng: crate::lfo::Rng,
    seed: u64,
}

impl LoopShifter {
    pub fn new(seed: u64) -> Self {
        Self { seed, rng: crate::lfo::Rng::new(seed), ..Default::default() }
    }

    pub fn prepare(&mut self, sample_rate: f32, window_ms: f32) {
        self.win = (window_ms as f64 * 0.001 * sample_rate as f64).max(32.0);
        // the longest travel: a reverse grain at ratio 4 over a jittered window
        self.line.prepare((self.win * 1.3 * 5.0) as usize + 16);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.line.reset();
        self.rng = crate::lfo::Rng::new(self.seed);
        self.cur_win = self.win;
        self.psi = [0.0, 0.5];
        self.ratio = 1.0;
        let s = self.start();
        self.d = [s, s];
    }

    pub fn set_ratio(&mut self, ratio: f64) {
        self.ratio = ratio.clamp(0.25, 4.0);
    }

    pub fn set_semitones(&mut self, st: f64) {
        self.set_ratio((st / 12.0).exp2());
    }

    pub fn set_reverse(&mut self, reverse: bool) {
        self.reverse = reverse;
    }

    fn start(&self) -> f64 {
        if !self.reverse && self.ratio > 1.0 {
            2.0 + self.cur_win * (self.ratio - 1.0)
        } else {
            2.0
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.line.push(x);
        let step = if self.reverse { 1.0 + self.ratio } else { 1.0 - self.ratio };
        let mut y = 0.0f32;
        for h in 0..2 {
            self.psi[h] += 1.0 / self.cur_win;
            if self.psi[h] >= 1.0 {
                self.psi[h] -= 1.0;
                if h == 0 {
                    // a new window length for the next cycle (±20 %, seeded)
                    self.cur_win = self.win * (1.0 + 0.2 * self.rng.bipolar() as f64);
                }
                self.d[h] = self.start();
            }
            let w = (std::f64::consts::PI * self.psi[h]).sin() as f32;
            let max = self.line.max_delay() as f64 - 4.0;
            y += w * self.line.read(self.d[h].clamp(2.0, max), Interp::Lagrange3);
            self.d[h] += step;
        }
        y
    }
}

/// Niemitalo's Hilbert pair: path A and path B (B one sample behind) are 90° apart.
const HA: [f64; 4] = [0.6923878, 0.936_065_432_295_9, 0.988_229_522_686_0, 0.998_748_845_273_7];
const HB: [f64; 4] = [0.402_192_116_242_6, 0.856_171_088_242_0, 0.972_290_954_565_1, 0.995_288_479_127_8];

#[derive(Clone, Copy, Debug, Default)]
struct Ap2 {
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Ap2 {
    /// y[n] = a² (x[n] + y[n−2]) − x[n−2]
    #[inline]
    fn run(&mut self, a: f64, x: f64) -> f64 {
        let y = a * a * (x + self.y2) - self.x2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// The analytic signal of a real input: (in phase, quadrature), 90° apart over the audio band.
#[derive(Clone, Debug, Default)]
pub struct Hilbert {
    a: [Ap2; 4],
    b: [Ap2; 4],
    b_delay: f64,
}

impl Hilbert {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> (f64, f64) {
        let x = x as f64;
        let mut i = x;
        for (s, a) in self.a.iter_mut().zip(HA) {
            i = s.run(a, i);
        }
        let mut q = x;
        for (s, a) in self.b.iter_mut().zip(HB) {
            q = s.run(a, q);
        }
        let qd = self.b_delay;
        self.b_delay = q;
        (i, qd)
    }
}

#[derive(Clone, Debug, Default)]
pub struct FreqShifter {
    a: [Ap2; 4],
    b: [Ap2; 4],
    b_delay: f64,
    phase: f64,
}

impl FreqShifter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Shifts every component of `x` by `hz` (negative = down).
    #[inline]
    pub fn process(&mut self, x: f32, hz: f64, sample_rate: f32) -> f32 {
        let x = x as f64;
        let mut i = x;
        for (s, a) in self.a.iter_mut().zip(HA) {
            i = s.run(a, i);
        }
        let mut q = x;
        for (s, a) in self.b.iter_mut().zip(HB) {
            q = s.run(a, q);
        }
        let qd = self.b_delay;
        self.b_delay = q;
        self.phase = (self.phase + hz / sample_rate as f64).rem_euclid(1.0);
        let (s, c) = (std::f64::consts::TAU * self.phase).sin_cos();
        (i * c + qd * s) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn freq_of(y: &[f32], sr: f64) -> f64 {
        let mut t = Vec::new();
        for i in 1..y.len() {
            if y[i - 1] < 0.0 && y[i] >= 0.0 {
                t.push((i - 1) as f64 + (-y[i - 1] / (y[i] - y[i - 1])) as f64);
            }
        }
        (t.len() - 1) as f64 / (t[t.len() - 1] - t[0]) * sr
    }

    #[test]
    fn grain_shifter_hits_the_ratio() {
        for f0 in [110.0f32, 330.0, 440.0, 1234.0] {
            for st in [12.0, 7.0, -12.0, 0.3] {
                let mut g = GrainShifter::new();
                g.prepare(48000.0, 60.0);
                g.set_semitones(st);
                let y: Vec<f32> = (0..96000).map(|i| g.process((std::f32::consts::TAU * f0 * i as f32 / 48000.0).sin())).collect();
                let f = freq_of(&y[12000..], 48000.0);
                let cents = 1200.0 * (f / f0 as f64).log2();
                assert!((cents - st * 100.0).abs() < 3.0, "{f0} Hz, {st} st: {cents:.1} cents");
            }
        }
    }

    #[test]
    fn loop_shifter_shifts_and_keeps_power() {
        for (st, rev) in [(12.0, false), (-12.0, false), (12.0, true), (7.0, false)] {
            let mut g = LoopShifter::new(5);
            g.prepare(48000.0, 60.0);
            g.set_reverse(rev);
            g.set_semitones(st);
            let mut r = crate::lfo::Rng::new(3);
            // decorrelated input (noise through a resonator at 500 Hz): the output keeps its power
            let (mut a, mut b) = (0.0f32, 0.0f32);
            let x: Vec<f32> = (0..96000)
                .map(|_| {
                    let v = 0.1 * r.bipolar() + 1.97 * a - 0.985 * b;
                    b = a;
                    a = v;
                    v * 0.05
                })
                .collect();
            let y: Vec<f32> = x.iter().map(|v| g.process(*v)).collect();
            let p = |v: &[f32]| v.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / v.len() as f64;
            let db = 10.0 * (p(&y[24000..]) / p(&x[24000..])).log10();
            assert!(db.abs() < 1.0, "{st} st reverse {rev}: power {db:.2} dB");
        }
        let mut g = LoopShifter::new(5);
        g.prepare(48000.0, 60.0);
        g.set_semitones(12.0);
        let y: Vec<f32> = (0..96000).map(|i| g.process((std::f32::consts::TAU * 330.0 * i as f32 / 48000.0).sin())).collect();
        let f = freq_of(&y[12000..], 48000.0);
        assert!((f / 660.0 - 1.0).abs() < 0.02, "{f}");
    }

    #[test]
    fn centered_shifter_is_in_tune_and_on_its_centre() {
        for f0 in [110.0f32, 330.0, 880.0] {
            for st in [12.0, 7.0, -5.0, -12.0, 0.25] {
                let mut g = CenteredShifter::new();
                // the search covers half a period of a low E (6 ms)
                g.prepare(48000.0, 12.0, 6.0, 60.0);
                g.set_semitones(st);
                let y: Vec<f32> = (0..96000).map(|i| g.process((std::f32::consts::TAU * f0 * i as f32 / 48000.0).sin())).collect();
                let f = freq_of(&y[12000..], 48000.0);
                let cents = 1200.0 * (f / f0 as f64).log2();
                // ±2 cents: the worst case (110 Hz, +12) sits at +1.5
                assert!((cents - st * 100.0).abs() < 2.0, "{f0} Hz, {st} st: {cents:.2} cents");
            }
        }
    }

    #[test]
    fn freq_shifter_moves_by_hertz() {
        let mut fs = FreqShifter::new();
        let y: Vec<f32> = (0..48000).map(|i| fs.process((std::f32::consts::TAU * 1000.0 * i as f32 / 48000.0).sin(), 30.0, 48000.0)).collect();
        let f = freq_of(&y[4800..], 48000.0);
        assert!((f - 1030.0).abs() < 1.0, "{f}");
        // single sideband: the level stays put (no beating against an image)
        let pk: Vec<f32> = y[4800..].chunks(1600).map(|c| c.iter().fold(0.0f32, |m, v| m.max(v.abs()))).collect();
        let (lo, hi) = pk.iter().fold((f32::MAX, 0.0f32), |(a, b), v| (a.min(*v), b.max(*v)));
        assert!(hi / lo < 1.05, "{lo} {hi}");
    }
}
