// SPDX-License-Identifier: GPL-3.0-or-later
//! The decay stage (README §2.3, as rewritten at review): the strings' sustain shortened per band
//! and per note, causally, at zero latency. **Never worse than the DI**: the gain is ≤ 0 dB, a pick is
//! never attenuated, and what it cannot shorten it leaves alone.
//!
//! - Bands: gfx-dsp `crossover::Tree`, LR4 at 250 Hz, 1 kHz and 4 kHz (the sum is an all-pass when
//!   nothing acts).
//! - Detector per band: a sliding maximum over 25 ms (one period of the lowest note, in every band),
//!   then a release smoother of 60 ms (instant attack), in dB.
//! - Reduction r (dB ≥ 0): while the detector falls, r grows by k dB per dB of fall (T60 ÷ (1 + k));
//!   on a rise of Δ above the last minimum, r = r_min · max(0, 1 − Δ / 3 dB) (a new pick re-opens the
//!   band, fully from a 3 dB rise); capped at [`CAP_DB`] (12 dB: the README's fallback 1), smoothed
//!   over 2 ms.
//! - The log, the rule and the exp run every [`TICK`] samples; the gain is ramped in between.
//! - **Noise** (set here, README §2.3 left it to the bench): r is held (frozen) while the band is
//!   below −70 dBFS (−76 to enter, 6 dB of hysteresis), and while the band is stationary: its
//!   detector moved less than 10 dB over the last 3.2 s (32 blocks of 100 ms, counted from the
//!   reset). Hiss never moves the gain; a note always has the range (its pick, its fall).
//! - `amount` scales every k_b (0 = off: the Tree is bypassed after a 20 ms crossfade, bit-exact).

use gfx_dsp::crossover::{Order, Tree};
use gfx_dsp::envelope::SlidingMax;

pub const BANDS: usize = 4;
pub const SPLITS: [f32; 3] = [250.0, 1000.0, 4000.0];
pub const TICK: usize = 8;
pub const WINDOW_MS: f64 = 25.0;
pub const RELEASE_MS: f64 = 60.0;
pub const SMOOTH_MS: f64 = 2.0;
pub const REOPEN_DB: f64 = 3.0;
/// The cap on the reduction. README §2.3 wrote 24 dB; its fallback 1 (12 dB, the models' k halved,
/// [`crate::K_SCALE`]) runs here because the bench row "a note 12 dB softer, 0.6 s after a loud one:
/// within 8 dB" failed with the rule as written (−8.7 dB in the 250 Hz – 1 kHz band; README, As
/// coded). Restoring 24 dB and K_SCALE 1 is the owner's call after listening.
pub const CAP_DB: f64 = 12.0;
pub const FLOOR_DB: f64 = -70.0;
pub const HYST_DB: f64 = 6.0;
/// Held while the band's detector moved less than this over the last 3.2 s (stationary: hiss, a
/// steady drone).
pub const STATIONARY_DB: f64 = 10.0;
const NF_BLOCKS: usize = 32;
const SILENT_DB: f64 = -180.0;

#[derive(Clone, Debug, Default)]
struct Band {
    max: SlidingMax,
    last: f32,
    s: f64,
    prev: f64,
    emin: f64,
    r0: f64,
    r: f64,
    rs: f64,
    held_abs: bool,
    g: f64,
    step: f64,
    /// per 100 ms block: the detector's minimum and maximum (NaN = not filled since the reset)
    lo: [f64; NF_BLOCKS],
    hi: [f64; NF_BLOCKS],
    at: usize,
    cur_lo: f64,
    cur_hi: f64,
    /// the detector's range over the filled blocks
    range: f64,
}

#[derive(Clone, Debug)]
pub struct Decay {
    tree: Tree,
    b: [Band; BANDS],
    k: [f64; BANDS],
    c_rel: f64,
    c_sm: f64,
    block_ticks: u32,
    ticks: u32,
    /// how much of the processed path is heard (20 ms crossfade); 0 and `off` = pass-through
    fade: f64,
    fade_step: f64,
    on: bool,
    running: bool,
}

impl Default for Decay {
    fn default() -> Self {
        Self {
            tree: Tree::new(Order::Lr4, BANDS),
            b: Default::default(),
            k: [0.0; BANDS],
            c_rel: 0.0,
            c_sm: 0.0,
            block_ticks: 1,
            ticks: 0,
            fade: 0.0,
            fade_step: 0.0,
            on: false,
            running: false,
        }
    }
}

impl Decay {
    pub fn prepare(&mut self, fs: f32) {
        let f = fs as f64;
        self.tree.prepare(fs, 0.0);
        for (k, &hz) in SPLITS.iter().enumerate() {
            self.tree.set_freq_now(k, hz);
        }
        for b in self.b.iter_mut() {
            b.max.prepare((WINDOW_MS * 0.001 * f).ceil() as usize + 1);
            b.max.set_window((WINDOW_MS * 0.001 * f).round() as usize);
        }
        self.c_rel = 1.0 - (-(TICK as f64) / (RELEASE_MS * 0.001 * f)).exp();
        self.c_sm = 1.0 - (-(TICK as f64) / (SMOOTH_MS * 0.001 * f)).exp();
        self.block_ticks = ((0.1 * f) / TICK as f64).round().max(1.0) as u32;
        self.fade_step = TICK as f64 / (0.020 * f);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.tree.reset();
        for b in self.b.iter_mut() {
            b.max.reset();
            b.last = 0.0;
            b.s = SILENT_DB;
            b.prev = SILENT_DB;
            b.emin = SILENT_DB;
            b.r0 = 0.0;
            b.r = 0.0;
            b.rs = 0.0;
            b.held_abs = true;
            b.g = 1.0;
            b.step = 0.0;
            b.lo = [f64::NAN; NF_BLOCKS];
            b.hi = [f64::NAN; NF_BLOCKS];
            b.at = 0;
            b.cur_lo = f64::INFINITY;
            b.cur_hi = f64::NEG_INFINITY;
            b.range = f64::INFINITY;
        }
        self.ticks = 0;
        self.fade = if self.on { 1.0 } else { 0.0 };
        self.running = self.on;
    }

    /// k per band (the model's × `decay` / 50).
    pub fn set_k(&mut self, k: [f64; BANDS]) {
        self.k = k;
        self.on = k.iter().any(|v| *v > 0.0);
        if self.on && !self.running {
            // starts cold from the next sample, faded in
            self.reset_state();
            self.running = true;
        }
    }

    fn reset_state(&mut self) {
        let on = self.on;
        let fade = self.fade;
        self.reset();
        self.on = on;
        self.fade = fade;
    }

    /// Whether the stage passes the input through untouched.
    pub fn bypassed(&self) -> bool {
        !self.running
    }

    /// The gain of band `b` (dB, ≤ 0): a diagnostic for the tests.
    pub fn gain_db(&self, b: usize) -> f64 {
        20.0 * self.b[b].g.max(1e-12).log10()
    }

    /// Band `b`'s detector (dB) and its range over the stationarity window: diagnostics.
    pub fn detector_db(&self, b: usize) -> (f64, f64) {
        (self.b[b].s, self.b[b].range)
    }

    /// Control tick (every [`TICK`] samples, before the tick's first sample).
    pub fn control(&mut self) {
        if !self.running {
            return;
        }
        // the crossfade with the dry
        let target = if self.on { 1.0 } else { 0.0 };
        if self.fade < target {
            self.fade = (self.fade + self.fade_step).min(1.0);
        } else if self.fade > target {
            self.fade = (self.fade - self.fade_step).max(0.0);
            if self.fade == 0.0 {
                self.running = false;
                return;
            }
        }
        self.ticks += 1;
        let new_block = self.ticks >= self.block_ticks;
        if new_block {
            self.ticks = 0;
        }
        for (i, b) in self.b.iter_mut().enumerate() {
            let k = self.k[i];
            // the 25 ms maximum read at the tick (it advanced every sample)
            let e = b.peek_db();
            b.s = if e > b.s { e } else { b.s + (e - b.s) * self.c_rel };
            let s = b.s;
            // stationarity: the detector's range over the last 3.2 s (filled blocks and this one)
            b.cur_lo = b.cur_lo.min(s);
            b.cur_hi = b.cur_hi.max(s);
            if new_block {
                b.lo[b.at] = b.cur_lo;
                b.hi[b.at] = b.cur_hi;
                b.at = (b.at + 1) % NF_BLOCKS;
                b.cur_lo = f64::INFINITY;
                b.cur_hi = f64::NEG_INFINITY;
            }
            let (mut lo, mut hi) = (b.cur_lo, b.cur_hi);
            for j in 0..NF_BLOCKS {
                if !b.lo[j].is_nan() {
                    lo = lo.min(b.lo[j]);
                    hi = hi.max(b.hi[j]);
                }
            }
            b.range = hi - lo;
            if b.held_abs {
                if s > FLOOR_DB {
                    b.held_abs = false;
                }
            } else if s < FLOOR_DB - HYST_DB {
                b.held_abs = true;
            }
            let held = b.held_abs || b.range < STATIONARY_DB;
            let d = s - b.prev;
            if !held {
                if d < 0.0 {
                    b.r = (b.r - k * d).min(CAP_DB);
                    b.emin = s;
                    b.r0 = b.r;
                } else if d > 0.0 {
                    b.r = b.r0 * (1.0 - (s - b.emin) / REOPEN_DB).max(0.0);
                }
            }
            // held: r frozen (a pick out of the floor widens the range and re-opens through the rule)
            b.prev = s;
            // the gain: never above 0 dB; a re-open is not smoothed (the pick is never attenuated)
            b.rs = if b.r < b.rs { b.r } else { b.rs + (b.r - b.rs) * self.c_sm };
            let g = 10f64.powf(-b.rs / 20.0);
            b.step = (g - b.g) / TICK as f64;
        }
    }

    /// One sample.
    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        if !self.running {
            return x;
        }
        let mut bands = [0.0f64; BANDS];
        self.tree.process(x, &mut bands);
        let mut y = 0.0;
        for (b, v) in self.b.iter_mut().zip(bands) {
            b.last = b.max.next(v as f32);
            b.g += b.step;
            y += v * b.g;
        }
        if self.fade >= 1.0 {
            y
        } else {
            x + (y - x) * self.fade
        }
    }
}

impl Band {
    /// The 25 ms maximum as of the last sample, in dB.
    fn peek_db(&self) -> f64 {
        if self.last > 0.0 {
            20.0 * (self.last as f64).log10()
        } else {
            SILENT_DB
        }
    }
}
