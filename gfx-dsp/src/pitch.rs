// SPDX-License-Identifier: GPL-3.0-or-later
//! Pitch and frequency shifters for feedback loops (gfx-delay, gfx-shimmer).
//!
//! - [`GrainShifter`]: delay-line pitch shifter. Two read heads sweep a window at the speed the ratio
//!   asks, half a window apart, crossfaded with sin² windows (Dattorro, *Effect Design* part 2). Inside a
//!   loop its window delay is part of the repeat, so it reports no latency.
//! - [`PeriodShifter`]: the live pitch-correction shifter (gfx-correct): one read head resampling at
//!   the ratio around a fixed centre, period-locked splices found by correlation, re-centring.
//! - [`FreqShifter`]: single-sideband frequency shifter. An IIR Hilbert pair of four all-pass sections
//!   per path (Niemitalo's coefficients, 90° ± 0.7° from 20 Hz to 20 kHz at 44.1 kHz), times a
//!   quadrature oscillator. Zero latency.

use crate::delay::{DelayLine, Interp};

#[derive(Clone, Debug, Default)]
pub struct GrainShifter {
    /// the last interval set (semitones), so the ratio is only recomputed when it moves
    last_st: f64,
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
        // NaN: the next interval set always applies
        self.last_st = f64::NAN;
        let s = self.start_delay();
        self.d = [s, s];
        self.age = [0.0, 0.5 * self.life];
    }

    /// Pitch ratio (2 = an octave up).
    pub fn set_ratio(&mut self, ratio: f64) {
        self.ratio = ratio.clamp(0.25, 4.0);
    }

    /// The ratio is recomputed only when the interval moves (an `exp2` per sample otherwise).
    pub fn set_semitones(&mut self, st: f64) {
        if st != self.last_st {
            self.last_st = st;
            self.set_ratio((st / 12.0).exp2());
        }
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
    /// the last interval set (semitones), so the ratio is only recomputed when it moves
    last_st: f64,
    line: DelayLine,
    centre: f64,
    search: usize,
    /// the search range the grain length is planned with (≥ `search`): shifters that must stay
    /// coherent (Poly's bands) share it, so they splice at the same moments from the same positions
    room_search: usize,
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
        self.room_search = self.search;
        self.min_life = 2.5 * ms;
        self.max_life = max_life_ms as f64 * ms;
        self.reset();
    }

    pub fn reset(&mut self) {
        self.line.reset();
        self.ratio = 1.0;
        // NaN: the next interval set always applies
        self.last_st = f64::NAN;
        let l = self.max_life;
        self.life = [l, l];
        self.age = [0.0, 0.5 * l];
        self.d = [self.centre, self.centre];
    }

    /// The reported latency of this shifter (samples): its centre.
    pub fn latency(&self) -> f64 {
        self.centre
    }

    /// Plans grain lengths with this search range (ms) instead of the shifter's own (it is clamped to at
    /// least the own one). Call after `configure`; resets.
    pub fn set_room_search(&mut self, sample_rate: f32, search_ms: f32) {
        let cap = self.line.max_delay() as f64 - self.corr as f64 - 16.0;
        let r = ((search_ms as f64 * sample_rate as f64 * 0.001) as usize).min((0.25 * cap) as usize);
        self.room_search = r.max(self.search);
        self.reset();
    }

    /// Ends head `k`'s grain in `remaining` samples at the same window phase (continuous, only faster).
    fn compress(&mut self, k: usize, remaining: f64) {
        let phi = self.age[k] / self.life[k];
        let life = remaining / (1.0 - phi).max(1e-6);
        self.life[k] = life;
        self.age[k] = phi * life;
    }

    /// The two heads' current delays (samples): a diagnostic.
    pub fn head_delays(&self) -> [f64; 2] {
        self.d
    }

    pub fn set_ratio(&mut self, ratio: f64) {
        self.ratio = ratio.clamp(0.25, 4.0);
    }

    /// The ratio is recomputed only when the interval moves (an `exp2` per sample otherwise).
    pub fn set_semitones(&mut self, st: f64) {
        if st != self.last_st {
            self.last_st = st;
            self.set_ratio((st / 12.0).exp2());
        }
    }

    fn grain(&self) -> f64 {
        let dev = (self.ratio - 1.0).abs();
        // ends at d ≥ 4: inside the overrun guard's edge (3), so a steady grain never gets compressed
        let room = 2.0 * (self.centre - 4.0 - self.room_search as f64);
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
        // nothing to align to (silence, the line still filling): the nominal start. A search on noise
        // would park the heads anywhere, and the offset would then persist
        let mut eo = 0.0f64;
        let mut k = 0;
        while k < self.corr {
            let v = self.line.tap(o + k) as f64;
            eo += v * v;
            k += STEP;
        }
        if eo < 1e-8 * (self.corr / STEP) as f64 {
            return nominal;
        }
        let lo = (nominal - self.search as f64).max(3.0) as usize;
        let hi = (nominal + self.search as f64) as usize;
        let (mut best, mut best_v) = (lo, f64::MIN);
        // equal correlations (a steady tone repeats every period): the candidate nearest the nominal
        // wins, by a penalty far below any real correlation difference
        let pen = 1e-6 * eo.sqrt() / self.search.max(1) as f64;
        for d in lo..=hi {
            let v = self.corr_at(d, o) - pen * (d as f64 - nominal).abs();
            if v > best_v {
                best_v = v;
                best = d;
            }
        }
        let best_v = self.corr_at(best, o);
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
        // splices first, against the other head's position for this same sample (aligning on a head that
        // had already advanced put every second splice (ratio − 1) samples off: a sharp / flat drift
        // growing with the shift, +6.8 cents at +24)
        for k in 0..2 {
            if self.age[k] >= self.life[k] {
                let life = self.grain();
                let travel = life * (self.ratio - 1.0);
                let nominal = self.centre + 0.5 * travel;
                self.d[k] = self.best_splice(nominal, self.d[1 - k]);
                self.age[k] = 0.0;
                self.life[k] = life;
            }
        }
        // a grain planned at one ratio overruns when the ratio sweeps meanwhile (a long grain begun near
        // unison, then an EXP / slew sweep to +24): when the head would leave the line before its fade
        // ends, the rest of the fade is compressed at the same window phase (continuous, only faster)
        let rate = self.ratio - 1.0;
        if rate.abs() > 1e-9 {
            // decided for the pair, so they stay staggered (two heads ending together leave no crossfade:
            // near unison both sit at the same delay and would reach the edge together): the younger head
            // ends by its edge, the older one by half the younger's end
            let edge = |d: f64| if rate > 0.0 { (d - 3.0) / rate } else { (max - 1.0 - d) / -rate };
            let (young, old) = if self.age[0] / self.life[0] <= self.age[1] / self.life[1] { (0, 1) } else { (1, 0) };
            let left_y = self.life[young] - self.age[young];
            let end_y = left_y.min(edge(self.d[young]).max(1.0));
            if end_y < left_y {
                self.compress(young, end_y);
            }
            let left_o = self.life[old] - self.age[old];
            let end_o = left_o.min(edge(self.d[old]).max(1.0)).min(0.5 * end_y);
            if end_o < left_o {
                self.compress(old, end_o.max(1.0));
            }
        }
        let mut y = 0.0f32;
        for k in 0..2 {
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
    /// the last interval set (semitones), so the ratio is only recomputed when it moves
    last_st: f64,
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
        self.last_st = f64::NAN;
        let s = self.start();
        self.d = [s, s];
    }

    pub fn set_ratio(&mut self, ratio: f64) {
        self.ratio = ratio.clamp(0.25, 4.0);
    }

    /// The ratio is recomputed only when the interval moves (an `exp2` per sample otherwise).
    pub fn set_semitones(&mut self, st: f64) {
        if st != self.last_st {
            self.last_st = st;
            self.set_ratio((st / 12.0).exp2());
        }
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
            // sin(πψ) by Bhaskara's approximation (error < 0.2 %: the heads stay equal-power)
            let q = self.psi[h] * (1.0 - self.psi[h]);
            let w = (16.0 * q / (5.0 - 4.0 * q)) as f32;
            let max = self.line.max_delay() as f64 - 4.0;
            y += w * self.line.read(self.d[h].clamp(2.0, max), Interp::Lagrange3);
            self.d[h] += step;
        }
        y
    }
}

/// Niemitalo's Hilbert pair: path A one sample behind and path B are 90° apart. (Until 2026-11-14 the
/// delay sat on path B: image only −18 dB at 1 kHz and a wrong-way shift above ~6 kHz at 48 kHz.)
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
    a_delay: f64,
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
        let id = self.a_delay;
        self.a_delay = i;
        (id, q)
    }
}

#[derive(Clone, Debug, Default)]
pub struct FreqShifter {
    a: [Ap2; 4],
    b: [Ap2; 4],
    a_delay: f64,
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
        let id = self.a_delay;
        self.a_delay = i;
        self.phase = (self.phase + hz / sample_rate as f64).rem_euclid(1.0);
        let (s, c) = (std::f64::consts::TAU * self.phase).sin_cos();
        (id * c + q * s) as f32
    }
}

/// The live pitch-correction shifter (gfx-correct §2.5): resampling with **period-locked splices**
/// (Hildebrand's patent, Zita-AT1).
///
/// - **One read head** moves at rate r around a fixed centre C (its delay changes by 1 − r per sample).
///   Only during a splice does a second head exist: when the delay leaves C ± T / 2 (moving away), a
///   head starts exactly one period nearer the centre and the two are crossfaded (raised cosine,
///   amplitude-complementary) over min(T, 2 ms). Shifting up repeats a cycle, shifting down drops one.
///   Breath and noise are never played by two heads at once outside a splice (no comb, no flutter).
/// - **Jump length**: the period given by the caller (an epoch tracker), searched ±10 % by correlation
///   on a coarse grid (the ~12 kHz step), then refined over ±2 samples at the full rate with a
///   parabola. With no period (unvoiced), the best lag within ±50 % of the last one, clipped to the
///   register.
/// - **Re-centring**: [`Self::recentre`] crossfades the head to C over 5 ms, so after a voiced passage
///   the output is again the input delayed by C, exactly (C is a whole number of samples).
/// - **Stereo**: the splices are decided on the analysis signal (the caller's (L + R) / 2) and applied
///   identically to every channel.
#[derive(Clone, Debug, Default)]
pub struct PeriodShifter {
    ana: DelayLine,
    ch: [DelayLine; 2],
    centre: f64,
    t_min: f64,
    t_max: f64,
    xfade_max: f64,
    recentre_len: f64,
    /// coarse search step (samples at the host rate per ~12 kHz sample)
    step: usize,
    /// correlation length (samples)
    corr: usize,
    d: f64,
    /// the second head during a splice / re-centring
    d_new: f64,
    fade_age: f64,
    fade_len: f64,
    ratio: f64,
    period: f64,
    voiced: bool,
    want_recentre: bool,
    splices: u64,
}

impl PeriodShifter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sizes the lines for the longest period this shifter will ever be configured with (samples).
    pub fn allocate(&mut self, sample_rate: f32, max_t_max: f64) {
        let ms = sample_rate as f64 * 0.001;
        let need = (max_t_max * 0.5 + 1.0 * ms) + 1.6 * max_t_max + 4.0 * ms + 16.0;
        self.ana.prepare(need as usize);
        for c in self.ch.iter_mut() {
            c.prepare(need as usize);
        }
    }

    /// The register's shortest and longest periods (samples): C = ⌈T_max / 2 + 1 ms⌉. Resets (no
    /// allocation within what `allocate` sized).
    pub fn configure(&mut self, sample_rate: f32, t_min: f64, t_max: f64) {
        let ms = sample_rate as f64 * 0.001;
        self.t_min = t_min.max(4.0);
        self.t_max = t_max.max(self.t_min + 1.0);
        self.centre = (0.5 * self.t_max + ms).ceil();
        self.xfade_max = 2.0 * ms;
        self.recentre_len = 5.0 * ms;
        self.step = (sample_rate as f64 / 12000.0).round().max(1.0) as usize;
        self.corr = ((2.0 * ms) as usize).max(16);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.ana.reset();
        for c in self.ch.iter_mut() {
            c.reset();
        }
        self.d = self.centre;
        self.d_new = self.centre;
        self.fade_age = 0.0;
        self.fade_len = 0.0;
        self.ratio = 1.0;
        self.period = 0.5 * (self.t_min + self.t_max);
        self.voiced = false;
        self.want_recentre = false;
        self.splices = 0;
    }

    /// The reported latency (samples): the centre C.
    pub fn latency(&self) -> usize {
        self.centre as usize
    }

    /// The read head's offset from the centre (samples): 0 when re-centred.
    pub fn offset(&self) -> f64 {
        self.d - self.centre
    }

    /// Splices made since reset (a diagnostic).
    pub fn splices(&self) -> u64 {
        self.splices
    }

    pub fn set_ratio(&mut self, ratio: f64) {
        self.ratio = ratio.clamp(0.5, 2.0);
    }

    /// The voice's period (samples) when the caller trusts one, else None (free search).
    pub fn set_period(&mut self, period: Option<f64>) {
        match period {
            Some(p) => {
                self.period = p.clamp(self.t_min, self.t_max);
                self.voiced = true;
            }
            None => self.voiced = false,
        }
    }

    /// Asks for the head to return to the centre (5 ms crossfade) as soon as no splice is running.
    pub fn recentre(&mut self) {
        self.want_recentre = true;
    }

    /// Normalised correlation of the analysis line's span at delay `a` with the span at delay `b`.
    #[inline]
    fn corr_at(&self, a: f64, b: f64) -> f64 {
        let (mut c, mut e, mut f) = (0.0f64, 1e-12f64, 1e-12f64);
        let mut k = 0;
        while k < self.corr {
            let x = self.ana.read(a + k as f64, Interp::Linear) as f64;
            let y = self.ana.read(b + k as f64, Interp::Linear) as f64;
            c += x * y;
            e += x * x;
            f += y * y;
            k += 2;
        }
        c / (e * f).sqrt()
    }

    /// The jump (samples, > 0) that best continues the head at delay `d`, the new head at d ± jump.
    fn best_jump(&self, d: f64, sign: f64) -> f64 {
        let (lo, hi) = if self.voiced {
            (0.9 * self.period, 1.1 * self.period)
        } else {
            (0.5 * self.period, 1.5 * self.period)
        };
        let (lo, mut hi) = (lo.max(self.t_min), hi.min(self.t_max));
        if sign < 0.0 {
            // the new head must stay inside the line (delay ≥ the interpolator's guard)
            hi = hi.min(d - 6.0);
        }
        let lo = lo.min(hi);
        let at = |j: f64| self.corr_at(d, d + sign * j);
        // coarse grid, then ±2 samples, then a parabola
        let mut best = self.period.clamp(lo, hi);
        let mut bv = f64::MIN;
        let mut j = lo;
        while j <= hi {
            let v = at(j);
            if v > bv {
                bv = v;
                best = j;
            }
            j += self.step as f64;
        }
        let c = best;
        for k in -2..=2 {
            let j = c + k as f64;
            if j >= lo && j <= hi {
                let v = at(j);
                if v > bv {
                    bv = v;
                    best = j;
                }
            }
        }
        let (a, b) = (at(best - 1.0), at(best + 1.0));
        let den = a - 2.0 * bv + b;
        let frac = if den.abs() > 1e-12 { (0.5 * (a - b) / den).clamp(-0.5, 0.5) } else { 0.0 };
        best + frac
    }

    /// Processes one frame: `ana` is the analysis sample ((L + R) / 2), `io` the channels (1 or 2),
    /// replaced by the shifted output.
    #[inline]
    pub fn process(&mut self, ana: f32, io: &mut [f32]) {
        self.ana.push(ana);
        for (c, x) in io.iter().enumerate() {
            self.ch[c].push(*x);
        }
        let rate = 1.0 - self.ratio;
        if self.fade_len == 0.0 {
            let half = 0.5 * self.period;
            let lo_edge = self.centre - half;
            let hi_edge = self.centre + half;
            // hard edges of the line, whatever the period
            let min_d = 3.0;
            let max_d = self.centre + 0.5 * self.t_max + 2.0;
            if (self.d > hi_edge && rate > 0.0) || self.d > max_d {
                let j = self.best_jump(self.d, -1.0);
                self.start_fade(self.d - j, self.xfade_max.min(self.period));
            } else if (self.d < lo_edge && rate < 0.0) || self.d < min_d {
                let j = self.best_jump(self.d, 1.0);
                self.start_fade(self.d + j, self.xfade_max.min(self.period));
            } else if self.want_recentre {
                self.want_recentre = false;
                if (self.d - self.centre).abs() > 1e-9 {
                    self.start_fade(self.centre, self.recentre_len);
                    self.splices -= 1;
                }
            }
        }
        if self.fade_len > 0.0 {
            let w = 0.5 - 0.5 * (std::f64::consts::PI * self.fade_age / self.fade_len).cos();
            let (wo, wn) = ((1.0 - w) as f32, w as f32);
            for (c, x) in io.iter_mut().enumerate() {
                *x = wo * self.ch[c].read(self.d, Interp::Lagrange3) + wn * self.ch[c].read(self.d_new, Interp::Lagrange3);
            }
            self.fade_age += 1.0;
            self.d += rate;
            self.d_new += rate;
            if self.fade_age >= self.fade_len {
                self.d = self.d_new;
                self.fade_len = 0.0;
            }
        } else {
            for (c, x) in io.iter_mut().enumerate() {
                *x = self.ch[c].read(self.d, Interp::Lagrange3);
            }
            self.d += rate;
        }
    }

    fn start_fade(&mut self, d_new: f64, len: f64) {
        self.d_new = d_new;
        self.fade_age = 0.0;
        self.fade_len = len.max(1.0).round();
        self.splices += 1;
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
    fn period_shifter_identity_shift_and_recentre() {
        let sr = 48000.0f32;
        let (t_min, t_max) = (sr as f64 / 800.0, sr as f64 / 90.0);
        let mut p = PeriodShifter::new();
        p.allocate(sr, sr as f64 / 60.0);
        p.configure(sr, t_min, t_max);
        let c = p.latency();
        assert_eq!(c, (0.5 * t_max + 48.0).ceil() as usize);
        // identity at r = 1
        let x: Vec<f32> = (0..20000).map(|i| (0.37 * i as f32).sin() * (0.0011 * i as f32).cos()).collect();
        for (i, &v) in x.iter().enumerate() {
            let mut io = [v];
            p.process(v, &mut io);
            if i >= c {
                assert_eq!(io[0], x[i - c]);
            }
        }
        // a 220 Hz tone shifted up 1 st and down 1 st
        for st in [1.0f64, -1.0, 0.5] {
            p.reset();
            let f0 = 220.0f32;
            p.set_period(Some(sr as f64 / f0 as f64));
            p.set_ratio((st / 12.0).exp2());
            let y: Vec<f32> = (0..96000)
                .map(|i| {
                    let v = (std::f32::consts::TAU * f0 * i as f32 / sr).sin();
                    let mut io = [v];
                    p.process(v, &mut io);
                    io[0]
                })
                .collect();
            let f = freq_of(&y[4800..], sr as f64);
            let cents = 1200.0 * (f / f0 as f64).log2();
            assert!((cents - st * 100.0).abs() < 2.0, "{st} st: {cents:.2} cents");
            assert!(p.splices() > 5);
        }
        // back to unity, re-centred: the delayed input again, exactly
        p.set_ratio(1.0);
        p.recentre();
        let base = 96000usize;
        let mut ok = 0;
        for i in 0..4800 {
            let v = (0.21 * (base + i) as f32).sin();
            let mut io = [v];
            p.process(v, &mut io);
            if i > 600 {
                let want = (0.21 * (base + i - c) as f32).sin();
                assert_eq!(io[0], want, "sample {i}");
                ok += 1;
            }
        }
        assert!(ok > 0 && p.offset() == 0.0);
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
                // within 0.2 cents (before the splice-order fix: +1.5 at +12)
                assert!((cents - st * 100.0).abs() < 0.2, "{f0} Hz, {st} st: {cents:.2} cents");
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

    /// level of `f` in `y` (single-bin DFT, Hann window), dB
    fn level_db(y: &[f32], f: f64, sr: f64) -> f64 {
        let n = y.len();
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (k, v) in y.iter().enumerate() {
            let w = 0.5 - 0.5 * (std::f64::consts::TAU * k as f64 / n as f64).cos();
            let a = std::f64::consts::TAU * f * k as f64 / sr;
            re += *v as f64 * w * a.cos();
            im -= *v as f64 * w * a.sin();
        }
        20.0 * ((re * re + im * im).sqrt() * 4.0 / n as f64).max(1e-12).log10()
    }

    #[test]
    fn freq_shifter_rejects_the_image() {
        // the wanted sideband up, the image under −40 dB, across the band (82 Hz – 15 kHz at 48 kHz)
        let sr = 48000.0;
        for (f0, hz) in [(82.0, 40.0), (1000.0, 200.0), (5000.0, 200.0), (15000.0, 200.0), (1000.0, -200.0)] {
            let mut fs = FreqShifter::new();
            let y: Vec<f32> = (0..96000).map(|i| fs.process((std::f64::consts::TAU * f0 * i as f64 / sr).sin() as f32, hz, sr as f32)).collect();
            let y = &y[48000..];
            let want = level_db(y, f0 + hz, sr);
            let image = level_db(y, f0 - hz, sr);
            assert!(want > -1.0, "{f0} {hz}: wanted {want:.1} dB");
            assert!(image - want < -40.0, "{f0} {hz}: image {:.1} dB", image - want);
        }
    }
}
