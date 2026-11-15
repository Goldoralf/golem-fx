// SPDX-License-Identifier: GPL-3.0-or-later
//! The phase-locked partial tracker (gfx-sustainer README §2.2 / §2.7; users: gfx-ringmod's Track
//! carrier, gfx-sustainer, gfx-polysynth's voices).
//!
//! One harmonic series: a **reference** partial (the fundamental) and up to K **partials** k = 2 … K+1.
//!
//! - **Phase detector**: the input times e^(−j·2π·ψ) (ψ = the NCO's phase, turns, f64), averaged over
//!   **one period of the NCO** (a fractional-length moving average: a comb with a zero on every
//!   harmonic, half a period of delay). The result z = d · e^(j·2π·e) gives the partial's level d (a unit
//!   sine reads 1) and the phase error e (turns) at once. The filter sits **inside** the loop, at
//!   baseband: nothing in front of the loop delays the string's phase modulation (the band-pass pair of
//!   gfx-ringmod's first design delayed it by 2.5 periods).
//!   - Deviation from gfx-sustainer §2.2, measured here: its two one-poles at f0 / 3 leave the 2nd
//!     harmonic 20 dB down and lag about a period, which allowed a bandwidth of f0 / 5 only; on a ±50 ¢,
//!     6 Hz vibrato that left 22 ¢ of error at A2 and 38 ¢ at E2. The comb nulls every harmonic and lags
//!     half a period, so the bandwidth rises to f0 / 2.5: 4.3 ¢ at A2, 8.0 ¢ at E2, 1.1 ¢ at A3.
//! - **Loop**: PI, damping 0.707, closed-loop bandwidth clamp(f0 / 2.5, 14, 100) Hz (ωn = 2π·bw / 2.06).
//!   Once engaged, the integrator holds the whole frequency, bounded to ±1 semitone around the
//!   feed-forward: the feed-forward (the caller's f0 from YIN, `multipitch` or `zoom`, smoothed over
//!   30 ms, `set_ff_time`) names the note and runs the NCO until the loop engages. Added to the loop
//!   it would be a disturbance as large as the vibrato it trails by YIN's window.
//! - **Engage**: feed-forward valid, d ≥ the hold level, the phase error settled (circular variance over
//!   10 ms under 0.02: a feed-forward a few cents off rotates the error too slowly to count) and five periods since the feed-forward (re)started. [`Engage::Snap`] puts ψ on the
//!   input's phase (for a silent generator); [`Engage::Keep`] keeps ψ and pulls the offset in through the
//!   frequency at most 5 ¢ away (a sounding generator: no step, no chirp).
//! - **Hold**: the reference is tracked while d ≥ the hold level and the error's circular variance over
//!   50 ms stays under 0.125 (≈ (0.5 rad)²). When it is lost, the NCO runs at the **mean of its last
//!   150 ms** (the centre of a vibrato, not where it stopped) until the loop engages again or a new note
//!   is fed forward (a jump of more than 50 ¢ between successive readings; a vibrato moves a few cents
//!   per reading).
//! - **Partials** k ≥ 2: oscillator k runs at k · ψ + φ_k, accumulated (its increment is k times the
//!   reference's times a trim ratio, ±15 ¢); the same demodulator at its phase (four one-poles at
//!   0.4 · f0: the neighbours, f0 away, are 34 dB down) and a 4 Hz PI move the trim. A bend followed by
//!   the reference carries every partial with it; the trim only absorbs the string's inharmonicity.
//! - **Weak fundamental**: [`Tracker::set_reference`] runs the reference loop on partial k_r (its error
//!   divided by k_r), ψ still the fundamental's phase.
//! - **Decimated partial loops** (opt-in, [`Tracker::set_partial_rate`], added for gfx-sustainer's bank
//!   of 16 partials; the default is the per-sample path above, unchanged): per sample, each partial's
//!   mixing (a polynomial sine / cosine) and the first **two** of its four one-poles; every n samples
//!   (n ≤ the cap, and fs / n ≥ 4 f0) the last two one-poles, the level and error, the variance
//!   averages and the trim's PI, scaled to the n-sample step. The sum-frequency image is 40 dB down
//!   before the decimation (k = 2, its worst case); the neighbours, f0 away, stay under fs / 2n.
//! - **Hold margin** (opt-in, [`Tracker::set_hold_margin`], gfx-sustainer): the 150 ms history of the
//!   reference's frequency, and the decimated partials' trim integrators, learn only while their level
//!   is that margin above the hold level. A partial fading into hiss keeps following it down to the
//!   hold level, but the frequency it is held at is the one measured with a good signal-to-noise
//!   ratio (under −60 dBFS of hiss the plain 150 ms mean held a fading E4 10 ¢ flat).
//!
//! Real-time safe (memory sized in `prepare`), deterministic, phases and states in f64.

use std::f64::consts::PI;

/// How the loop takes hold of a partial (gfx-sustainer §2.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engage {
    /// The generator is silent: ψ jumps onto the input's phase (nothing is heard).
    Snap,
    /// The generator is sounding: ψ stays continuous; the offset is pulled in through the frequency,
    /// at most 5 ¢ away.
    Keep,
}

/// The state of one tracked partial after a sample.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lock {
    /// The oscillator's phase, turns (0 – 1).
    pub psi: f64,
    /// Its increment, turns per sample (the frequency in Hz is `inc` × the sample rate).
    pub inc: f64,
    /// The partial's level from the demodulator (a unit sine reads 1).
    pub level: f32,
    /// Phase-locked now.
    pub locked: bool,
}

/// Feed-forward smoothing (ms), default.
const FF_MS: f64 = 30.0;
/// The demodulators' corners, × f0.
const PART_LP: f64 = 0.4;
/// Loop bandwidth: clamp(f0 / 5, 14, 50) Hz; the partials' trim loop.
const BW_DIV: f64 = 2.5;
const BW_MIN: f64 = 14.0;
const BW_MAX: f64 = 100.0;
/// The reference's comb is sized for this lowest fundamental (Hz).
const F_LOWEST: f64 = 20.0;
const PART_BW: f64 = 4.0;
const ZETA: f64 = 0.707;
/// The reference's correction bound (semitones) and the partials' trim bound (cents).
const CORR_ST: f64 = 1.0;
const TRIM_CENTS: f64 = 15.0;
/// Keep: the offset's pull-in rate, as a frequency deviation (cents).
const PULL_CENTS: f64 = 5.0;
/// Circular variance of the error over 50 ms: engage under, lose over.
const VAR_MS: f64 = 50.0;
const ENG_MS: f64 = 10.0;
const VAR_ENGAGE: f64 = 0.02;
const VAR_LOSE: f64 = 0.125;
/// Decimated partial loops: the smoothing of the frequency-offset estimate (per update), and of the
/// fitted inharmonicity.
const ACQ_A: f64 = 0.05;
const STRETCH_A: f64 = 0.01;
/// Periods after a feed-forward (re)start before the loop may engage.
const SETTLE_PERIODS: f64 = 5.0;
/// Hold: the mean of the last 150 ms of the increment, kept in chunks of 32 samples.
const HOLD_MS: f64 = 150.0;
const CHUNK: usize = 32;
/// A feed-forward this far from the held or current frequency is a new note (cents).
const NEW_NOTE_CENTS: f64 = 50.0;
/// Coefficients are recomputed when the smoothed feed-forward moved this much (ratio).
const RETUNE: f64 = 1e-4;

/// sin and cos of a phase in turns, evaluated in f32 after the wrap (the phase itself stays f64; the
/// demodulator's error from it is under 1e−6 rad).
#[inline]
fn sin_cos_turns(t: f64) -> (f64, f64) {
    let w = (t - t.floor()) as f32 * std::f32::consts::TAU;
    let (s, c) = w.sin_cos();
    (s as f64, c as f64)
}

/// sin and cos of a phase in turns (0 ≤ t < 1 or a little outside), polynomial: wrapped to a quarter
/// turn, degree 9 / 10 (error < 5e−7). The decimated partial loops' mixer.
#[inline]
fn poly_sin_cos(t: f64) -> (f64, f64) {
    #[inline]
    fn cos_q(t: f32) -> f32 {
        // cos(2π t), |t| ≤ ½
        let mut y = t.abs();
        let neg = y > 0.25;
        if neg {
            y = 0.5 - y;
        }
        let w = y * std::f32::consts::TAU;
        let z = w * w;
        let c = 1.0 + z * (-0.5 + z * (1.0 / 24.0 + z * (-1.0 / 720.0 + z * (1.0 / 40320.0 - z * (1.0 / 3_628_800.0)))));
        if neg {
            -c
        } else {
            c
        }
    }
    let x = (t - t.round()) as f32;
    let xs = x - 0.25;
    let xs = if xs < -0.5 { xs + 1.0 } else { xs };
    (cos_q(xs) as f64, cos_q(x) as f64)
}

#[inline]
fn wrap_half(t: f64) -> f64 {
    t - t.round()
}

#[inline]
fn cents(a: f64, b: f64) -> f64 {
    1200.0 * (a / b).log2()
}

/// A complex demodulator: x · e^(−j·2π·φ) through `n` cascaded complex one-poles; error and level.
#[derive(Clone, Copy, Debug, Default)]
struct Demod {
    re: [f64; 4],
    im: [f64; 4],
    /// the phasor of the error, averaged over 50 ms (its length → the circular variance)
    ur: f64,
    ui: f64,
    /// the same over 10 ms (engage: a slow rotation of a few cents does not count)
    vr: f64,
    vi: f64,
    e: f64,
    d: f64,
    /// z / |z|
    pr: f64,
    pi: f64,
}

impl Demod {
    fn reset(&mut self) {
        *self = Self::default();
    }

    #[inline]
    fn step(&mut self, x: f64, phase: f64, n: usize, a: f64) {
        let (s, c) = sin_cos_turns(phase);
        let (mut r, mut i) = (x * c, -x * s);
        for q in 0..n {
            self.re[q] += a * (r - self.re[q]);
            self.im[q] += a * (i - self.im[q]);
            r = self.re[q];
            i = self.im[q];
        }
        self.set(r, i);
    }

    /// Updates the error phasor's averages: the demodulator's unit phasor, turned back by `off`
    /// turns (the Keep offset, × the reference's partial number).
    #[inline]
    fn track_var(&mut self, off: f64, a_var: f64, a_eng: f64) {
        let (mut c, mut s) = (self.pr, self.pi);
        if off != 0.0 {
            let (so, co) = sin_cos_turns(-off);
            (c, s) = (c * co - s * so, c * so + s * co);
        }
        self.ur += a_var * (c - self.ur);
        self.ui += a_var * (s - self.ui);
        self.vr += a_eng * (c - self.vr);
        self.vi += a_eng * (s - self.vi);
    }

    #[inline]
    fn circ_var_fast(&self) -> f64 {
        1.0 - self.vr.hypot(self.vi)
    }

    #[inline]
    fn circ_var(&self) -> f64 {
        1.0 - self.ur.hypot(self.ui)
    }

    /// At engage: the loop error starts at 0, so both averages restart on the real axis, keeping the
    /// 10 ms one's spread (the start-up transient is behind).
    fn restart_var(&mut self) {
        let r = self.vr.hypot(self.vi);
        self.ur = r;
        self.ui = 0.0;
        self.vr = r;
        self.vi = 0.0;
    }

    /// Turns the filter states by `turns` and re-reads the error (the decimated partial loops' Snap).
    fn rotate(&mut self, turns: f64) {
        let (s, c) = (2.0 * PI * turns).sin_cos();
        for q in 0..4 {
            let (a, b) = (self.re[q], self.im[q]);
            self.re[q] = a * c - b * s;
            self.im[q] = a * s + b * c;
        }
        self.set(self.re[3], self.im[3]);
    }

    /// Level and error from an already filtered baseband value.
    #[inline]
    fn set(&mut self, r: f64, i: f64) {
        let (zr, zi) = (2.0 * r, 2.0 * i);
        self.d = (zr * zr + zi * zi).sqrt();
        if self.d > 1e-12 {
            self.e = zi.atan2(zr) * (0.5 / PI);
            let inv = 1.0 / self.d;
            self.pr = zr * inv;
            self.pi = zi * inv;
        }
    }
}

/// A one-period moving average of a complex signal, fractional length: zeros on every harmonic of
/// the period, half a period of delay. The running sum is recomputed exactly once per buffer turn.
#[derive(Clone, Debug, Default)]
struct Comb {
    re: Vec<f64>,
    im: Vec<f64>,
    at: usize,
    n: usize,
    frac: f64,
    len: f64,
    sr: f64,
    si: f64,
}

impl Comb {
    fn prepare(&mut self, max: usize) {
        self.re = vec![0.0; max + 2];
        self.im = vec![0.0; max + 2];
        self.reset();
    }

    fn reset(&mut self) {
        self.re.iter_mut().for_each(|v| *v = 0.0);
        self.im.iter_mut().for_each(|v| *v = 0.0);
        self.at = 0;
        self.n = 1;
        self.frac = 0.0;
        self.len = 1.0;
        self.sr = 0.0;
        self.si = 0.0;
    }

    /// The sample of age `j` (0 = the newest).
    #[inline]
    fn age(&self, j: usize) -> usize {
        // j < m − 1: one conditional wrap, no division
        let m = self.re.len();
        let k = self.at + m - 1 - j;
        if k >= m {
            k - m
        } else {
            k
        }
    }

    /// Rotates every stored value by `turns` (a phase jump of the demodulating NCO).
    fn rotate(&mut self, turns: f64) {
        let (s, c) = (2.0 * PI * turns).sin_cos();
        for (r, i) in self.re.iter_mut().zip(self.im.iter_mut()) {
            let (a, b) = (*r, *i);
            *r = a * c - b * s;
            *i = a * s + b * c;
        }
        let (a, b) = (self.sr, self.si);
        self.sr = a * c - b * s;
        self.si = a * s + b * c;
    }

    /// The window length (samples, fractional).
    fn set_len(&mut self, len: f64) {
        let len = len.clamp(2.0, (self.re.len() - 2) as f64);
        let n = len.floor() as usize;
        while self.n < n {
            let k = self.age(self.n);
            self.sr += self.re[k];
            self.si += self.im[k];
            self.n += 1;
        }
        while self.n > n {
            self.n -= 1;
            let k = self.age(self.n);
            self.sr -= self.re[k];
            self.si -= self.im[k];
        }
        self.frac = len - n as f64;
        self.len = len;
    }

    #[inline]
    fn push(&mut self, r: f64, i: f64) -> (f64, f64) {
        self.re[self.at] = r;
        self.im[self.at] = i;
        self.at += 1;
        if self.at == self.re.len() {
            self.at = 0;
        }
        let k = self.age(self.n);
        self.sr += r - self.re[k];
        self.si += i - self.im[k];
        if self.at == 0 {
            // exact once per turn: no drift of the running sums
            let (mut a, mut b) = (0.0, 0.0);
            for j in 0..self.n {
                let q = self.age(j);
                a += self.re[q];
                b += self.im[q];
            }
            self.sr = a;
            self.si = b;
        }
        let inv = 1.0 / self.len;
        ((self.sr + self.frac * self.re[k]) * inv, (self.si + self.frac * self.im[k]) * inv)
    }
}

/// Partial k ≥ 2: its phase, trim ratio and loop.
#[derive(Clone, Copy, Debug, Default)]
struct Partial {
    k: f64,
    phi: f64,
    inc: f64,
    /// integral part of the trim (ratio − 1) and the whole trim now
    integ: f64,
    trim: f64,
    /// Keep: the offset still to pull in (turns)
    offset: f64,
    engaged: bool,
    locked: bool,
    dm: Demod,
    /// decimated path: the error at the last update, and the frequency offset it shows (turns per
    /// sample) before the loop engages (the trim starts there)
    e_prev: f64,
    f_est: f64,
    /// the error's increment per update as a unit phasor, averaged like the 10 ms engage average: its
    /// spread does not grow with a steady rotation (a partial a few hertz off its k · ψ)
    wr: f64,
    wi: f64,
    /// decimated path: the string's stretch predicted for this partial, √((1 + B k²) / (1 + B)) − 1;
    /// the trim (±15 ¢) runs around it
    pr: f64,
    /// decimated path: e^(j2π φ) carried by a rotator between updates (re-anchored on φ at each)
    zr: f64,
    zi: f64,
    rr: f64,
    ri: f64,
}

/// One harmonic series: a reference partial and K trims.
#[derive(Clone, Debug)]
pub struct Tracker {
    sr: f64,
    // feed-forward
    ff_target: f64,
    ff: f64,
    ff_a: f64,
    has_ff: bool,
    // coefficients (for `tuned`)
    tuned: f64,
    lp_part: f64,
    kp: f64,
    ki: f64,
    kp_p: f64,
    ki_p: f64,
    a_var: f64,
    a_eng: f64,
    pull: f64,
    // reference loop
    k_ref: f64,
    psi: f64,
    inc: f64,
    integ: f64,
    offset: f64,
    dm: Demod,
    comb: Comb,
    engaged: bool,
    locked: bool,
    mode: Engage,
    floor: f64,
    /// samples since the feed-forward (re)started
    since: f64,
    // hold
    held: bool,
    latched: bool,
    hold_inc: f64,
    chunks: Vec<f64>,
    chunk_at: usize,
    chunk_acc: f64,
    chunk_n: usize,
    chunk_fill: usize,
    chunk_sum: f64,
    // partials
    parts: Vec<Partial>,
    n_parts: usize,
    // decimated partial loops (opt-in): the cap, the step now, its counter and coefficients
    p_rate: usize,
    p_n: usize,
    p_ctr: usize,
    lp_part_n: f64,
    a_var_n: f64,
    a_eng_n: f64,
    // hold margin (opt-in): the level the history and the trims learn above (0 = off)
    learn_min: f64,
    margin: f64,
    // narrow mode (opt-in): periods spanned by the comb; bandwidths divided by it
    narrow: f64,
    // decimated path: the string's inharmonicity B, fitted from the locked partials
    b_hat: f64,
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Tracker {
    pub fn new() -> Self {
        Self {
            sr: 48000.0,
            ff_target: 0.0,
            ff: 0.0,
            ff_a: 0.0,
            has_ff: false,
            tuned: 0.0,
            lp_part: 0.0,
            kp: 0.0,
            ki: 0.0,
            kp_p: 0.0,
            ki_p: 0.0,
            a_var: 0.0,
            a_eng: 0.0,
            pull: 0.0,
            k_ref: 1.0,
            psi: 0.0,
            inc: 0.0,
            integ: 0.0,
            offset: 0.0,
            dm: Demod::default(),
            comb: Comb::default(),
            engaged: false,
            locked: false,
            mode: Engage::Keep,
            floor: 1e-4,
            since: 0.0,
            held: false,
            latched: false,
            hold_inc: 0.0,
            chunks: Vec::new(),
            chunk_at: 0,
            chunk_acc: 0.0,
            chunk_n: 0,
            chunk_fill: 0,
            chunk_sum: 0.0,
            parts: Vec::new(),
            n_parts: 0,
            p_rate: 1,
            p_n: 1,
            p_ctr: 0,
            lp_part_n: 0.0,
            a_var_n: 0.0,
            a_eng_n: 0.0,
            learn_min: 0.0,
            margin: 0.0,
            narrow: 1.0,
            b_hat: 0.0,
        }
    }

    /// Sizes memory: the 150 ms hold history and `max_partials` trims (0 = reference only).
    pub fn prepare(&mut self, sample_rate: f32, max_partials: usize) {
        self.sr = sample_rate as f64;
        let n = ((HOLD_MS * 1e-3 * self.sr) / CHUNK as f64).ceil().max(1.0) as usize;
        self.chunks = vec![0.0; n];
        self.parts = vec![Partial::default(); max_partials];
        self.comb.prepare((self.sr / F_LOWEST).ceil() as usize + 4);
        self.n_parts = max_partials;
        self.ff_a = 1.0 - (-1.0 / (FF_MS * 1e-3 * self.sr)).exp();
        self.a_var = 1.0 - (-1.0 / (VAR_MS * 1e-3 * self.sr)).exp();
        self.a_eng = 1.0 - (-1.0 / (ENG_MS * 1e-3 * self.sr)).exp();
        self.reset();
    }

    /// Back to idle: no feed-forward, ψ = 0, every state cleared.
    pub fn reset(&mut self) {
        self.ff_target = 0.0;
        self.ff = 0.0;
        self.has_ff = false;
        self.tuned = 0.0;
        self.psi = 0.0;
        self.inc = 0.0;
        self.integ = 0.0;
        self.offset = 0.0;
        self.dm.reset();
        self.comb.reset();
        self.engaged = false;
        self.locked = false;
        self.since = 0.0;
        self.held = false;
        self.latched = false;
        self.hold_inc = 0.0;
        self.chunks.iter_mut().for_each(|v| *v = 0.0);
        self.chunk_at = 0;
        self.chunk_acc = 0.0;
        self.chunk_n = 0;
        self.chunk_fill = 0;
        self.chunk_sum = 0.0;
        for (i, p) in self.parts.iter_mut().enumerate() {
            *p = Partial { k: (i + 2) as f64, ..Partial::default() };
        }
    }

    /// The feed-forward smoothing time (ms; 30 by default).
    pub fn set_ff_time(&mut self, ms: f32) {
        self.ff_a = 1.0 - (-1.0 / ((ms as f64).max(0.1) * 1e-3 * self.sr)).exp();
    }

    /// Opt-in: the partials' loops run every n samples at most (1 = every sample, the default path;
    /// capped at 16). The step adapts to the note: fs / step ≥ 4 f0.
    pub fn set_partial_rate(&mut self, n: usize) {
        self.p_rate = n.clamp(1, 16);
        self.p_ctr = 0;
        if self.ff > 0.0 {
            self.retune(true);
        }
    }

    /// How many trims run (≤ `max_partials`): partials 2 … n + 1.
    pub fn set_partials(&mut self, n: usize) {
        let n = n.min(self.parts.len());
        for p in self.parts[n.min(self.n_parts)..].iter_mut() {
            let k = p.k;
            *p = Partial { k, ..Partial::default() };
        }
        self.n_parts = n;
    }

    /// The reference loop's partial (1 = the fundamental; 2 or 3 for a weak fundamental).
    pub fn set_reference(&mut self, k: usize) {
        let k = k.max(1) as f64;
        if k != self.k_ref {
            self.k_ref = k;
            self.dm.reset();
            self.comb.reset();
            self.release();
            self.since = 0.0;
        }
    }

    /// The hold level (dBFS of the partial's amplitude): under it a partial is not tracked.
    pub fn set_floor(&mut self, dbfs: f32) {
        self.floor = 10f64.powf(dbfs as f64 / 20.0);
        if self.margin > 0.0 {
            self.learn_min = self.floor * self.margin;
        }
    }

    /// Opt-in, for a note inside a chord: the reference's comb spans `n` periods (zeros every f0 / n:
    /// a fifth's partials leak 13 dB through the one-period comb, none through two; a major third's
    /// through four), and the loop bandwidth, the partials' corners and their loops are divided by `n`
    /// (1 = the default), and the error's variance windows multiplied by it. Takes effect at the next
    /// note fed forward. The comb holds n periods of
    /// `f_min` only after [`Self::prepare_narrow`].
    pub fn set_narrow(&mut self, n: usize) {
        self.narrow = n.clamp(1, 8) as f64;
        // the error's variance windows (engage, loss) lengthen with the comb: a noise band n times
        // narrower wanders n times slower
        self.a_var = 1.0 - (-1.0 / (VAR_MS * self.narrow * 1e-3 * self.sr)).exp();
        self.a_eng = 1.0 - (-1.0 / (ENG_MS * self.narrow * 1e-3 * self.sr)).exp();
        if self.ff > 0.0 {
            self.retune(true);
        }
    }

    /// Sizes the comb for `max_n` periods of `f_min` (allocates: call after `prepare`).
    pub fn prepare_narrow(&mut self, max_n: usize, f_min: f32) {
        let need = (max_n.max(1) as f64 * self.sr / (f_min as f64).max(1.0)).ceil() as usize + 4;
        let have = (self.sr / F_LOWEST).ceil() as usize + 4;
        self.comb.prepare(need.max(have));
    }

    /// Opt-in: the 150 ms frequency history and the decimated partials' trim integrators learn only
    /// while the level is `db` above the hold level (`None` = always, the default).
    pub fn set_hold_margin(&mut self, db: Option<f32>) {
        match db {
            Some(d) => {
                self.margin = 10f64.powf(d.max(0.0) as f64 / 20.0);
                self.learn_min = self.floor * self.margin;
            }
            None => {
                self.margin = 0.0;
                self.learn_min = 0.0;
            }
        }
    }

    /// The rule for the next engagements.
    pub fn engage(&mut self, mode: Engage) {
        self.mode = mode;
    }

    /// The feed-forward (control rate). Unconfident readings are ignored: the last one holds.
    pub fn set_f0(&mut self, hz: f32, confident: bool) {
        let hz = hz as f64;
        if !confident || hz.is_nan() || hz <= 1.0 || hz >= 0.45 * self.sr {
            return;
        }
        // a new note is a jump between successive readings (a vibrato moves a few cents per reading)
        let new_note = !self.has_ff || cents(hz, self.ff_target).abs() > NEW_NOTE_CENTS;
        self.ff_target = hz;
        if !self.has_ff {
            self.has_ff = true;
            self.ff = hz;
            self.inc = hz / self.sr;
        }
        if new_note {
            // a new note: the loop lets go, the NCO moves to the new feed-forward (phase continuous)
            self.release();
            self.held = false;
            self.latched = false;
            self.ff = hz;
            self.inc = hz / self.sr;
            self.since = 0.0;
            self.dm.reset();
            self.comb.reset();
            self.comb.set_len(self.narrow * self.sr / hz);
            self.b_hat = 0.0;
            for p in self.parts.iter_mut() {
                p.dm.reset();
                p.e_prev = 0.0;
                p.f_est = 0.0;
                p.wr = 0.0;
                p.wi = 0.0;
                p.pr = 0.0;
                p.integ = 0.0;
                p.trim = 0.0;
                p.engaged = false;
                p.locked = false;
            }
        }
        self.retune(true);
    }

    /// Every frequency frozen at its 150 ms mean (Release, lock lost). Latched until a new note is fed
    /// forward or `reset`.
    pub fn hold(&mut self) {
        if !self.held {
            self.hold_inc = self.mean_inc();
        }
        self.release();
        self.held = true;
        self.latched = true;
    }

    /// Lifts `hold`'s latch: the loop may engage again on the note it holds (a re-picked note; a new
    /// note lifts it by itself).
    pub fn resume(&mut self) {
        if self.latched {
            self.latched = false;
            self.since = 0.0;
        }
    }

    /// The feed-forward now (Hz, smoothed), 0 before any.
    pub fn ff_hz(&self) -> f64 {
        if self.has_ff {
            self.ff
        } else {
            0.0
        }
    }

    /// The last reference error (turns) and the error's circular variance over 50 ms.
    pub fn error(&self) -> (f64, f64) {
        (self.dm.e - self.offset, self.dm.circ_var())
    }

    fn release(&mut self) {
        self.engaged = false;
        self.locked = false;
        self.integ = 0.0;
        self.offset = 0.0;
        for p in self.parts.iter_mut() {
            p.engaged = false;
            p.locked = false;
            p.offset = 0.0;
        }
    }

    fn mean_inc(&self) -> f64 {
        if self.chunk_fill == 0 {
            return self.inc;
        }
        self.chunk_sum / (self.chunk_fill * CHUNK) as f64
    }

    fn retune(&mut self, force: bool) {
        let f0 = self.ff;
        if f0 <= 0.0 || (!force && (f0 / self.tuned - 1.0).abs() < RETUNE) {
            return;
        }
        self.tuned = f0;
        let sr = self.sr;
        self.lp_part = 1.0 - (-2.0 * PI * (f0 * PART_LP / self.narrow).min(0.2 * sr) / sr).exp();
        let bw = (f0 / BW_DIV).clamp(BW_MIN, BW_MAX) / self.narrow;
        let wn = 2.0 * PI * bw / 2.06;
        self.kp = 2.0 * ZETA * wn / sr;
        self.ki = wn * wn / (sr * sr);
        let wp = 2.0 * PI * PART_BW / self.narrow / 2.06;
        self.kp_p = 2.0 * ZETA * wp / sr;
        self.ki_p = wp * wp / (sr * sr);
        self.pull = f0 * (2f64.powf(PULL_CENTS / 1200.0) - 1.0) / sr;
        if self.p_rate > 1 {
            let n = ((sr / (4.0 * f0)).floor() as usize).clamp(1, self.p_rate);
            self.p_n = n;
            let nf = n as i32;
            self.lp_part_n = 1.0 - (1.0 - self.lp_part).powi(nf);
            self.a_var_n = 1.0 - (1.0 - self.a_var).powi(nf);
            self.a_eng_n = 1.0 - (1.0 - self.a_eng).powi(nf);
        }
    }

    /// One input sample: the reference partial's state.
    #[inline]
    pub fn process(&mut self, x: f32) -> Lock {
        let x = x as f64;
        if !self.has_ff {
            return Lock::default();
        }
        // feed-forward smoothing, coefficients at control rate
        self.ff += self.ff_a * (self.ff_target - self.ff);
        self.since += 1.0;
        if (self.since as u64).is_multiple_of(CHUNK as u64) {
            self.retune(false);
            // the comb spans one period of the NCO (the fundamental): zeros on every harmonic
            let inc = if self.inc > 0.0 { self.inc } else { self.ff / self.sr };
            self.comb.set_len(self.narrow / inc);
        }
        let ff_inc = self.ff / self.sr;
        // the reference demodulator at k_ref · ψ
        let kr = self.k_ref;
        let (s, c) = sin_cos_turns((kr * self.psi).fract());
        let (br, bi) = self.comb.push(x * c, -x * s);
        self.dm.set(br, bi);
        let e = self.dm.e / kr;
        let el = wrap_half(e - self.offset);
        self.dm.track_var(self.offset * kr, self.a_var, self.a_eng);
        let var = self.dm.circ_var();
        let var_eng = self.dm.circ_var_fast();
        let loud = self.dm.d >= self.floor;
        let lo = ff_inc * 2f64.powf(-CORR_ST / 12.0);
        let hi = ff_inc * 2f64.powf(CORR_ST / 12.0);

        if self.engaged {
            // pull the Keep offset in at most 5 ¢
            if self.offset != 0.0 {
                let step = self.pull.min(self.offset.abs());
                self.offset -= step.copysign(self.offset);
            }
            // the integrator holds the whole frequency once engaged: the feed-forward (late by YIN's
            // window and its smoothing) only bounds it, else its lag would be a disturbance as large
            // as the vibrato it trails
            self.integ = (self.integ + self.ki * el).clamp(lo, hi);
            self.inc = (self.integ + self.kp * el).clamp(lo, hi);
            if !loud || var > VAR_LOSE {
                // lost: the NCO runs at its 150 ms mean
                self.hold_inc = self.mean_inc();
                self.held = true;
                self.release();
            } else {
                self.locked = true;
            }
        } else {
            self.inc = if self.held { self.hold_inc } else { ff_inc };
            let settled = self.since >= (SETTLE_PERIODS + self.narrow - 1.0) * self.sr / self.ff && var_eng < VAR_ENGAGE;
            if !self.latched && loud && settled {
                match self.mode {
                    Engage::Snap => {
                        self.psi = (self.psi + e).rem_euclid(1.0);
                        self.offset = 0.0;
                        // the comb's contents were demodulated at the old phase
                        self.comb.rotate(-e * kr);
                        let (r, i) = (self.comb.sr / self.comb.len, self.comb.si / self.comb.len);
                        self.dm.set(r, i);
                    }
                    Engage::Keep => self.offset = e,
                }
                // the 50 ms average restarts from the 10 ms one (the start-up transient is behind)
                self.dm.restart_var();
                // the loop starts from the frequency the NCO runs at: no step
                self.integ = self.inc.clamp(lo, hi);
                self.held = false;
                self.engaged = true;
                self.locked = true;
            }
        }
        self.psi = (self.psi + self.inc).rem_euclid(1.0);

        // 150 ms history of the increment (only while not holding; with a hold margin, only while the
        // reference is well above the hold level)
        if !self.held && (self.learn_min <= 0.0 || self.dm.d >= self.learn_min) {
            self.chunk_acc += self.inc;
            self.chunk_n += 1;
            if self.chunk_n == CHUNK {
                let old = self.chunks[self.chunk_at];
                self.chunks[self.chunk_at] = self.chunk_acc;
                self.chunk_sum += self.chunk_acc - if self.chunk_fill == self.chunks.len() { old } else { 0.0 };
                self.chunk_fill = (self.chunk_fill + 1).min(self.chunks.len());
                self.chunk_at = (self.chunk_at + 1) % self.chunks.len();
                self.chunk_acc = 0.0;
                self.chunk_n = 0;
            }
        }

        // partials
        if self.p_rate > 1 {
            self.step_partials_decimated(x);
        } else {
            for i in 0..self.n_parts {
                self.step_partial(i, x);
            }
        }
        Lock { psi: self.psi, inc: self.inc, level: self.dm.d as f32, locked: self.locked }
    }

    #[inline]
    fn step_partial(&mut self, i: usize, x: f64) {
        let (inc_ref, ff, sr, floor, a_var, a_eng) = (self.inc, self.ff, self.sr, self.floor, self.a_var, self.a_eng);
        let (lp, kp, ki, mode, latched) = (self.lp_part, self.kp_p, self.ki_p, self.mode, self.latched);
        let ref_on = self.engaged || self.held;
        let p = &mut self.parts[i];
        if p.k * ff >= 0.45 * sr {
            p.locked = false;
            p.inc = 0.0;
            return;
        }
        p.dm.step(x, p.phi, 4, lp);
        let el = wrap_half(p.dm.e - p.offset);
        p.dm.track_var(p.offset, a_var, a_eng);
        let var = p.dm.circ_var();
        let var_eng = p.dm.circ_var_fast();
        let loud = p.dm.d >= floor;
        let tb = 2f64.powf(TRIM_CENTS / 1200.0) - 1.0;
        let base = p.k * ff / sr;
        if p.engaged && !self.held {
            if p.offset != 0.0 {
                let step = (self.pull * p.k).min(p.offset.abs());
                p.offset -= step.copysign(p.offset);
            }
            p.integ = (p.integ + ki * el / base).clamp(-tb, tb);
            p.trim = (p.integ + kp * el / base).clamp(-tb, tb);
            if !loud || var > VAR_LOSE {
                p.engaged = false;
                p.locked = false;
                p.offset = 0.0;
                // the trim freezes at its integral
                p.trim = p.integ;
            } else {
                p.locked = true;
            }
        } else if ref_on && !latched && !self.held && loud && var_eng < VAR_ENGAGE {
            match mode {
                Engage::Snap => {
                    p.phi = (p.phi + p.dm.e).rem_euclid(1.0);
                    p.offset = 0.0;
                    p.dm.reset();
                }
                Engage::Keep => p.offset = p.dm.e,
            }
            p.dm.restart_var();
            p.engaged = true;
            p.locked = true;
        }
        p.inc = p.k * inc_ref * (1.0 + p.trim);
        p.phi = (p.phi + p.inc).rem_euclid(1.0);
    }

    /// Decimated path: fits B from the locked partials (B_k = ((1 + t_k)² − 1) / (k² − 1), t_k the
    /// partial's whole ratio − 1, weighted by k² − 1), smoothed, and moves every partial's predicted
    /// stretch to it (an engaged partial keeps its frequency: its trim takes the difference). A wound
    /// string's partial 8 is 16 ¢ sharp at B = 3·10⁻⁴, partial 16 64 ¢: beyond a ±15 ¢ trim alone.
    fn fit_stretch(&mut self, learn_min: f64) {
        let (mut num, mut den) = (0.0f64, 0.0f64);
        for p in self.parts[..self.n_parts].iter() {
            if !p.locked || (learn_min > 0.0 && p.dm.d < learn_min) {
                continue;
            }
            let t = 1.0 + p.pr + p.integ;
            let w = p.k * p.k - 1.0;
            num += t * t - 1.0;
            den += w;
        }
        if den <= 0.0 {
            return;
        }
        let b = (self.b_hat + STRETCH_A * (num / den - self.b_hat)).clamp(0.0, 1e-3);
        if b == self.b_hat {
            return;
        }
        self.b_hat = b;
        let tb = 2f64.powf(TRIM_CENTS / 1200.0) - 1.0;
        for p in self.parts.iter_mut() {
            let pr = ((1.0 + b * p.k * p.k) / (1.0 + b)).sqrt() - 1.0;
            if p.engaged {
                let d = pr - p.pr;
                p.integ = (p.integ - d).clamp(-tb, tb);
                p.trim = (p.trim - d).clamp(-tb, tb);
            }
            p.pr = pr;
        }
    }

    /// Partial k's cos(2π φ_k) after the last sample, from the decimated path's phasor (k ≥ 2 with
    /// [`Self::set_partial_rate`] > 1; within 10⁻⁵ of the exact value). None otherwise.
    #[inline]
    pub fn partial_cos(&self, k: usize) -> Option<f64> {
        if self.p_rate <= 1 || k < 2 || k - 2 >= self.n_parts {
            return None;
        }
        Some(self.parts[k - 2].zr)
    }

    /// The string's inharmonicity as fitted by the decimated partial loops (0 otherwise).
    pub fn stretch_b(&self) -> f64 {
        self.b_hat
    }

    /// The opt-in decimated partial loops ([`Self::set_partial_rate`]).
    #[inline]
    fn step_partials_decimated(&mut self, x: f64) {
        let n = self.p_n.max(1);
        self.p_ctr += 1;
        let upd = self.p_ctr >= n;
        if upd {
            self.p_ctr = 0;
        }
        let (inc_ref, ff, sr, floor) = (self.inc, self.ff, self.sr, self.floor);
        let (lp, lpn, a_var, a_eng) = (self.lp_part, self.lp_part_n, self.a_var_n, self.a_eng_n);
        let (kp, ki, mode, latched, held, pull) = (self.kp_p, self.ki_p * n as f64, self.mode, self.latched, self.held, self.pull * n as f64);
        let ref_on = self.engaged || self.held;
        let tb = 2f64.powf(TRIM_CENTS / 1200.0) - 1.0;
        let learn_min = self.learn_min;
        if upd && !held {
            self.fit_stretch(learn_min);
        }
        for p in self.parts[..self.n_parts].iter_mut() {
            if p.k * ff >= 0.45 * sr {
                p.locked = false;
                p.inc = 0.0;
                continue;
            }
            // the mixer's phasor: exact at each update, rotated by the update's increment in between (the
            // increment moves by the reference's correction only: a drift under 10⁻⁵ turn)
            if upd || p.rr == 0.0 {
                let (s0, c0) = poly_sin_cos(p.phi);
                p.zr = c0;
                p.zi = s0;
                let (s1, c1) = poly_sin_cos(p.k * inc_ref * (1.0 + p.pr + p.trim));
                p.rr = c1;
                p.ri = s1;
            }
            let (s, c) = (p.zi, p.zr);
            let (r, i) = (x * c, -x * s);
            let d = &mut p.dm;
            d.re[0] += lp * (r - d.re[0]);
            d.im[0] += lp * (i - d.im[0]);
            d.re[1] += lp * (d.re[0] - d.re[1]);
            d.im[1] += lp * (d.im[0] - d.im[1]);
            if upd {
                let (mut r, mut i) = (d.re[1], d.im[1]);
                for q in 2..4 {
                    d.re[q] += lpn * (r - d.re[q]);
                    d.im[q] += lpn * (i - d.im[q]);
                    r = d.re[q];
                    i = d.im[q];
                }
                d.set(r, i);
                let el = wrap_half(d.e - p.offset);
                // the variance averages (as `track_var`, with the polynomial turn and plain roots: this
                // runs for every partial at every update)
                let (mut uc, mut us) = (d.pr, d.pi);
                if p.offset != 0.0 {
                    let (so, co) = poly_sin_cos(-p.offset);
                    (uc, us) = (uc * co - us * so, uc * so + us * co);
                }
                d.ur += a_var * (uc - d.ur);
                d.ui += a_var * (us - d.ui);
                d.vr += a_eng * (uc - d.vr);
                d.vi += a_eng * (us - d.vi);
                let var = 1.0 - (d.ur * d.ur + d.ui * d.ui).sqrt();
                let var_eng = 1.0 - (d.vr * d.vr + d.vi * d.vi).sqrt();
                let loud = d.d >= floor;
                let base = p.k * ff / sr;
                // the error's rotation before the loop engages: the partial's frequency offset
                let step = wrap_half(d.e - p.e_prev);
                let de = step / n as f64;
                p.e_prev = d.e;
                p.f_est += ACQ_A * (de - p.f_est);
                let (ss, sc) = poly_sin_cos(step);
                p.wr += a_eng * (sc - p.wr);
                p.wi += a_eng * (ss - p.wi);
                let var_inc = 1.0 - (p.wr * p.wr + p.wi * p.wi).sqrt();
                if p.engaged && !held {
                    if p.offset != 0.0 {
                        let step = (pull * p.k).min(p.offset.abs());
                        p.offset -= step.copysign(p.offset);
                    }
                    if learn_min <= 0.0 || d.d >= learn_min {
                        p.integ = (p.integ + ki * el / base).clamp(-tb, tb);
                    }
                    p.trim = (p.integ + kp * el / base).clamp(-tb, tb);
                    if !loud || var > VAR_LOSE {
                        p.engaged = false;
                        p.locked = false;
                        p.offset = 0.0;
                        p.trim = p.integ;
                    } else {
                        p.locked = true;
                    }
                } else if ref_on && !latched && !held && loud && var_eng.min(var_inc) < VAR_ENGAGE {
                    match mode {
                        Engage::Snap => {
                            // the phase jumps onto the input; the demodulator's states (made at the old
                            // phase) turn with it, so the error reads 0 at once and the variance
                            // averages keep their spread (clearing them read as a lost lock)
                            let e = p.dm.e;
                            p.phi = (p.phi + e).rem_euclid(1.0);
                            p.offset = 0.0;
                            p.dm.rotate(-e);
                            p.e_prev = 0.0;
                            // the phasor follows the jump
                            let (s0, c0) = poly_sin_cos(p.phi);
                            p.zr = c0;
                            p.zi = s0;
                        }
                        Engage::Keep => p.offset = p.dm.e,
                    }
                    p.dm.restart_var();
                    // frequency-aided acquisition: the trim starts at the offset the error showed (a
                    // 4 Hz loop alone loses a partial 6 Hz away, k = 5 of a wound E4)
                    // (only with a good signal: under the hold margin the trim stays what it learned)
                    if learn_min <= 0.0 || p.dm.d >= learn_min {
                        p.integ = (p.integ + p.f_est / base).clamp(-tb, tb);
                    }
                    p.trim = p.integ;
                    if mode == Engage::Keep {
                        p.e_prev = p.dm.e;
                    }
                    p.engaged = true;
                    p.locked = true;
                }
            }
            if held && p.trim != p.integ {
                // held (Release, lock lost): the trim's proportional part goes, its integral stays
                p.trim = p.integ;
            }
            p.inc = p.k * inc_ref * (1.0 + p.pr + p.trim);
            p.phi += p.inc;
            if p.phi >= 1.0 {
                p.phi -= 1.0;
            }
            let (zr, zi) = (p.zr * p.rr - p.zi * p.ri, p.zr * p.ri + p.zi * p.rr);
            p.zr = zr;
            p.zi = zi;
        }
    }

    /// Partial k (2 … K + 1) after the last sample: its phase k · ψ + φ_k, increment, level, lock.
    pub fn partial(&self, k: usize) -> Lock {
        if k < 2 || k - 2 >= self.n_parts {
            return Lock::default();
        }
        let p = &self.parts[k - 2];
        Lock { psi: p.phi, inc: p.inc, level: p.dm.d as f32, locked: p.locked }
    }

    /// Partial k's trim: (integral part, whole trim), ratios − 1 (diagnostics).
    pub fn partial_trim(&self, k: usize) -> (f64, f64) {
        if k < 2 || k - 2 >= self.n_parts {
            return (0.0, 0.0);
        }
        let p = &self.parts[k - 2];
        (p.pr + p.integ, p.pr + p.trim)
    }

    /// The reference's state after the last sample.
    pub fn reference(&self) -> Lock {
        Lock { psi: self.psi, inc: self.inc, level: self.dm.d as f32, locked: self.locked }
    }

    /// Holding a frequency (lock lost, or `hold`).
    pub fn is_held(&self) -> bool {
        self.held
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48000.0;

    /// A harmonic tone with a pitch trajectory: `amps` per partial, instantaneous f0 per sample;
    /// returns (signal, phase of the fundamental in turns per sample).
    fn tone(f0: &[f64], amps: &[f64], sr: f32) -> (Vec<f32>, Vec<f64>) {
        let mut ph = 0.0f64;
        let mut x = Vec::with_capacity(f0.len());
        let mut p = Vec::with_capacity(f0.len());
        for &f in f0 {
            let mut s = 0.0;
            for (k, a) in amps.iter().enumerate() {
                s += a * (2.0 * PI * (k + 1) as f64 * ph).cos();
            }
            x.push(s as f32);
            p.push(ph);
            ph = (ph + f / sr as f64).rem_euclid(1.0);
        }
        (x, p)
    }

    fn vibrato(n: usize, f: f64, cents: f64, rate: f64, sr: f32) -> Vec<f64> {
        (0..n).map(|i| f * 2f64.powf(cents / 1200.0 * (2.0 * PI * rate * i as f64 / sr as f64).sin())).collect()
    }

    /// Runs the tracker with a feed-forward that lags `lag` s and is off by `err` cents, updated
    /// every 2.7 ms; returns per-sample (inc · sr, ψ, locked).
    fn run(t: &mut Tracker, x: &[f32], f0: &[f64], lag: f64, err: f64, sr: f32) -> Vec<(f64, f64, bool)> {
        let hop = (0.0027 * sr) as usize;
        let lag = (lag * sr as f64) as usize;
        let mut out = Vec::with_capacity(x.len());
        for i in 0..x.len() {
            if i % hop == 0 && i >= lag {
                t.set_f0((f0[i - lag] * 2f64.powf(err / 1200.0)) as f32, true);
            }
            let l = t.process(x[i]);
            out.push((l.inc * sr as f64, l.psi, l.locked));
        }
        out
    }

    /// The tracked frequency against the truth, both through a 5 ms one-pole (cents).
    fn smooth_cents(o: &[(f64, f64, bool)], f: &[f64]) -> Vec<f64> {
        let a = 1.0 - (-1.0 / (0.005 * SR as f64)).exp();
        let (mut sa, mut sb) = (f[0], f[0]);
        o.iter()
            .zip(f)
            .map(|(s, &t)| {
                sa += a * (s.0 - sa);
                sb += a * (t - sb);
                cents(sa, sb)
            })
            .collect()
    }

    fn fresh(sr: f32, k: usize) -> Tracker {
        let mut t = Tracker::new();
        t.prepare(sr, k);
        t.set_floor(-60.0);
        t.engage(Engage::Keep);
        t
    }

    #[test]
    fn locks_on_a_steady_tone_in_phase() {
        for &sr in &[44100.0f32, 48000.0, 96000.0] {
            for &f in &[82.4, 220.0, 659.3] {
                let n = (sr * 3.0) as usize;
                let fv = vec![f; n];
                let (x, ph) = tone(&fv, &[0.5, 0.4, 0.3, 0.2, 0.1], sr);
                let mut t = fresh(sr, 0);
                let o = run(&mut t, &x, &fv, 0.025, 4.0, sr);
                let from = 2 * n / 3;
                let tail = &o[from..];
                assert!(tail.iter().all(|s| s.2), "{f} Hz at {sr}: lock held");
                // the frequency over the last second (the proportional path ripples at f0)
                let mean = tail.iter().map(|s| s.0).sum::<f64>() / tail.len() as f64;
                let worst = cents(mean, f).abs();
                assert!(worst < 0.1, "{f} Hz at {sr}: frequency within 0.1 ¢ ({worst:.3})");
                // ψ on the input's phase (the Keep offset has been pulled in)
                let pe: Vec<f64> = (from..n - 1).map(|i| wrap_half(o[i].1 - ph[i + 1]).abs() * 360.0).collect();
                let (mean, max) = (pe.iter().sum::<f64>() / pe.len() as f64, pe.iter().fold(0.0f64, |m, v| m.max(*v)));
                assert!(mean < 1.0 && max < 3.0, "{f} Hz at {sr}: ψ on the input (mean {mean:.2}°, max {max:.2}°)");
            }
        }
    }

    #[test]
    fn engage_keep_has_no_step() {
        // a sounding generator: from the feed-forward's 4 ¢ error to lock, the frequency never leaves
        // the band between the two by more than the pull rate, and ψ never jumps
        let f = 196.0;
        let n = (SR * 1.5) as usize;
        let fv = vec![f; n];
        let (x, _) = tone(&fv, &[0.5, 0.3, 0.2], SR);
        let mut t = fresh(SR, 0);
        let o = run(&mut t, &x, &fv, 0.025, 6.0, SR);
        let first = o.iter().position(|s| s.2).expect("engages");
        assert!(first as f32 / SR < 0.2, "engages within 0.2 s ({:.3})", first as f32 / SR);
        let worst = smooth_cents(&o, &vec![f; o.len()])[first..].iter().fold(0.0f64, |m, v| m.max(v.abs()));
        assert!(worst <= 6.0 + 5.0 + 0.5, "excursion at engage {worst:.2} ¢");
        for w in o.windows(2).skip(1) {
            let d = wrap_half(w[1].1 - w[0].1) * SR as f64;
            assert!((d - w[1].0).abs() < 1e-6 * f, "ψ continuous");
        }
    }

    #[test]
    fn follows_vibrato() {
        // ±50 ¢ at 6 Hz: the tracked frequency against the string's (both 5 ms smoothed)
        for &(f, bound) in &[(110.0, 5.0), (220.0, 5.0), (329.6, 5.0), (82.4, 10.0)] {
            // the Keep offset is pulled in at 5 ¢ (up to 2 s at E2): measured after it
            let n = (SR * 4.0) as usize;
            let fv = vibrato(n, f, 50.0, 6.0, SR);
            let (x, _) = tone(&fv, &[0.5, 0.4, 0.3, 0.2, 0.1], SR);
            let mut t = fresh(SR, 0);
            let o = run(&mut t, &x, &fv, 0.025, 3.0, SR);
            let sc = smooth_cents(&o, &fv);
            let mut worst = 0.0f64;
            for i in (SR * 2.5) as usize..n {
                assert!(o[i].2, "{f} Hz: lock held through the vibrato");
                worst = worst.max(sc[i].abs());
            }
            eprintln!("vibrato ±50 ¢ 6 Hz at {f} Hz: worst {worst:.2} ¢");
            assert!(worst <= bound, "{f} Hz: carrier error {worst:.2} ¢ > {bound}");
        }
    }

    #[test]
    fn holds_the_mean_when_lost() {
        // a vibrato that stops dead (the note is cut): the NCO holds the vibrato's centre
        let f = 220.0;
        let n = (SR * 2.0) as usize;
        let mut fv = vibrato(n, f, 30.0, 6.0, SR);
        let (mut x, _) = tone(&fv, &[0.5, 0.3], SR);
        let cut = (SR * 1.37) as usize;
        for v in x[cut..].iter_mut() {
            *v = 0.0;
        }
        for v in fv[cut..].iter_mut() {
            *v = f;
        }
        let mut t = fresh(SR, 0);
        let o = run(&mut t, &x, &fv[..cut].iter().chain(std::iter::repeat(&0.0)).take(n).copied().collect::<Vec<_>>(), 0.0, 0.0, SR);
        assert!(!o[n - 1].2 && t.is_held());
        let c = cents(o[n - 1].0, f);
        assert!(c.abs() < 6.0, "held within 6 ¢ of the centre ({c:.2}), the vibrato swings ±30");
    }

    #[test]
    fn partials_follow_inharmonic_string() {
        // B = 3e-4: partial k at k f0 √(1 + B k²); the trims take the stretch
        let f = 110.0;
        let b = 3e-4f64;
        let n = (SR * 3.0) as usize;
        let mut x = vec![0.0f32; n];
        for k in 1..=6usize {
            let fk = k as f64 * f * (1.0 + b * (k * k) as f64).sqrt();
            for (i, v) in x.iter_mut().enumerate() {
                *v += (0.4 / k as f64 * (2.0 * PI * fk * i as f64 / SR as f64).cos()) as f32;
            }
        }
        let f1 = f * (1.0 + b).sqrt();
        let mut t = fresh(SR, 4);
        let hop = (0.0027 * SR) as usize;
        let mut sum = [0.0f64; 4];
        let from = n - (SR * 0.5) as usize;
        for i in 0..n {
            if i % hop == 0 {
                t.set_f0(f1 as f32, true);
            }
            t.process(x[i]);
            if i >= from {
                for (k, s) in sum.iter_mut().enumerate() {
                    *s += t.partial(k + 2).inc;
                }
            }
        }
        for k in 2..=5usize {
            let l = t.partial(k);
            let fk = k as f64 * f * (1.0 + b * (k * k) as f64).sqrt();
            let c = cents(sum[k - 2] / (n - from) as f64 * SR as f64, fk);
            assert!(l.locked && c.abs() < 0.2, "partial {k}: {c:.3} ¢, locked {}", l.locked);
        }
    }

    #[test]
    fn decimated_partials_follow_inharmonic_string() {
        // the opt-in decimated loops: same string as above, same bound
        for &sr in &[44100.0f32, 48000.0, 96000.0] {
            let f = 110.0;
            let b = 3e-4f64;
            let n = (sr * 3.0) as usize;
            let mut x = vec![0.0f32; n];
            for k in 1..=6usize {
                let fk = k as f64 * f * (1.0 + b * (k * k) as f64).sqrt();
                for (i, v) in x.iter_mut().enumerate() {
                    *v += (0.4 / k as f64 * (2.0 * PI * fk * i as f64 / sr as f64).cos()) as f32;
                }
            }
            let f1 = f * (1.0 + b).sqrt();
            let mut t = fresh(sr, 4);
            t.set_partial_rate(8);
            if sr == 48000.0 {
                // Snap: the phase jumps onto each partial (a silent generator)
                t.engage(Engage::Snap);
            }
            let hop = (0.0027 * sr) as usize;
            let mut sum = [0.0f64; 4];
            let from = n - (sr * 0.5) as usize;
            for i in 0..n {
                if i % hop == 0 {
                    t.set_f0(f1 as f32, true);
                }
                t.process(x[i]);
                if i >= from {
                    for (k, s) in sum.iter_mut().enumerate() {
                        *s += t.partial(k + 2).inc;
                    }
                }
            }
            for k in 2..=5usize {
                let l = t.partial(k);
                let fk = k as f64 * f * (1.0 + b * (k * k) as f64).sqrt();
                let c = cents(sum[k - 2] / (n - from) as f64 * sr as f64, fk);
                let lv = 0.4 / k as f64;
                assert!(l.locked && c.abs() < 0.2, "{sr}: partial {k}: {c:.3} ¢, locked {}", l.locked);
                // the neighbours, f0 away, are 34 dB down: a 2 % ripple on the level
                assert!((l.level as f64 / lv - 1.0).abs() < 0.05, "{sr}: partial {k} level {:.4} vs {lv:.4}", l.level);
            }
        }
    }

    #[test]
    fn poly_sin_cos_accuracy() {
        let mut worst = 0.0f64;
        for i in -1000..=21000 {
            let t = i as f64 / 20000.0;
            let (s, c) = poly_sin_cos(t);
            let w = 2.0 * PI * t;
            worst = worst.max((s - w.sin()).abs()).max((c - w.cos()).abs());
        }
        assert!(worst < 1e-6, "{worst:e}");
    }

    #[test]
    fn weak_fundamental_reference() {
        // fundamental 24 dB under the 2nd partial: the loop on partial 2 still gives ψ = the fundamental's
        let f = 98.0;
        let n = (SR * 2.0) as usize;
        let fv = vec![f; n];
        let (x, _) = tone(&fv, &[0.03, 0.5, 0.3], SR);
        let mut t = fresh(SR, 0);
        t.set_reference(2);
        let o = run(&mut t, &x, &fv, 0.0, 3.0, SR);
        assert!(o[n - 1].2);
        assert!(cents(o[n - 1].0, f).abs() < 0.1);
    }

    #[test]
    fn snap_puts_psi_on_the_input() {
        let f = 330.0;
        let n = (SR * 0.5) as usize;
        let fv = vec![f; n];
        let (x, ph) = tone(&fv, &[0.5, 0.2], SR);
        let mut t = fresh(SR, 0);
        t.engage(Engage::Snap);
        let o = run(&mut t, &x, &fv, 0.0, 2.0, SR);
        let first = o.iter().position(|s| s.2).unwrap();
        let pe = wrap_half(o[first + 10].1 - ph[first + 11]).abs() * 360.0;
        assert!(pe < 20.0, "snapped within 20° ({pe:.1}°)");
    }

    #[test]
    fn deterministic_and_silent_safe() {
        let mut t = fresh(SR, 3);
        t.set_f0(200.0, true);
        for _ in 0..48000 {
            let l = t.process(0.0);
            assert!(l.inc.is_finite() && l.psi.is_finite() && !l.locked);
        }
        let mut a = fresh(SR, 2);
        let mut b = fresh(SR, 2);
        let fv = vec![150.0; 20000];
        let (x, _) = tone(&fv, &[0.5, 0.3, 0.2], SR);
        let oa = run(&mut a, &x, &fv, 0.0, 1.0, SR);
        let ob = run(&mut b, &x, &fv, 0.0, 1.0, SR);
        assert_eq!(oa, ob);
    }

    #[test]
    fn follows_bends() {
        // 200 ¢ up in 150 ms, held, and back: the lock holds; the error (5 ms smoothed) is the ramp's lag
        for &(f, bound) in &[(329.6, 3.0), (110.0, 10.0), (82.4, 12.0)] {
            let n = (SR * 4.0) as usize;
            let (t0, ramp) = ((SR * 2.5) as usize, (SR * 0.15) as usize);
            let fv: Vec<f64> = (0..n)
                .map(|i| {
                    let up = if i < t0 { 0.0 } else { ((i - t0) as f64 / ramp as f64).min(1.0) };
                    let down = if i < t0 + 3 * ramp { 0.0 } else { ((i - t0 - 3 * ramp) as f64 / ramp as f64).min(1.0) };
                    f * 2f64.powf(2.0 * (up - down) / 12.0)
                })
                .collect();
            let (x, _) = tone(&fv, &[0.5, 0.4, 0.3, 0.2, 0.1], SR);
            let mut t = fresh(SR, 0);
            let o = run(&mut t, &x, &fv, 0.025, 3.0, SR);
            let sc = smooth_cents(&o, &fv);
            let worst = (t0..n).map(|i| sc[i].abs()).fold(0.0, f64::max);
            assert!((t0..n).all(|i| o[i].2), "{f} Hz: lock held through the bend");
            eprintln!("bend 200 ¢ / 150 ms at {f} Hz: worst {worst:.2} ¢");
            assert!(worst <= bound, "{f} Hz: {worst:.2} ¢");
        }
    }
}
