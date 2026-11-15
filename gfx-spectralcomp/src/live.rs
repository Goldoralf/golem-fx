// SPDX-License-Identifier: GPL-3.0-or-later
//! Live: the gain curve applied with zero latency (README §2.6). 27 matched bells on the ISO
//! third-octave centres 40 Hz – 16 kHz, solved by gfx-dsp `geq` (ACGE, [`PlanSpec::THIRDS`]: no
//! fill-ins, c = 0.5, ±40 dB, 55 design points), one warm-started correction per 1.33 ms control
//! tick, coefficients ramped over the tick (`Svf::set_biquad_ramped`).
//!
//! - **Targets** (Review change 1): at each design point p, G_p = 10 log10(Σ P_k 10^(g_k / 10) /
//!   Σ P_k) over the bins within ±1/3 octave (at least the nearest bin), P_k the frame's bin power
//!   plus 1e-20, summed over each window (not by prefix differences: under a loud band they would be
//!   rounding noise where the weights sit at the floor); then T_p = ¼ G_(p−1) + ½ G_p + ¼ G_(p+1), ends repeated. A
//!   frame's targets are reached linearly over one hop from the previous frame's.
//! - **Beyond the end bells** (as coded): a matched low shelf at 30 Hz and a high shelf at 18 kHz
//!   (Q 0.71) take the end design points' targets (35.6 Hz, 17.96 kHz) as their gains, and the
//!   bells solve the targets minus the shelves' exact responses. Under 35 Hz and above 18 kHz the
//!   response is then the end target (the flat extrapolation the review's Live-against-Studio figures
//!   assume) instead of 0 dB: Live compresses sub-bass and air as Studio does, and the make-up never
//!   lifts an uncut band. The shelves run as the 28th and 29th sections.
//! - **Flat is flat** (gfx-geq's rule): every target exactly 0 → every section exactly 0 dB; after
//!   50 ms there the cascade fades out (5 ms) and is bypassed bit-exactly. It comes back from rest
//!   and fades in when a target moves off 0.

use gfx_dsp::geq::{Phi, Plan, PlanSpec, MAX_POINTS, MAX_SECTIONS, TOLERANCE_DB};
use gfx_dsp::matched::{design, Analog, Biquad};
use gfx_dsp::smooth::LinearRamp;
use gfx_dsp::svf::SvfCoefs;

/// The ISO third-octave centres 40 Hz – 16 kHz (any above 0.45 fs dropped: none from 44.1 kHz up).
pub const THIRDS: [f64; 27] = [
    40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0, 400.0, 500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3150.0, 4000.0, 5000.0,
    6300.0, 8000.0, 10000.0, 12500.0, 16000.0,
];
/// The target rule's half-width (octaves).
pub const TARGET_HALF_OCT: f64 = 1.0 / 3.0;
/// The bin-power floor of the target weights.
pub const P_FLOOR: f64 = 1e-20;
/// The control tick: 1.33 ms (64 samples at 48 kHz), defined in time (gfx-geq's).
pub const TICK_S: f64 = 1.0 / 750.0;
/// One correction every second tick, its coefficients ramped over both (README §2.9's named lever:
/// a correction of the 27-section plan costs about 30 µs).
pub const TICKS_PER_SOLVE: u32 = 2;
/// A section whose gain moves less than this (dB) in a correction keeps its design (`correct_lean`).
pub const SKIP_DB: f64 = 1e-3;
/// Corrections go on this many ticks after the target stops.
const SETTLE_TICKS: u32 = 4;
/// Every section exactly 0 dB for 50 ms: the cascade fades out (5 ms) and is bypassed.
pub const FLAT_HOLD_S: f64 = 0.05;
pub const BYPASS_FADE_MS: f32 = 5.0;

/// The shelves beyond the end bells: corners and Q.
pub const SHELF_LO_HZ: f64 = 30.0;
pub const SHELF_HI_HZ: f64 = 18000.0;
pub const SHELF_Q: f64 = 0.707;
/// Sections of a channel's cascade: the 27 bells and the two shelves.
pub const CASC: usize = MAX_SECTIONS + 2;

pub const UNITY: Biquad = Biquad { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };

/// The Live plan at a sample rate.
pub fn plan(sample_rate: f64) -> Plan {
    Plan::with_spec(&THIRDS, sample_rate, &PlanSpec::THIRDS)
}

/// One solver: the section gains of one gain curve, its targets' ramp and its bypass state. Plain
/// arrays, so a copy (a channel unlinking) allocates nothing.
#[derive(Clone, Debug)]
pub struct Slot {
    pub n: usize,
    pub np: usize,
    pub g: [f64; MAX_SECTIONS],
    pub designs: [Biquad; MAX_SECTIONS],
    /// the ramp of the targets: from the previous frame's to this frame's, over `ramp_len` samples
    /// from `ramp_start` (sample clock)
    pub t_from: [f64; MAX_POINTS],
    pub t_to: [f64; MAX_POINTS],
    pub ramp_start: u64,
    /// the target the last correction aimed at
    t: [f64; MAX_POINTS],
    still: u32,
    last_dg: f64,
    flat_ticks: u32,
    /// the cascade runs (else the slot passes its input through untouched)
    pub engaged: bool,
    /// the cascade's coefficients changed this tick: the channels ramp to `designs`
    pub changed: bool,
    /// the cascade engaged this tick from bypass: the channels start from rest on `designs`
    pub restart: bool,
    wet: LinearRamp,
    pub m: f32,
    /// the shelves beyond the end bells (low, high): their gains (the end points' targets), designs
    /// and summed response at the design points
    pub shelf_db: [f64; 2],
    pub shelf: [Biquad; 2],
    s_resp: [[f64; MAX_POINTS]; 2],
    phi: [Phi; MAX_POINTS],
    sr: f64,
    /// each section's 0 dB design (its poles, unity numerator): where an engaging cascade starts
    pub zero: [Biquad; MAX_SECTIONS],
    pub zero_shelf: [Biquad; 2],
}

impl Slot {
    pub fn new() -> Self {
        Self {
            n: 0,
            np: 0,
            g: [0.0; MAX_SECTIONS],
            designs: [UNITY; MAX_SECTIONS],
            t_from: [0.0; MAX_POINTS],
            t_to: [0.0; MAX_POINTS],
            ramp_start: 0,
            t: [0.0; MAX_POINTS],
            still: 0,
            last_dg: 0.0,
            flat_ticks: 0,
            engaged: false,
            changed: false,
            restart: false,
            wet: LinearRamp::new(BYPASS_FADE_MS, 0.0),
            m: 0.0,
            shelf_db: [0.0; 2],
            shelf: [UNITY; 2],
            s_resp: [[0.0; MAX_POINTS]; 2],
            phi: [Phi::default(); MAX_POINTS],
            sr: 48000.0,
            zero: [UNITY; MAX_SECTIONS],
            zero_shelf: [UNITY; 2],
        }
    }

    pub fn prepare(&mut self, plan: &Plan, sample_rate: f32) {
        self.n = plan.len();
        self.np = plan.points().len();
        self.sr = sample_rate as f64;
        for (k, &f) in plan.points().iter().enumerate() {
            self.phi[k] = Phi::new(f, self.sr);
        }
        for m in 0..self.n {
            self.zero[m] = plan.design(m, 0.0);
        }
        self.zero_shelf = [shelf_design(0, 0.0, self.sr), shelf_design(1, 0.0, self.sr)];
        self.wet.prepare(sample_rate);
        self.reset();
    }

    /// From rest: flat, bypassed.
    pub fn reset(&mut self) {
        self.g = [0.0; MAX_SECTIONS];
        self.designs = [UNITY; MAX_SECTIONS];
        self.t_from = [0.0; MAX_POINTS];
        self.t_to = [0.0; MAX_POINTS];
        self.ramp_start = 0;
        self.t = [0.0; MAX_POINTS];
        self.still = 0;
        self.last_dg = 0.0;
        self.flat_ticks = 0;
        self.engaged = false;
        self.changed = false;
        self.restart = false;
        self.wet.set_immediate(0.0);
        self.m = 0.0;
        self.shelf_db = [0.0; 2];
        self.shelf = self.zero_shelf;
        self.s_resp = [[0.0; MAX_POINTS]; 2];
    }

    /// A frame's new targets, reached linearly over `hop` samples from the next sample (`now` + 1),
    /// starting from where the ramp is at `now`.
    pub fn install(&mut self, t: &[f64; MAX_POINTS], now: u64, hop: usize) {
        let a = self.ramp_at(now, hop);
        for k in 0..self.np {
            self.t_from[k] = if a >= 1.0 { self.t_to[k] } else { self.t_from[k] + a * (self.t_to[k] - self.t_from[k]) };
            self.t_to[k] = t[k];
        }
        self.ramp_start = now + 1;
    }

    #[inline]
    fn ramp_at(&self, at: u64, hop: usize) -> f64 {
        if at + 1 < self.ramp_start {
            0.0
        } else {
            (((at + 1 - self.ramp_start) as f64) / hop as f64).min(1.0)
        }
    }

    fn flat(&self) -> bool {
        self.shelf_db == [0.0; 2] && self.g[..self.n].iter().all(|&v| v == 0.0)
    }

    /// One control tick of `len` samples starting at sample `now`: the target at the tick's end, a
    /// warm-started correction while it moves (and a few ticks after), the bypass fade.
    pub fn tick(&mut self, plan: &Plan, now: u64, len: u32, hop: usize, flat_hold: u32) {
        self.changed = false;
        self.restart = false;
        let a = self.ramp_at(now + len as u64 - 1, hop);
        let mut t = [0.0; MAX_POINTS];
        for k in 0..self.np {
            t[k] = if a >= 1.0 { self.t_to[k] } else { self.t_from[k] + a * (self.t_to[k] - self.t_from[k]) };
        }
        // the shelves take the end points' targets; the bells draw the rest
        let mut shelf_moved = false;
        for (j, sdb) in [t[0], t[self.np - 1]].into_iter().enumerate() {
            let sdb = sdb.clamp(-plan.clamp_db(), plan.clamp_db());
            if sdb != self.shelf_db[j] {
                self.shelf_db[j] = sdb;
                if sdb == 0.0 {
                    // its own 0 dB design (same poles), so the coefficients ramp along the shelf
                    self.shelf[j] = self.zero_shelf[j];
                    self.s_resp[j] = [0.0; MAX_POINTS];
                } else {
                    self.shelf[j] = shelf_design(j, sdb, self.sr);
                    for k in 0..self.np {
                        self.s_resp[j][k] = 10.0 * self.phi[k].mag2(&self.shelf[j]).max(1e-300).log10();
                    }
                }
                shelf_moved = true;
            }
        }
        for j in 0..2 {
            if self.shelf_db[j] != 0.0 {
                for k in 0..self.np {
                    t[k] -= self.s_resp[j][k];
                }
            }
        }
        let moved = t[..self.np] != self.t[..self.np];
        if moved {
            self.t = t;
            self.still = 0;
        } else {
            self.still = self.still.saturating_add(1);
        }
        if shelf_moved {
            self.changed = true;
        }
        if moved || self.still <= SETTLE_TICKS || self.last_dg >= TOLERANCE_DB {
            if self.t[..self.np].iter().all(|&v| v == 0.0) {
                // flat is flat: exactly 0 dB, no iteration towards it
                if self.g[..self.n].iter().any(|&v| v != 0.0) {
                    self.g = [0.0; MAX_SECTIONS];
                    plan.designs(&self.g, &mut self.designs);
                    self.changed = true;
                }
                self.last_dg = 0.0;
            } else {
                self.last_dg = plan.correct_lean(&self.t, &mut self.g, &mut self.designs, SKIP_DB);
                self.changed = true;
            }
        }
        let flat = self.flat();
        self.flat_ticks = if flat { self.flat_ticks.saturating_add(1) } else { 0 };
        if !flat && !self.engaged {
            // out of bypass: the cascade starts from rest on its 0 dB designs (exactly the input),
            // ramps to the designs over the tick, and fades in
            self.engaged = true;
            self.restart = true;
            self.changed = false;
            self.wet.set_immediate(0.0);
            self.wet.set_target(1.0);
            return;
        }
        if self.engaged {
            self.wet.set_target(if flat && self.flat_ticks >= flat_hold { 0.0 } else { 1.0 });
        }
    }

    /// Per sample: the bypass fade.
    #[inline]
    pub fn advance(&mut self) {
        if self.engaged {
            self.m = self.wet.next();
            if self.m == 0.0 && self.wet.is_settled() {
                self.engaged = false;
            }
        }
    }

    /// The realised response at `f` (dB): the bells and the shelves.
    pub fn response_db(&self, plan: &Plan, f: f64) -> f64 {
        let ph = Phi::new(f, self.sr);
        plan.response_db(&self.designs, f) + self.shelf.iter().map(|s| 10.0 * ph.mag2(s).max(1e-300).log10()).sum::<f64>()
    }

    /// The largest section gain (dB, tests).
    pub fn largest_db(&self) -> f64 {
        self.g[..self.n].iter().fold(0.0f64, |m, v| m.max(v.abs()))
    }
}

/// Shelf `j` (0 = low at 30 Hz, 1 = high at 18 kHz) of `gain_db`.
fn shelf_design(j: usize, gain_db: f64, sr: f64) -> Biquad {
    if j == 0 {
        design(Analog::low_shelf(SHELF_Q, gain_db), SHELF_LO_HZ, sr)
    } else {
        design(Analog::high_shelf(SHELF_Q, gain_db), SHELF_HI_HZ, sr)
    }
}

impl Default for Slot {
    fn default() -> Self {
        Self::new()
    }
}

/// The 27 bells of one channel: Simper's TPT state-variable structure (gfx-dsp `svf`, the same state
/// and coefficients, ramped linearly the same way), with each section's output written as
/// y = G · x + C(state), G = m0 + m1 a2 + m2 a3: the same filter, but the path from one section
/// to the next is one multiply-add instead of three, so the cascade is not latency-bound.
#[derive(Clone, Debug)]
pub struct Cascade {
    n: usize,
    c: [[f64; 6]; CASC],
    step: [[f64; 6]; CASC],
    g: [f64; CASC],
    left: u32,
    s: [[f64; 2]; CASC],
}

impl Default for Cascade {
    fn default() -> Self {
        Self { n: 0, c: [[0.0, 0.0, 0.0, 1.0, 0.0, 0.0]; CASC], step: [[0.0; 6]; CASC], g: [1.0; CASC], left: 0, s: [[0.0; 2]; CASC] }
    }
}

impl Cascade {
    /// Silence in every section.
    pub fn reset(&mut self) {
        self.s = [[0.0; 2]; CASC];
    }

    /// The biquad of section k: a bell, or a shelf after the `n` bells.
    #[inline]
    fn section<'a>(designs: &'a [Biquad; MAX_SECTIONS], shelf: &'a [Biquad; 2], n: usize, k: usize) -> &'a Biquad {
        if k < n {
            &designs[k]
        } else {
            &shelf[k - n]
        }
    }

    /// Coefficients of `designs[..n]` and the shelves at once (a biquad without a TPT realisation
    /// keeps its section's).
    pub fn set(&mut self, designs: &[Biquad; MAX_SECTIONS], shelf: &[Biquad; 2], n: usize) {
        self.n = n + 2;
        self.left = 0;
        for k in 0..n + 2 {
            if let Some(c) = SvfCoefs::from_biquad(Self::section(designs, shelf, n, k)) {
                self.c[k] = c.parts();
            }
            let [_, a2, a3, m0, m1, m2] = self.c[k];
            self.g[k] = m0 + m1 * a2 + m2 * a3;
        }
    }

    /// The same, reached linearly over `len` samples.
    pub fn ramp(&mut self, designs: &[Biquad; MAX_SECTIONS], shelf: &[Biquad; 2], n: usize, len: u32) {
        self.n = n + 2;
        let len = len.max(1);
        let r = 1.0 / len as f64;
        for k in 0..n + 2 {
            let t = SvfCoefs::from_biquad(Self::section(designs, shelf, n, k)).map_or(self.c[k], |c| c.parts());
            for j in 0..6 {
                self.step[k][j] = (t[j] - self.c[k][j]) * r;
            }
        }
        self.left = len;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let n = self.n;
        if self.left > 0 {
            self.left -= 1;
            // one flat run of adds (vectorises), then each section's G
            for (c, st) in self.c.as_flattened_mut()[..6 * n].iter_mut().zip(self.step.as_flattened()) {
                *c += *st;
            }
            for (g, c) in self.g[..n].iter_mut().zip(self.c.iter()) {
                *g = c[3] + c[4] * c[1] + c[5] * c[2];
            }
        }
        let mut y = x as f64;
        for k in 0..n {
            let [a1, a2, a3, _, m1, m2] = self.c[k];
            let [ic1, ic2] = self.s[k];
            // C(state): what the section adds whatever its input
            let cst = m1 * (a1 * ic1 - a2 * ic2) + m2 * (ic2 + a2 * ic1 - a3 * ic2);
            let v3 = y - ic2;
            let v1 = a1 * ic1 + a2 * v3;
            let v2 = ic2 + a2 * ic1 + a3 * v3;
            self.s[k] = [2.0 * v1 - ic1, 2.0 * v2 - ic2];
            y = self.g[k] * y + cst;
        }
        y as f32
    }
}

/// Bins [lo, hi] (inclusive) of each design point's ±1/3-octave window, at least the nearest bin
/// (log distance, bins ≥ 1).
pub fn point_bins(points: &[f64], n: usize, sample_rate: f64, out: &mut [(u32, u32); MAX_POINTS]) {
    let bin = sample_rate / n as f64;
    let nb = n / 2 + 1;
    for (i, &fp) in points.iter().enumerate() {
        let lo = ((fp * (-TARGET_HALF_OCT).exp2() / bin).ceil().max(1.0)) as usize;
        let hi = ((fp * TARGET_HALF_OCT.exp2() / bin).floor() as usize).min(nb - 1);
        out[i] = if lo <= hi {
            (lo as u32, hi as u32)
        } else {
            // no bin inside: the nearest in log frequency
            let k = (fp / bin).floor().max(1.0) as usize;
            let (a, b) = (k.min(nb - 1), (k + 1).min(nb - 1));
            let da = ((a as f64 * bin) / fp).log2().abs();
            let db = ((b as f64 * bin) / fp).log2().abs();
            let k = if db < da { b } else { a };
            (k as u32, k as u32)
        };
    }
}
