// SPDX-License-Identifier: GPL-3.0-or-later
//! Zero-latency onset detection for note-by-note effects (design: gfx-swell/README.md §2.2 – 2.4).
//!
//! Each detector is a standalone type, usable alone:
//! - [`Level`]: the note level (a full-band peak follower in dB), the phrase level (its peak, held 5 s,
//!   then falling 3 dB / s) and the floor relative to it; [`LevelGate`]: the level crossing the floor
//!   for 1 ms (the classic slow gear's rule, used after a reset only).
//! - [`Pick`] (detector A): a fast envelope in dB against its own 150 ms average, with a slope test, a
//!   floor, hysteresis re-arm and a refractory time. Its envelope is either its own (2 × 2nd-order
//!   high-pass at 1.5 kHz, 0.2 / 5 ms: gfx-swell, later gfx-killswitch) or any **external level in dB**
//!   fed to [`Pick::detect`] (gfx-ducker's sliding maximum).
//! - [`Flux`] (detector B): a 16-band "shape flux" in dB against a maximum filter over neighbouring
//!   bands 8 ms earlier (SuperFlux's idea), corrected for level rises common to every ringing band
//!   (tremolo-proof), with an adaptive threshold (a 150 ms median).
//! - [`PitchJump`] (detector C): note changes read in a YIN pitch track (stable pairs, a dragged
//!   reference, integer-ratio jumps ignored), with an adaptive window.
//! - [`Strum`]: the arming logic: one onset per strum window, a same-note hold-off for the late
//!   detectors, slides merged.
//! - [`Strength`]: a continuous onset strength 0 – 1 (no event): a fast envelope in dB against a
//!   slow one that follows the fast *envelope* (gfx-widener's transient focus).
//!
//! Every level is relative to the playing's own level, so the same phrase 24 dB quieter fires the
//! same onsets. Allocation in `prepare` only; deterministic.

use crate::detect::PitchDetector;
use crate::envelope::{PeakFollower, SlidingMax};
use crate::svf::{Svf, SvfMode};
use crate::util::one_pole_coef;

/// The floor of every dB value in this module.
pub const DB_FLOOR: f32 = -120.0;

#[inline]
fn to_db(x: f64) -> f32 {
    if x <= 1e-6 {
        DB_FLOOR
    } else {
        (20.0 * x.log10()).max(DB_FLOOR as f64) as f32
    }
}

/// Which detector fired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// L: the level crossed the floor (after a reset).
    Level,
    /// A: a pick.
    Pick,
    /// B: new energy (shape flux).
    Flux,
    /// C: a pitch jump.
    Pitch,
    /// A manual trigger (footswitch).
    Manual,
}

// ---------------------------------------------------------------------------------------------
// L: levels

/// Note level, phrase level and the floor below which nothing may trigger.
#[derive(Clone, Debug)]
pub struct Level {
    follower: PeakFollower,
    e_db: f32,
    phrase_db: f32,
    hold: u32,
    hold_n: u32,
    fall: f32,
}

impl Default for Level {
    fn default() -> Self {
        Self { follower: PeakFollower::new(0.2, 30.0), e_db: DB_FLOOR, phrase_db: DB_FLOOR, hold: 0, hold_n: 0, fall: 0.0 }
    }
}

impl Level {
    pub const HOLD_S: f32 = 5.0;
    pub const FALL_DB_S: f32 = 3.0;

    pub fn prepare(&mut self, sample_rate: f32) {
        self.follower.prepare(sample_rate);
        self.hold_n = (Self::HOLD_S * sample_rate) as u32;
        self.fall = Self::FALL_DB_S / sample_rate;
    }

    pub fn reset(&mut self) {
        self.follower.reset();
        self.e_db = DB_FLOOR;
        self.phrase_db = DB_FLOOR;
        self.hold = 0;
    }

    /// One detection sample; `thresh_db` is the absolute floor the phrase level never falls under.
    /// Returns the note level in dB.
    #[inline]
    pub fn next(&mut self, x: f32, thresh_db: f32) -> f32 {
        self.e_db = to_db(self.follower.next(x) as f64);
        if self.e_db >= self.phrase_db {
            self.phrase_db = self.e_db;
            self.hold = self.hold_n;
        } else if self.hold > 0 {
            self.hold -= 1;
        } else {
            self.phrase_db -= self.fall;
        }
        self.phrase_db = self.phrase_db.max(thresh_db);
        self.e_db
    }

    /// The note level (dB).
    pub fn note_db(&self) -> f32 {
        self.e_db
    }

    /// The phrase level (dB).
    pub fn phrase_db(&self) -> f32 {
        self.phrase_db
    }

    /// The floor for any onset: max(`thresh`, phrase − `window`).
    pub fn floor_db(&self, window_db: f32, thresh_db: f32) -> f32 {
        thresh_db.max(self.phrase_db - window_db)
    }
}

/// L: the level above the floor for 1 ms, while the caller's gain is closed. It arms only 6 dB under
/// the floor (or by [`LevelGate::arm`]: reset, engage), so a tail still near the floor when the gain
/// closes cannot open it again.
#[derive(Clone, Debug, Default)]
pub struct LevelGate {
    armed: bool,
    count: u32,
    need: u32,
}

impl LevelGate {
    /// The gate arms this far under the floor: a tail hovering at the floor right after a reset does
    /// not open it again (it did, on an 8 s decaying note).
    pub const HYST_DB: f32 = 6.0;

    pub fn prepare(&mut self, sample_rate: f32) {
        self.need = ((0.001 * sample_rate) as u32).max(1);
    }

    pub fn reset(&mut self) {
        self.arm();
    }

    pub fn arm(&mut self) {
        self.armed = true;
        self.count = 0;
    }

    /// Detection lag after the crossing (samples).
    pub fn lag(&self) -> u32 {
        self.need
    }

    /// `level_db` against `floor_db`: it fires above the floor, and arms only 6 dB under it.
    #[inline]
    pub fn next(&mut self, level_db: f32, floor_db: f32, closed: bool) -> bool {
        if !closed {
            self.armed = false;
            self.count = 0;
            return false;
        }
        if level_db <= floor_db {
            if level_db < floor_db - Self::HYST_DB {
                self.armed = true;
            }
            self.count = 0;
            return false;
        }
        if !self.armed {
            return false;
        }
        self.count += 1;
        if self.count >= self.need {
            self.armed = false;
            return true;
        }
        false
    }
}

// ---------------------------------------------------------------------------------------------
// A: picks

/// Detector A's settings (each user maps its own `sens`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickSettings {
    /// R: env − slow must exceed it (dB); re-arms below R / 2.
    pub rise_db: f32,
    /// The slope test's window: the envelope must have risen by R / 2 within it (ms, ≤ [`Pick::MAX_SLOPE_MS`]).
    pub slope_ms: f32,
    /// The slope test's reference: the envelope's maximum over this long before the slope window (ms,
    /// ≤ [`Pick::MAX_HOLD_MS`]); 0 = its value at the window's start. One period of the lowest note
    /// makes the test ripple-free: a periodic envelope never rises above its own last peak.
    pub hold_ms: f32,
    /// No new onset for this long after one (ms).
    pub refractory_ms: f32,
}

impl Default for PickSettings {
    fn default() -> Self {
        Self { rise_db: Pick::rise_for(0.5), slope_ms: 3.0, hold_ms: 13.4, refractory_ms: 20.0 }
    }
}

/// Detector A. Feed it samples ([`Pick::next`], its own high-passed envelope) or an external level in
/// dB ([`Pick::detect`]); the floor is the caller's (a bool per sample).
#[derive(Clone, Debug, Default)]
pub struct Pick {
    hp: [Svf; 2],
    fast: f64,
    k_fa: f64,
    k_fr: f64,
    slow_db: f64,
    k_slow: f64,
    /// the envelope's recent past in dB (the slope test)
    hist: Vec<f32>,
    at: usize,
    slope_n: usize,
    /// the maximum of the envelope over `hold` before the slope window (dB above DB_FLOOR)
    held: SlidingMax,
    hold_on: bool,
    /// the envelope's peak since the last onset, and the samples since it (re-arm)
    peak_db: f32,
    since: u32,
    rearm_n: u32,
    refr_n: u32,
    refr: u32,
    armed: bool,
    s: PickSettings,
    sr: f32,
    /// env − slow at the last decision (dB), read by callers that grade onsets (gfx-grains)
    last_rise: f32,
}

impl Pick {
    pub const MAX_SLOPE_MS: f32 = 50.0;
    pub const MAX_HOLD_MS: f32 = 50.0;
    /// Re-armed at the latest this long after an onset (ms): the slope test and the refractory time
    /// already keep one transient from firing twice.
    pub const REARM_MS: f32 = 100.0;
    /// Detection lag after the pick, for a lookahead placement (ms).
    pub const LAG_MS: f32 = 0.5;

    /// gfx-swell's `sens` mapping (0 – 1): rise 12 → 3 dB.
    pub fn rise_for(sens: f32) -> f32 {
        12.0 - 9.0 * sens.clamp(0.0, 1.0)
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sr = sample_rate;
        for f in self.hp.iter_mut() {
            f.set(SvfMode::HighPass, 1500.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, sample_rate);
        }
        let sr = sample_rate as f64;
        self.k_fa = one_pole_coef(0.0002 * sr);
        self.k_fr = one_pole_coef(0.005 * sr);
        self.k_slow = one_pole_coef(0.15 * sr);
        self.hist = vec![DB_FLOOR; (Self::MAX_SLOPE_MS * 0.001 * sample_rate) as usize + 2];
        self.held.prepare((Self::MAX_HOLD_MS * 0.001 * sample_rate).ceil() as usize + 1);
        let s = self.s;
        self.set(s);
        self.reset();
    }

    pub fn set(&mut self, s: PickSettings) {
        self.s = s;
        let n = (s.slope_ms.clamp(0.0, Self::MAX_SLOPE_MS) * 0.001 * self.sr).round() as usize;
        self.slope_n = n.clamp(1, self.hist.len().max(2) - 1);
        let h = (s.hold_ms.clamp(0.0, Self::MAX_HOLD_MS) * 0.001 * self.sr).round() as usize;
        self.hold_on = h > 0;
        self.held.set_window(h.max(1));
        self.refr_n = (s.refractory_ms.max(0.0) * 0.001 * self.sr) as u32;
        self.rearm_n = (Self::REARM_MS * 0.001 * self.sr) as u32;
    }

    pub fn reset(&mut self) {
        for f in self.hp.iter_mut() {
            f.reset();
        }
        self.fast = 0.0;
        self.slow_db = DB_FLOOR as f64;
        self.hist.iter_mut().for_each(|v| *v = DB_FLOOR);
        self.at = 0;
        self.held.reset();
        self.peak_db = DB_FLOOR;
        self.since = 0;
        self.refr = 0;
        self.armed = true;
        self.last_rise = 0.0;
    }

    /// Its own envelope: high-passed at 1.5 kHz (2 × 2nd order), fast follower 0.2 / 5 ms, in dB.
    #[inline]
    pub fn envelope(&mut self, x: f32) -> f32 {
        let h0 = self.hp[0].process(x);
        let h = self.hp[1].process(h0);
        let a = h.abs() as f64;
        let k = if a > self.fast { self.k_fa } else { self.k_fr };
        self.fast = a + (self.fast - a) * k;
        to_db(self.fast)
    }

    /// The decision on an envelope in dB (own or external). Fires when:
    /// - env − slow > R (slow: the 150 ms average of env in dB);
    /// - env rose by more than R / 2 above its maximum over `hold` before the slope window (the slope
    ///   test: a transient, not the beating of a chord nor the per-period ripple of a low note);
    /// - the caller's floor is passed, the detector is armed and out of its refractory time.
    ///
    /// Re-arms when env − slow < R / 2, once env has fallen R / 2 under its peak since the onset, or
    /// 100 ms after it. After silence the slow reference takes hundreds of ms to climb from the floor:
    /// with the first rule alone, a second strum 250 ms later, or a note over a steady hiss that had
    /// fired at its start, was missed.
    #[inline]
    pub fn detect(&mut self, env_db: f32, above_floor: bool) -> bool {
        let len = self.hist.len();
        let past = self.hist[(self.at + len - self.slope_n) % len];
        self.at = (self.at + 1) % len;
        self.hist[self.at] = env_db;
        let reference = if self.hold_on { self.held.next(past - DB_FLOOR) + DB_FLOOR } else { past };
        let rise = env_db - self.slow_db as f32;
        self.last_rise = rise;
        self.slow_db = env_db as f64 + (self.slow_db - env_db as f64) * self.k_slow;
        self.refr = self.refr.saturating_sub(1);
        self.peak_db = self.peak_db.max(env_db);
        let r = self.s.rise_db;
        if self.armed && self.refr == 0 && rise > r && env_db - reference > 0.5 * r && above_floor {
            self.armed = false;
            self.refr = self.refr_n;
            self.peak_db = env_db;
            self.since = 0;
            return true;
        }
        self.since = self.since.saturating_add(1);
        if !self.armed && (rise < 0.5 * r || env_db < self.peak_db - 0.5 * r || self.since >= self.rearm_n) {
            self.armed = true;
        }
        false
    }

    /// The rise (envelope − its 150 ms average, dB) at the last decision: an onset's strength is its
    /// maximum over the few ms after it fires. Read only; it changes no decision.
    #[inline]
    pub fn rise_db(&self) -> f32 {
        self.last_rise
    }

    /// One sample through its own envelope.
    #[inline]
    pub fn next(&mut self, x: f32, above_floor: bool) -> bool {
        let e = self.envelope(x);
        self.detect(e, above_floor)
    }
}

// ---------------------------------------------------------------------------------------------
// B: shape flux

pub const BANDS: usize = 16;
/// The maximum filter's distance (hops of 1 ms).
const MU: usize = 8;
/// The adaptive threshold's median length (hops).
const MED: usize = 150;

/// Detector B: new energy without a pick.
#[derive(Clone, Debug)]
pub struct Flux {
    bp: [Svf; BANDS],
    env: [f64; BANDS],
    k: [f64; BANDS],
    hop: u32,
    cnt: u32,
    /// band levels (dB) of the last MU + 1 hops
    hist: [[f32; BANDS]; MU + 1],
    /// each band's level MU hops ago, peak-held with a 20 dB / s release
    held: [f32; BANDS],
    hi: usize,
    filled: usize,
    d_hist: [f32; MED],
    di: usize,
    d_filled: usize,
    scratch: [f32; MED],
    armed: bool,
    last_d: f32,
    last_thr: f32,
}

impl Default for Flux {
    fn default() -> Self {
        Self {
            bp: Default::default(),
            env: [0.0; BANDS],
            k: [0.0; BANDS],
            hop: 48,
            cnt: 0,
            hist: [[DB_FLOOR; BANDS]; MU + 1],
            held: [DB_FLOOR; BANDS],
            hi: 0,
            filled: 0,
            d_hist: [0.0; MED],
            di: 0,
            d_filled: 0,
            scratch: [0.0; MED],
            armed: true,
            last_d: 0.0,
            last_thr: 0.0,
        }
    }
}

fn median(v: &mut [f32]) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let m = v.len() / 2;
    let (_, x, _) = v.select_nth_unstable_by(m, |a, b| a.total_cmp(b));
    *x
}

impl Flux {
    /// The lowest band's centre (Hz) and the spacing (semitones).
    pub const F0: f32 = 90.0;
    pub const STEP_ST: f32 = 5.0;
    /// Band-pass Q (5 semitones wide).
    pub const Q: f32 = 3.4;
    /// The band envelopes' shortest time constant (s).
    pub const TAU_MIN_S: f32 = 0.002;
    /// The maximum filter reads each band's level peak-held with this release (dB / s): a band coming
    /// back from a beating null, a tremolo trough or a noise dip only returns to where it recently was.
    /// With the single frame of 8 ms ago (the design), two partials beating at 4.4 Hz (a third left
    /// ringing) and pink noise both fired.
    pub const HOLD_RELEASE_DB_S: f32 = 20.0;
    /// Detection lag for a lookahead placement (ms): the maximum filter's distance.
    pub const LAG_MS: f32 = MU as f32;

    /// δ for gfx-swell's `legato` (0 – 1): 36 → 12 dB summed over the bands.
    pub fn delta_for(legato: f32) -> f32 {
        36.0 - 24.0 * legato.clamp(0.0, 1.0)
    }

    pub fn centre(b: usize) -> f32 {
        Self::F0 * 2f32.powf(Self::STEP_ST * b as f32 / 12.0)
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        for b in 0..BANDS {
            let fc = Self::centre(b).min(0.45 * sample_rate);
            self.bp[b].set(SvfMode::BandPass, fc, Self::Q, 0.0, sample_rate);
            let tau = (2.0 / fc).max(Self::TAU_MIN_S) as f64;
            self.k[b] = one_pole_coef(tau * sample_rate as f64);
        }
        self.hop = ((0.001 * sample_rate).round() as u32).max(1);
        self.reset();
    }

    pub fn reset(&mut self) {
        for f in self.bp.iter_mut() {
            f.reset();
        }
        self.env = [0.0; BANDS];
        self.cnt = 0;
        self.hist = [[DB_FLOOR; BANDS]; MU + 1];
        self.held = [DB_FLOOR; BANDS];
        self.hi = 0;
        self.filled = 0;
        self.d_hist = [0.0; MED];
        self.di = 0;
        self.d_filled = 0;
        self.armed = true;
        self.last_d = 0.0;
        self.last_thr = 0.0;
    }

    /// The last hop's flux D and threshold (diagnostics, tuning).
    pub fn last(&self) -> (f32, f32) {
        (self.last_d, self.last_thr)
    }

    /// One sample. `phrase_db` ties the band floor to the playing level (phrase − 60 dB); `delta_db` is
    /// δ. Returns true on the hop where D crosses the adaptive threshold.
    #[inline]
    pub fn next(&mut self, x: f32, phrase_db: f32, delta_db: f32) -> bool {
        for b in 0..BANDS {
            let a = self.bp[b].process(x).abs() as f64;
            self.env[b] = a + (self.env[b] - a) * self.k[b];
        }
        self.cnt += 1;
        if self.cnt < self.hop {
            return false;
        }
        self.cnt = 0;
        self.hop_frame(phrase_db, delta_db)
    }

    fn hop_frame(&mut self, phrase_db: f32, delta_db: f32) -> bool {
        let floor = phrase_db - 60.0;
        let mut lv = [0.0f32; BANDS];
        for b in 0..BANDS {
            lv[b] = to_db(self.env[b]).max(floor);
        }
        // the frame MU hops ago (the oldest in the ring)
        let old_i = (self.hi + 1) % (MU + 1);
        self.hi = old_i;
        let prev = self.hist[old_i];
        self.hist[old_i] = lv;
        self.filled = (self.filled + 1).min(MU + 1);
        let rel = Self::HOLD_RELEASE_DB_S * 0.001;
        for b in 0..BANDS {
            self.held[b] = prev[b].max(self.held[b] - rel);
        }
        if self.filled <= MU {
            return false;
        }
        let held = self.held;
        let mut d = [0.0f32; BANDS];
        let mut ring = [0.0f32; BANDS];
        let mut nr = 0;
        for b in 0..BANDS {
            let lo = if b > 0 { held[b - 1] } else { DB_FLOOR };
            let hi = if b + 1 < BANDS { held[b + 1] } else { DB_FLOOR };
            // a band under the (moving) floor counts from the floor
            let mx = held[b].max(lo).max(hi).max(floor);
            d[b] = lv[b] - mx;
            if prev[b] > floor + 6.0 {
                ring[nr] = d[b];
                nr += 1;
            }
        }
        let r = if nr >= 3 { median(&mut ring[..nr]).max(0.0) } else { 0.0 };
        let mut sum = 0.0f32;
        for b in 0..BANDS {
            let corr = if prev[b] > floor + 6.0 { r } else { 0.0 };
            sum += (d[b] - corr).max(0.0);
        }
        let n = self.d_filled;
        self.scratch[..n].copy_from_slice(&self.d_hist[..n]);
        let thr = delta_db + 1.5 * median(&mut self.scratch[..n]);
        self.d_hist[self.di] = sum;
        self.di = (self.di + 1) % MED;
        self.d_filled = (self.d_filled + 1).min(MED);
        self.last_d = sum;
        self.last_thr = thr;
        if self.armed && sum > thr {
            self.armed = false;
            return true;
        }
        if !self.armed && sum < 0.5 * thr {
            self.armed = true;
        }
        false
    }
}

// ---------------------------------------------------------------------------------------------
// C: pitch jump

/// A note change found by [`PitchJump`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Jump {
    /// The interval from the reference (cents).
    pub cents: f32,
    /// The detection lag behind the change (samples), for a lookahead placement.
    pub lag: u32,
}

/// Detector C: mono legato (hammer-ons, pull-offs, slides) read in a YIN pitch track.
#[derive(Clone, Default)]
pub struct PitchJump {
    det: PitchDetector,
    sr: f32,
    clock: u64,
    /// the previous analysis, when voiced and clear: (cents, time)
    prev: Option<(f32, u64)>,
    /// the reference pitch (cents), when it was set, the last analysis that matched it
    reference: Option<f32>,
    ref_start: u64,
    ref_last: u64,
    last_voiced: u64,
    low_hz: f32,
    fresh: bool,
    /// the previous voiced analysis at any clarity (cents)
    path: Option<f32>,
}

/// cents of a frequency (relative to 1 Hz)
#[inline]
fn cents(hz: f32) -> f32 {
    1200.0 * hz.max(1e-3).log2()
}

impl PitchJump {
    /// A jump is at least this far from the reference (cents).
    pub const JUMP_CENTS: f32 = 70.0;
    /// Two analyses closer than this form a stable pair (cents).
    pub const STABLE_CENTS: f32 = 25.0;
    /// A voiced analysis this close to the previous one continues a glide (cents): vibrato of ± 100
    /// cents at 7 Hz moves 12 cents per analysis, a 200-cent bend in 40 ms 13.5.
    pub const DRAG_CENTS: f32 = 18.0;
    pub const REF_MIN_MS: f32 = 30.0;
    pub const CHANGE_MAX_MS: f32 = 40.0;
    pub const SILENT_MS: f32 = 30.0;

    /// The clarity bound for gfx-swell's `legato` (0 – 1): 0.92 → 0.80.
    pub fn clarity_for(legato: f32) -> f32 {
        0.92 - 0.12 * legato.clamp(0.0, 1.0)
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sr = sample_rate;
        self.det.prepare_range(sample_rate, crate::detect::F_MIN, crate::detect::F_MAX, true);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.det.reset();
        self.det.set_range(crate::detect::F_MIN, crate::detect::F_MAX);
        self.low_hz = crate::detect::F_MIN;
        self.clock = 0;
        self.prev = None;
        self.reference = None;
        self.ref_start = 0;
        self.ref_last = 0;
        self.last_voiced = 0;
        self.path = None;
    }

    fn ms(&self, n: u64) -> f32 {
        n as f32 * 1000.0 / self.sr
    }

    fn follow(&mut self, c: f32) {
        // the window follows the note: lowest frequency = max(75 Hz, 0.45 × the reference)
        let low = (0.45 * 2f32.powf(c / 1200.0)).max(crate::detect::F_MIN);
        if (low - self.low_hz).abs() > 0.5 {
            self.low_hz = low;
            self.det.set_range(low, crate::detect::F_MAX);
        }
    }

    fn reopen(&mut self) {
        if self.low_hz != crate::detect::F_MIN {
            self.low_hz = crate::detect::F_MIN;
            self.det.set_range(crate::detect::F_MIN, crate::detect::F_MAX);
        }
        self.reference = None;
        self.prev = None;
    }

    /// The last analysis: (fundamental when voiced, clarity) — diagnostics.
    pub fn analysis(&self) -> (Option<f32>, f32) {
        (self.det.hz(), self.det.clarity())
    }

    /// Whether the last sample completed an analysis (diagnostics).
    pub fn fresh(&self) -> bool {
        self.fresh
    }

    /// The reference pitch (Hz), when there is one.
    pub fn reference_hz(&self) -> Option<f32> {
        self.reference.map(|c| 2f32.powf(c / 1200.0))
    }

    /// One sample. `clarity_min` is the voicing bound. Returns a jump on the analysis that confirms it.
    #[inline]
    pub fn next(&mut self, x: f32, clarity_min: f32) -> Option<Jump> {
        self.clock += 1;
        self.fresh = self.det.push(x);
        if !self.fresh {
            return None;
        }
        self.analyse(clarity_min)
    }

    /// A reading from the caller's own YIN instead of this detector's (gfx-sustainer runs one for its
    /// notes already): `elapsed` host samples since the previous reading. The internal detector is
    /// not used; its range-following has no effect. Same rules as [`Self::next`].
    pub fn feed(&mut self, hz: Option<f32>, clarity: f32, elapsed: u64, clarity_min: f32) -> Option<Jump> {
        self.clock += elapsed;
        self.fresh = true;
        self.analyse_with(hz, clarity, clarity_min)
    }

    fn analyse(&mut self, clarity_min: f32) -> Option<Jump> {
        let (hz, clarity) = (self.det.hz(), self.det.clarity());
        self.analyse_with(hz, clarity, clarity_min)
    }

    fn analyse_with(&mut self, det_hz: Option<f32>, det_clarity: f32, clarity_min: f32) -> Option<Jump> {
        let t = self.clock;
        // a glide drags the reference along through every voiced analysis that continues the path, at
        // any clarity: on a low E the 13.3 ms window smears a ±100 cent vibrato down to clarity 0.65 –
        // 0.8, and with stable pairs alone the reference stayed behind until the vibrato's peak stood
        // 70 cents away. A legato change crosses unvoiced or garbled analyses instead, and lands at once.
        match det_hz {
            Some(h) => {
                let c = cents(h);
                if let (Some(pv), Some(r)) = (self.path, self.reference) {
                    if (c - pv).abs() <= Self::DRAG_CENTS && (c - r).abs() < Self::JUMP_CENTS {
                        self.reference = Some(c);
                        self.ref_last = t;
                        self.follow(c);
                    }
                }
                self.path = Some(c);
            }
            None => self.path = None,
        }
        let hz = match det_hz {
            Some(h) if det_clarity >= clarity_min => h,
            _ => {
                self.prev = None;
                if self.ms(t - self.last_voiced) > Self::SILENT_MS {
                    self.reopen();
                }
                return None;
            }
        };
        self.last_voiced = t;
        let c = cents(hz);
        let stable = matches!(self.prev, Some((p, _)) if (c - p).abs() <= Self::STABLE_CENTS);
        let pair_t = self.prev.map_or(t, |p| p.1);
        self.prev = Some((c, t));
        if !stable {
            return None;
        }
        let Some(r) = self.reference else {
            self.reference = Some(c);
            self.ref_start = t;
            self.ref_last = t;
            self.follow(c);
            return None;
        };
        let dc = c - r;
        if dc.abs() < Self::JUMP_CENTS {
            // a glide slower than 25 cents per analysis drags the reference along
            self.reference = Some(c);
            self.ref_last = t;
            self.follow(c);
            return None;
        }
        // integer-ratio jumps (an octave, a twelfth, two octaves) re-anchor the reference without firing.
        // They are YIN's own errors and the common period of two strings ringing together, and they
        // hold: on a Karplus-Strong 220 Hz note the harmonic lock read 110 Hz (clarity 1.0) for 140 ms
        // before flipping back, and a major third left ringing read 82 Hz (their common sub-period)
        // then 247 Hz. The design's "three more analyses" confirmation passed both.
        let octave_like = [1200.0f32, 1902.0, 2400.0].iter().any(|o| (dc.abs() - o).abs() <= 30.0);
        let ref_ok = self.ms(self.ref_last - self.ref_start) >= Self::REF_MIN_MS;
        let fast = self.ms(pair_t.saturating_sub(self.ref_last)) <= Self::CHANGE_MAX_MS;
        // the window that had to fill with the new note (the old reference's)
        let window = 1.0 / self.low_hz.max(crate::detect::F_MIN);
        // the new pitch becomes the reference either way
        self.reference = Some(c);
        self.ref_start = t;
        self.ref_last = t;
        self.follow(c);
        if octave_like || !(ref_ok && fast) {
            return None;
        }
        // lag ≈ the window + one period of the new note (measured on the synthetic strings)
        let lag = (window + 1.0 / hz) * self.sr;
        Some(Jump { cents: dc, lag: lag as u32 })
    }
}

// ---------------------------------------------------------------------------------------------
// Onset logic

/// One swell per strum: onsets within `strum` of the one that started a swell are absorbed; the late
/// detectors (B, C) also within max(`strum`, 40 ms) of it (same-note hold-off); a pitch jump in the
/// same direction as the previous one, within `strum` of it, continues a slide.
#[derive(Clone, Debug, Default)]
pub struct Strum {
    clock: u64,
    last: Option<u64>,
    last_jump: Option<(u64, bool)>,
    sr: f32,
}

impl Strum {
    pub const HOLD_OFF_MS: f32 = 40.0;

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sr = sample_rate;
        self.reset();
    }

    pub fn reset(&mut self) {
        self.clock = 0;
        self.last = None;
        self.last_jump = None;
    }

    /// Advances one sample (call once per sample, before [`Strum::decide`]).
    #[inline]
    pub fn tick(&mut self) {
        self.clock += 1;
    }

    /// Whether an onset starts a swell (true) or is absorbed. `jump_up` is the direction of a pitch
    /// jump. A manual onset always starts one.
    pub fn decide(&mut self, src: Source, strum_ms: f32, jump_up: bool) -> bool {
        let t = self.clock;
        let ms = |n: u64| n as f32 * 1000.0 / self.sr;
        let since = self.last.map(|l| ms(t - l));
        let mut accept = match src {
            Source::Manual => true,
            Source::Flux | Source::Pitch => since.is_none_or(|s| s >= strum_ms.max(Self::HOLD_OFF_MS)),
            Source::Level | Source::Pick => since.is_none_or(|s| s >= strum_ms),
        };
        if src == Source::Pitch {
            if let Some((tj, up)) = self.last_jump {
                if up == jump_up && ms(t - tj) < strum_ms {
                    accept = false;
                }
            }
            self.last_jump = Some((t, jump_up));
        }
        if accept {
            self.last = Some(t);
        }
        accept
    }
}

// ---------------------------------------------------------------------------------------------
// Strength: a continuous onset strength

/// A continuous onset strength o in 0 – 1, no event (design: gfx-widener/README.md §2.4):
/// - e_f = peak follower of |x| (attack 0.5 ms, release 100 ms), in dB;
/// - e_s = e_f smoothed by a 20 ms one-pole while it rises, equal to e_f while it falls, and never
///   more than `DEAD_DB` + `RANGE_DB` under it;
/// - o = clamp((e_f − e_s − `DEAD_DB`) / `RANGE_DB`, 0, 1).
///
/// The slow envelope follows the fast *envelope*, not the signal: two followers of |x| with different
/// times never agree on a sustained low note (a permanent partial strength, rippling at the note's
/// rate); this cascade reads ≈ 0 on a sustain and 1 on a pick after silence.
#[derive(Clone, Debug)]
pub struct Strength {
    fast: PeakFollower,
    slow_db: f32,
    coef: f32,
    o: f32,
}

impl Default for Strength {
    fn default() -> Self {
        Self { fast: PeakFollower::new(0.5, 100.0), slow_db: DB_FLOOR, coef: 0.0, o: 0.0 }
    }
}

impl Strength {
    pub const DEAD_DB: f32 = 1.5;
    pub const RANGE_DB: f32 = 9.0;
    pub const SLOW_MS: f32 = 20.0;

    pub fn prepare(&mut self, sample_rate: f32) {
        self.fast.prepare(sample_rate);
        self.coef = one_pole_coef((Self::SLOW_MS * 0.001 * sample_rate) as f64) as f32;
    }

    pub fn reset(&mut self) {
        self.fast.reset();
        self.slow_db = DB_FLOOR;
        self.o = 0.0;
    }

    /// The last strength.
    #[inline]
    pub fn value(&self) -> f32 {
        self.o
    }

    /// One sample; returns the strength.
    #[inline]
    pub fn next(&mut self, x: f32) -> f32 {
        let f = to_db(self.fast.next(x) as f64);
        self.slow_db = if f < self.slow_db { f } else { f + (self.slow_db - f) * self.coef };
        self.slow_db = self.slow_db.max(f - (Self::DEAD_DB + Self::RANGE_DB));
        self.o = ((f - self.slow_db - Self::DEAD_DB) / Self::RANGE_DB).clamp(0.0, 1.0);
        self.o
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48000.0;

    fn pluck(n: usize, f: f32, sr: f32, at: usize, amp: f32) -> Vec<f32> {
        // a decaying harmonic tone with a 1 ms noisy attack
        let mut r = crate::lfo::Rng::new(5);
        (0..n)
            .map(|i| {
                if i < at {
                    return 0.0;
                }
                let t = (i - at) as f32 / sr;
                let mut s = 0.0;
                for h in 1..8 {
                    s += (2.0 * std::f32::consts::PI * f * h as f32 * t).sin() / h as f32;
                }
                let click = if t < 0.001 { r.bipolar() * (1.0 - t / 0.001) } else { 0.0 };
                amp * ((-t * 1.5).exp() * s * 0.5 + click)
            })
            .collect()
    }

    #[test]
    fn level_and_floor_follow_the_playing() {
        let mut l = Level::default();
        l.prepare(SR);
        for _ in 0..4800 {
            l.next(0.5, -66.0);
        }
        assert!((l.note_db() + 6.0).abs() < 0.2, "{}", l.note_db());
        assert!((l.floor_db(33.0, -66.0) + 39.0).abs() < 0.3);
        // silence: held 5 s, then 3 dB / s, never under thresh
        for _ in 0..(SR * 5.0) as usize {
            l.next(0.0, -66.0);
        }
        assert!((l.phrase_db() + 6.0).abs() < 0.3, "{}", l.phrase_db());
        for _ in 0..(SR * 2.0) as usize {
            l.next(0.0, -66.0);
        }
        assert!((l.phrase_db() + 12.0).abs() < 0.5, "{}", l.phrase_db());
        for _ in 0..(SR * 30.0) as usize {
            l.next(0.0, -66.0);
        }
        assert_eq!(l.phrase_db(), -66.0);
    }

    #[test]
    fn level_gate_needs_one_ms_and_rearms_below() {
        let mut g = LevelGate::default();
        g.prepare(SR);
        g.reset();
        let mut fired = 0;
        for i in 0..4800 {
            if g.next(-20.0, -40.0, true) {
                fired += 1;
                assert_eq!(i + 1, 48);
            }
        }
        assert_eq!(fired, 1);
        // still above: no re-fire; just under, then above: no; 6 dB under, then above: fires again
        g.next(-41.0, -40.0, true);
        assert!(!(0..96).any(|_| g.next(-20.0, -40.0, true)));
        g.next(-47.0, -40.0, true);
        assert!((0..48).any(|_| g.next(-20.0, -40.0, true)));
    }

    #[test]
    fn pick_fires_once_per_pluck_at_any_level_and_rate() {
        for sr in [44100.0, 48000.0, 96000.0] {
            for amp in [1.0, 0.25, 0.06] {
                let n = (sr * 2.0) as usize;
                let mut x = pluck(n, 196.0, sr, (0.1 * sr) as usize, amp);
                let y = pluck(n, 247.0, sr, (1.1 * sr) as usize, amp);
                x.iter_mut().zip(&y).for_each(|(a, b)| *a += b);
                let mut p = Pick::default();
                p.prepare(sr);
                p.set(PickSettings::default());
                let hits: Vec<usize> = (0..n).filter(|&i| p.next(x[i], x[i] != 0.0 || i > 0)).collect();
                assert_eq!(hits.len(), 2, "sr {sr} amp {amp}: {hits:?}");
                for (h, at) in hits.iter().zip([0.1, 1.1]) {
                    let late = (*h as f32 - at * sr) / sr * 1000.0;
                    assert!((0.0..1.5).contains(&late), "late {late} ms");
                }
            }
        }
    }

    #[test]
    fn pick_ignores_stationary_noise() {
        let mut r = crate::lfo::Rng::new(9);
        let mut p = Pick::default();
        p.prepare(SR);
        p.set(PickSettings { rise_db: Pick::rise_for(0.75), ..Default::default() });
        let n = (SR * 5.0) as usize;
        let hits = (0..n).filter(|_| p.next(0.1 * r.bipolar(), true)).count();
        // the first sample of sound may fire (from silence); nothing after
        assert!(hits <= 1, "{hits}");
    }

    #[test]
    fn pick_on_an_external_level() {
        // a level in dB that steps up 20 dB twice: two onsets, with a 3 dB ripple in between none
        let mut p = Pick::default();
        p.prepare(SR);
        p.set(PickSettings { rise_db: 6.0, slope_ms: 3.0, hold_ms: 0.0, refractory_ms: 40.0 });
        let mut hits = vec![];
        for i in 0..(SR * 2.0) as usize {
            let t = i as f32 / SR;
            let base = if t < 0.5 { -60.0 } else if t < 1.2 { -40.0 } else { -20.0 };
            let l = base + 1.5 * (2.0 * std::f32::consts::PI * 4.0 * t).sin();
            if p.detect(l, l > -70.0) {
                hits.push(i);
            }
        }
        assert_eq!(hits.len(), 3, "{hits:?}"); // the first sample (from the floor), 0.5 s, 1.2 s
        assert_eq!(hits[1], (0.5 * SR) as usize);
    }

    #[test]
    fn flux_quiet_on_steady_tone_fires_on_new_chord() {
        let n = (SR * 2.0) as usize;
        let tone = |f: f32, at: f32| -> Vec<f32> {
            (0..n)
                .map(|i| {
                    let t = i as f32 / SR - at;
                    if t < 0.0 {
                        0.0
                    } else {
                        (1..6).map(|h| (2.0 * std::f32::consts::PI * f * h as f32 * t).sin() * 0.1 / h as f32).sum::<f32>()
                            * (1.0 - (-t * 400.0).exp())
                    }
                })
                .collect()
        };
        let a = tone(110.0, 0.0);
        let b = tone(415.3, 1.0);
        let x: Vec<f32> = a.iter().zip(&b).map(|(p, q)| p + q).collect();
        let mut l = Level::default();
        l.prepare(SR);
        let mut f = Flux::default();
        f.prepare(SR);
        let mut hits = vec![];
        for i in 0..n {
            l.next(x[i], -66.0);
            if f.next(x[i], l.phrase_db(), Flux::delta_for(0.5)) {
                hits.push(i as f32 / SR);
            }
        }
        // the first note from silence, then the new note at 1 s (within 15 ms)
        assert_eq!(hits.len(), 2, "{hits:?}");
        assert!(hits[1] > 1.0 && hits[1] < 1.015, "{hits:?}");
    }

    #[test]
    fn pitch_jump_on_a_semitone_not_on_vibrato() {
        let n = (SR * 1.0) as usize;
        let run = |f: &dyn Fn(f32) -> f32| -> Vec<(f32, Jump)> {
            let mut c = PitchJump::default();
            c.prepare(SR);
            let mut ph = 0.0f64;
            let mut hits = vec![];
            for i in 0..n {
                let t = i as f32 / SR;
                ph += f(t) as f64 / SR as f64;
                let x = (0.3 * (std::f64::consts::TAU * ph).sin() + 0.1 * (2.0 * std::f64::consts::TAU * ph).sin()) as f32;
                if let Some(j) = c.next(x, PitchJump::clarity_for(0.5)) {
                    hits.push((t, j));
                }
            }
            hits
        };
        let jump = run(&|t| if t < 0.5 { 330.0 } else { 349.23 });
        assert_eq!(jump.len(), 1, "{jump:?}");
        assert!(jump[0].0 > 0.5 && jump[0].0 < 0.518, "{jump:?}");
        assert!((jump[0].1.cents - 100.0).abs() < 10.0);
        let vib = run(&|t| 330.0 * 2f32.powf((100.0 * (2.0 * std::f32::consts::PI * 7.0 * t).sin()) / 1200.0));
        assert!(vib.is_empty(), "{vib:?}");
    }

    #[test]
    fn strum_absorbs_and_slides_merge() {
        let mut s = Strum::default();
        s.prepare(SR);
        let at = |s: &mut Strum, ms: f32, src: Source, up: bool| {
            let target = (ms * 0.001 * SR) as u64;
            while s.clock < target {
                s.tick();
            }
            s.decide(src, 60.0, up)
        };
        assert!(at(&mut s, 0.0, Source::Pick, true));
        assert!(!at(&mut s, 12.0, Source::Pick, true));
        assert!(!at(&mut s, 50.0, Source::Pitch, true));
        assert!(at(&mut s, 100.0, Source::Pick, true));
        // B / C within 40 ms of an onset are absorbed whatever `strum` is
        assert!(!s.decide(Source::Flux, 20.0, true));
        // a slide: jumps up every 30 ms → one swell
        assert!(at(&mut s, 300.0, Source::Pitch, true));
        for k in 1..5 {
            assert!(!at(&mut s, 300.0 + 30.0 * k as f32, Source::Pitch, true));
        }
        // a jump down after the slide is a new note
        assert!(at(&mut s, 520.0, Source::Pitch, false));
    }

    #[test]
    fn strength_fires_on_picks_and_rests_on_a_low_sustain() {
        for sr in [44100.0f32, 48000.0, 96000.0, 192000.0] {
            let mut o = Strength::default();
            o.prepare(sr);
            o.reset();
            // an 82 Hz note (7 harmonics, slow decay) picked after silence
            let n = (1.5 * sr) as usize;
            let at = (0.2 * sr) as usize;
            let x = pluck(n, 82.4, sr, at, 0.5);
            let y: Vec<f32> = x.iter().map(|&v| o.next(v)).collect();
            let attack = y[at..at + (0.01 * sr) as usize].iter().fold(0.0f32, |m, &v| m.max(v));
            let worst = y[at + (0.25 * sr) as usize..].iter().fold(0.0f32, |m, &v| m.max(v));
            eprintln!("{sr}: attack strength {attack:.3}, sustain ≤ {worst:.4}");
            assert!(attack > 0.95, "{sr}: attack strength {attack}");
            assert!(worst < 0.05, "{sr}: sustain strength up to {worst}");
            assert!(y[..at].iter().all(|&v| v == 0.0));
        }
    }
}
