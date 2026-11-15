// SPDX-License-Identifier: GPL-3.0-or-later
//! Pitch-synchronous analysis and synthesis of a monophonic voice (TD-PSOLA: Moulines & Charpentier,
//! Speech Communication 1990). Moved here from gfx-golem without behaviour change (gfx-golem's renders
//! are bit-exact before and after the move) and shared by gfx-golem, gfx-correct and gfx-formant.
//!
//! - [`Tracker`]: the input history, the YIN detector (`detect`), the octave-error rejection, the voicing
//!   gate (`sens`, `thresh`) and the **glottal epoch tracker**: marks predicted at the previous mark + the
//!   smoothed period, searched within ±25 % on a 1 kHz low-passed copy, polarity held per voiced segment,
//!   refined to sub-sample by a parabola, the low-pass group delay compensated. It returns the marks, the
//!   smoothed period, the clarity, the gate and the predicted next mark. **Backward seeding** (an option,
//!   off for gfx-golem): at a voicing onset the tracker also walks back through its history from the
//!   first mark, so the first cycle periods come with YIN's first reading instead of periods later.
//! - [`Reader`]: a **fixed-delay streaming reader**. Every output sample reads the active grains directly
//!   (asymmetric Hann halves on the epochs, gfx-golem's shape); output time t reads grain k at input
//!   position m_k + (t − D − s_k) · α, with synthesis marks s spaced T / r. Each synthesis mark takes the
//!   **latest** analysis mark at or before it. The delay D is the caller's (≥ 1.25 · T_max + gd + 2
//!   samples). When the next mark is not confirmed in time the reader **commits the predicted mark** (the
//!   period clamped to T_max) as the grain boundary; both halves meeting there use it, so they still sum
//!   to one. Per grain: r, α and a gain. At r = α = 1 the output is the input delayed by D (identity, voiced
//!   or not: with no marks the reader freewheels on committed marks). The caller owns the gate.
//! - Helpers: [`REGISTERS`], [`voicing_thresholds`], [`level_norm`], [`hermite`].

use crate::detect::PitchDetector;
use crate::svf::{Svf, SvfMode};

/// `register`: (lowest, highest) sung fundamental, Hz.
pub const REGISTERS: [(f32, f32); 3] = [(60.0, 500.0), (90.0, 800.0), (150.0, 1200.0)];
/// The epoch marker's low-pass (Hz).
pub const MARK_LP_HZ: f32 = 1000.0;
/// Octave-error rejection: a ×2 / ÷2 jump must last this long to be believed (ms).
pub const OCTAVE_CONFIRM_MS: f32 = 40.0;
/// Voicing: hold (ms), gain ramps (ms).
pub const HOLD_MS: f32 = 30.0;
pub const OPEN_MS: f32 = 10.0;
pub const CLOSE_MS: f32 = 30.0;
/// Marks the tracker keeps.
pub const MARKS: usize = 16;
/// Earlier marks found by backward seeding at most.
const SEED_BACK: usize = 3;

/// Voicing thresholds (clarity) for `sens` 0 – 100 %: (open, close).
pub fn voicing_thresholds(sens: f32) -> (f32, f32) {
    let s = (sens / 100.0).clamp(0.0, 1.0);
    // 0 % → 0.9 / 0.8, 50 % → 0.8 / 0.65, 100 % → 0.7 / 0.5
    let lerp = |a: f32, b: f32, c: f32| if s < 0.5 { a + (b - a) * s * 2.0 } else { b + (c - b) * (s - 0.5) * 2.0 };
    (lerp(0.9, 0.8, 0.7), lerp(0.8, 0.65, 0.5))
}

/// A shifted voice's level normalisation for an interval ratio `r` and a formant ratio `alpha`: the
/// grains' overlap scales the output power by about r / alpha.
pub fn level_norm(r: f64, alpha: f64) -> f64 {
    (alpha / r).sqrt().clamp(0.25, 4.0)
}

/// Cubic Hermite read of a power-of-two ring at an absolute position. Exact at integer positions.
#[inline]
pub fn hermite(buf: &[f32], mask: usize, pos: f64) -> f32 {
    let i = pos.floor();
    let f = (pos - i) as f32;
    let i = i as i64;
    let at = |k: i64| buf[(k as usize) & mask];
    let (y0, y1, y2, y3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
    let c0 = y1;
    let c1 = 0.5 * (y2 - y0);
    let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
    ((c3 * f + c2) * f + c1) * f + c0
}

/// What one [`Tracker::push`] produced.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Events {
    /// a detector analysis was made (the voicing was updated)
    pub frame: bool,
    /// a new epoch mark was appended
    pub mark: bool,
}

/// The epoch tracker and voicing gate (see the module docs).
#[derive(Clone)]
pub struct Tracker {
    sr: f64,
    t: u64,
    hist: Vec<f32>,
    lpbuf: Vec<f32>,
    mask: usize,
    mark_lp: Svf,
    gd: f64,
    det: PitchDetector,
    det_hop: f64,
    a_level: f64,
    register: usize,
    sens: f32,
    thresh: f32,
    backward_seed: bool,
    f_det: f32,
    octave_pending: u32,
    level: f64,
    gate_open: bool,
    agree: u32,
    hold: u32,
    pitched_quiet: bool,
    active: bool,
    pol: f32,
    marks: [f64; MARKS],
    nmarks: usize,
    mark_index: u64,
    period: f64,
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
            t: 0,
            hist: Vec::new(),
            lpbuf: Vec::new(),
            mask: 0,
            mark_lp: Svf::new(),
            gd: 0.0,
            det: PitchDetector::default(),
            det_hop: 128.0,
            a_level: 0.0,
            register: 1,
            sens: 50.0,
            thresh: -50.0,
            backward_seed: false,
            f_det: 0.0,
            octave_pending: 0,
            level: 0.0,
            gate_open: false,
            agree: 0,
            hold: 0,
            pitched_quiet: false,
            active: false,
            pol: 1.0,
            marks: [0.0; MARKS],
            nmarks: 0,
            mark_index: 0,
            period: 0.0,
        }
    }

    /// Sizes the history (`history_s` seconds, rounded up to a power of two), the detector (whole
    /// register table) and the marker low-pass; resets.
    pub fn prepare(&mut self, sample_rate: f32, history_s: f64) {
        self.sr = sample_rate as f64;
        let need = (history_s * self.sr) as usize;
        let len = need.next_power_of_two();
        self.hist = vec![0.0; len];
        self.lpbuf = vec![0.0; len];
        self.mask = len - 1;
        self.mark_lp.set(SvfMode::LowPass, MARK_LP_HZ, std::f32::consts::FRAC_1_SQRT_2, 0.0, sample_rate);
        // second-order Butterworth low-pass group delay at low frequency: √2 / ωc
        self.gd = self.sr * std::f64::consts::SQRT_2 / (2.0 * std::f64::consts::PI * MARK_LP_HZ as f64);
        self.det.prepare_range(sample_rate, REGISTERS[0].0, REGISTERS[2].1, false);
        self.det_hop = 32.0 * (self.sr / 12000.0).round().max(1.0);
        let (lo, hi) = REGISTERS[self.register.min(2)];
        self.det.set_range(lo, hi);
        self.a_level = 1.0 - (-1.0 / (0.020 * self.sr)).exp();
        self.reset();
    }

    pub fn reset(&mut self) {
        self.t = 0;
        for b in [&mut self.hist, &mut self.lpbuf] {
            b.iter_mut().for_each(|x| *x = 0.0);
        }
        self.mark_lp.reset();
        self.det.reset();
        self.f_det = 0.0;
        self.octave_pending = 0;
        self.level = 0.0;
        self.gate_open = false;
        self.agree = 0;
        self.hold = 0;
        self.pitched_quiet = false;
        self.active = false;
        self.nmarks = 0;
        self.mark_index = 0;
        self.period = 0.0;
    }

    /// `register` (0 Low · 1 Mid · 2 High): the detector's range (no allocation).
    pub fn set_register(&mut self, register: usize) {
        if register != self.register {
            self.register = register;
            if !self.hist.is_empty() {
                let (lo, hi) = REGISTERS[register.min(2)];
                self.det.set_range(lo, hi);
            }
        }
    }

    /// Narrows (or restores) the detector's range inside the register (no allocation).
    pub fn set_detector_range(&mut self, lo: f32, hi: f32) {
        self.det.set_range(lo, hi);
    }

    /// Voicing sensitivity (0 – 100 %) and level threshold (dB).
    pub fn set_gate(&mut self, sens: f32, thresh: f32) {
        self.sens = sens;
        self.thresh = thresh;
    }

    /// Backward seeding at a voicing onset (off by default: gfx-golem).
    pub fn set_backward_seed(&mut self, on: bool) {
        self.backward_seed = on;
    }

    /// The marker low-pass group delay (samples).
    pub fn group_delay(&self) -> f64 {
        self.gd
    }

    /// Time (samples) of the next sample to be pushed.
    pub fn time(&self) -> u64 {
        self.t
    }

    pub fn history(&self) -> &[f32] {
        &self.hist
    }

    pub fn mask(&self) -> usize {
        self.mask
    }

    pub fn gate_open(&self) -> bool {
        self.gate_open
    }

    /// Pitched (in range, clear) but under the level threshold.
    pub fn pitched_quiet(&self) -> bool {
        self.pitched_quiet
    }

    /// The gate's verdict as a meter: 0 idle, 1 tracking, 2 pitched but gated by level.
    pub fn state(&self) -> u8 {
        if self.gate_open {
            1
        } else if self.pitched_quiet {
            2
        } else {
            0
        }
    }

    /// The octave-guarded detector pitch (Hz; 0 before the first reading).
    pub fn f_det(&self) -> f32 {
        self.f_det
    }

    pub fn clarity(&self) -> f32 {
        self.det.clarity()
    }

    /// The detector's raw estimate of its last analysis (Hz).
    pub fn raw_hz(&self) -> f32 {
        self.det.period_hz()
    }

    /// The tracker follows epochs.
    pub fn active(&self) -> bool {
        self.active
    }

    /// The smoothed period (samples); meaningful when [`Self::active`].
    pub fn period(&self) -> f64 {
        self.period
    }

    /// The marks, oldest first (absolute input times, samples).
    pub fn marks(&self) -> &[f64] {
        &self.marks[..self.nmarks]
    }

    /// Marks appended since reset.
    pub fn mark_index(&self) -> u64 {
        self.mark_index
    }

    /// The next epoch as predicted: the last mark + the smoothed period.
    pub fn predicted_next(&self) -> Option<f64> {
        (self.active && self.nmarks > 0).then(|| self.marks[self.nmarks - 1] + self.period)
    }

    /// Feeds one input sample: history, level, detector, voicing, epochs.
    #[inline]
    pub fn push(&mut self, x: f32) -> Events {
        let t = self.t as usize;
        self.hist[t & self.mask] = x;
        self.lpbuf[t & self.mask] = self.mark_lp.process(x);
        self.level += self.a_level * ((x as f64) * (x as f64) - self.level);
        let mut ev = Events::default();
        if self.det.push(x) {
            self.voicing();
            ev.frame = true;
        }
        ev.mark = self.track();
        self.t += 1;
        ev
    }

    /// The best (polarity-weighted, penalised) low-passed peak near `pred`, refined by a parabola:
    /// the mark's time and its peak value.
    fn search(&self, pred: f64, half: f64) -> (f64, f32) {
        let lo = (pred - half + self.gd).ceil() as i64;
        let hi = (pred + half + self.gd).floor() as i64;
        let mut best = lo;
        let mut bs = f32::MIN;
        for k in lo..=hi {
            let d = (k as f64 - self.gd - pred) / half;
            let pen = 1.0 - 0.3 * (d * d) as f32;
            let v = self.pol * self.lpbuf[(k as usize) & self.mask] * pen;
            if v > bs {
                bs = v;
                best = k;
            }
        }
        let y = |k: i64| self.pol * self.lpbuf[(k as usize) & self.mask];
        let (y0, y1, y2) = (y(best - 1), y(best), y(best + 1));
        let den = y0 - 2.0 * y1 + y2;
        let off = if den.abs() > 1e-12 { (0.5 * (y0 - y2) / den).clamp(-0.5, 0.5) } else { 0.0 } as f64;
        (best as f64 + off - self.gd, y1)
    }

    /// The epoch tracker, every sample. True when a mark was appended.
    fn track(&mut self) -> bool {
        let t = self.t as f64;
        let tdet = self.sr / self.f_det.max(1.0) as f64;
        if !self.gate_open && self.hold == 0 {
            self.active = false;
            self.nmarks = 0;
            return false;
        }
        if self.f_det <= 0.0 {
            return false;
        }
        if !self.active {
            // start: polarity from the last two periods, first mark on the strongest peak of the last one
            let span = (2.0 * tdet) as i64;
            let (mut mx, mut mn) = (f32::MIN, f32::MAX);
            for k in 0..span {
                let v = self.lpbuf[((self.t as i64 - k) as usize) & self.mask];
                mx = mx.max(v);
                mn = mn.min(v);
            }
            self.pol = if mx >= -mn { 1.0 } else { -1.0 };
            let mut best = 0i64;
            let mut bv = f32::MIN;
            for k in 0..(tdet as i64) {
                let v = self.pol * self.lpbuf[((self.t as i64 - k) as usize) & self.mask];
                if v > bv {
                    bv = v;
                    best = k;
                }
            }
            self.marks[0] = t - best as f64 - self.gd;
            self.nmarks = 1;
            self.period = tdet;
            self.active = true;
            if self.backward_seed {
                self.seed_back(bv);
            }
            return false;
        }
        let last = self.marks[self.nmarks - 1];
        let pred = last + self.period;
        let half = 0.25 * self.period;
        // the search window must be fully in the past of the low-passed signal
        if t < pred + half + self.gd + 2.0 {
            return false;
        }
        let (m, _) = self.search(pred, half);
        let measured = m - last;
        if (measured - tdet).abs() < 0.3 * tdet {
            self.period += 0.3 * (measured - self.period);
        }
        self.period += 0.05 * (tdet - self.period);
        if self.nmarks == MARKS {
            self.marks.copy_within(1.., 0);
            self.nmarks -= 1;
        }
        self.marks[self.nmarks] = m;
        self.nmarks += 1;
        self.mark_index += 1;
        true
    }

    /// Backward seeding: earlier marks one period apart, while the peaks keep at least a third of the
    /// first mark's (the voice was already there) and the history holds them.
    fn seed_back(&mut self, peak: f32) {
        let tdet = self.period;
        let half = 0.25 * tdet;
        let oldest = self.t as f64 - (self.mask as f64 - 4.0 * tdet).max(0.0);
        let mut found = [0.0f64; SEED_BACK];
        let mut n = 0;
        let mut m = self.marks[0];
        while n < SEED_BACK {
            let pred = m - tdet;
            if pred - half < oldest + 2.0 {
                break;
            }
            let (mk, v) = self.search(pred, half);
            if v < 0.33 * peak || (m - mk - tdet).abs() > 0.3 * tdet {
                break;
            }
            found[n] = mk;
            n += 1;
            m = mk;
        }
        if n == 0 {
            return;
        }
        let first = self.marks[0];
        for i in 0..n {
            self.marks[i] = found[n - 1 - i];
        }
        self.marks[n] = first;
        self.nmarks = n + 1;
        // the seeded cycles give the period at once
        self.period = (first - self.marks[0]) / n as f64;
    }

    /// The detector's verdict → octave-error rejection and the voicing gate.
    fn voicing(&mut self) {
        let f = self.det.period_hz();
        let c = self.det.clarity();
        let (lo, hi) = REGISTERS[self.register.min(2)];
        let in_range = f >= lo && f <= hi;
        // octave errors: a ×2 / ÷2 jump must persist before it is believed
        let frames_confirm = ((OCTAVE_CONFIRM_MS * 0.001) as f64 * self.sr / self.det_hop).ceil() as u32;
        let mut f_new = f;
        if self.f_det > 0.0 && in_range {
            let r = f / self.f_det;
            if ((r - 2.0).abs() < 0.12 || (r - 0.5).abs() < 0.03) && self.octave_pending < frames_confirm {
                self.octave_pending += 1;
                f_new = self.f_det;
            } else {
                self.octave_pending = 0;
            }
        }
        let (open_th, close_th) = voicing_thresholds(self.sens);
        let level_ok = self.level > 10f64.powf(self.thresh as f64 / 10.0);
        let pitched = in_range && c >= close_th;
        self.pitched_quiet = pitched && !level_ok;
        let hold_frames = ((HOLD_MS * 0.001) as f64 * self.sr / self.det_hop).ceil() as u32;
        if !self.gate_open {
            self.agree = if in_range && c >= open_th && level_ok { self.agree + 1 } else { 0 };
            if self.agree >= 3 {
                self.gate_open = true;
                self.hold = hold_frames;
            }
        } else if pitched && level_ok {
            self.hold = hold_frames;
        } else if self.hold > 0 {
            self.hold -= 1;
        } else {
            self.gate_open = false;
            self.agree = 0;
        }
        if in_range && (self.gate_open || c >= open_th) {
            self.f_det = f_new;
        }
    }
}

/// Grains the reader can hold at once (r up to about 3 at α ≥ 0.5).
const GRAINS: usize = 8;
/// Analysis marks the reader keeps.
const RMARKS: usize = 64;

#[derive(Clone, Copy, Debug, Default)]
struct Grain {
    /// synthesis centre (input-time domain: output time − D)
    s: f64,
    /// source analysis mark
    a: f64,
    /// output half lengths
    lo: f64,
    ro: f64,
    alpha: f64,
    gain: f32,
}

/// The fixed-delay streaming reader (see the module docs). One voice; a caller with two voices runs two.
#[derive(Clone, Debug)]
pub struct Reader {
    delay: f64,
    t_max: f64,
    period: f64,
    marks: [f64; RMARKS],
    nmarks: usize,
    grains: [Grain; GRAINS],
    ngrains: usize,
    /// next synthesis mark (input-time domain); NaN before the first
    next: f64,
    committed: u64,
}

impl Default for Reader {
    fn default() -> Self {
        Self::new()
    }
}

impl Reader {
    pub fn new() -> Self {
        Self {
            delay: 0.0,
            t_max: 800.0,
            period: 400.0,
            marks: [0.0; RMARKS],
            nmarks: 0,
            grains: [Grain::default(); GRAINS],
            ngrains: 0,
            next: f64::NAN,
            committed: 0,
        }
    }

    /// The delay D (samples, whole) and the longest period the reader predicts with (T_max). Resets.
    pub fn configure(&mut self, delay: usize, t_max: f64) {
        self.delay = delay as f64;
        self.t_max = t_max.max(4.0);
        self.reset();
    }

    pub fn delay(&self) -> usize {
        self.delay as usize
    }

    pub fn reset(&mut self) {
        self.period = 0.5 * self.t_max;
        self.nmarks = 0;
        self.ngrains = 0;
        self.next = f64::NAN;
        self.committed = 0;
    }

    /// Predicted marks committed as grain boundaries since reset (a diagnostic).
    pub fn committed(&self) -> u64 {
        self.committed
    }

    /// A new analysis mark from the tracker, with the tracker's smoothed period. A mark within half a
    /// period of a committed (predicted) one is that one: the boundary already used stays.
    pub fn push_mark(&mut self, m: f64, period: f64) {
        self.period = period.clamp(2.0, self.t_max);
        if self.nmarks > 0 && m <= self.marks[self.nmarks - 1] + 0.5 * self.period {
            return;
        }
        // a gap (the voice's return after freewheeling, a note under the register's floor) is bridged
        // by evenly spaced committed marks, so no boundary is ever more than T_max from the next
        if self.nmarks > 0 {
            let last = self.marks[self.nmarks - 1];
            let gap = m - last;
            if gap > (1.5 * self.period).min(self.t_max) {
                let n = (gap / self.period).round().max((gap / self.t_max).ceil()).max(1.0) as usize;
                let step = gap / n as f64;
                for i in 1..n {
                    self.append(last + step * i as f64);
                    self.committed += 1;
                }
            }
        }
        self.append(m);
    }

    /// The period predicted marks are spaced by when the voice has none (unvoiced freewheel).
    pub fn set_freewheel_period(&mut self, period: f64) {
        self.period = period.clamp(2.0, self.t_max);
    }

    fn append(&mut self, m: f64) {
        if self.nmarks == RMARKS {
            self.marks.copy_within(1.., 0);
            self.nmarks -= 1;
        }
        self.marks[self.nmarks] = m;
        self.nmarks += 1;
    }

    /// Commits predicted marks until one lies after `s`.
    fn extend_past(&mut self, s: f64) {
        if self.nmarks == 0 {
            self.append(s.floor());
            self.committed += 1;
        }
        while self.marks[self.nmarks - 1] <= s {
            let m = self.marks[self.nmarks - 1] + self.period;
            self.append(m);
            self.committed += 1;
        }
    }

    /// The left half (input samples) of a grain at synthesis mark `s`, when its source mark and the one
    /// before are known.
    fn left_of(&self, s: f64) -> Option<f64> {
        let n = self.nmarks;
        if n < 2 || self.marks[n - 1] <= s {
            return None;
        }
        let mut j = n - 1;
        while j > 0 && self.marks[j] > s {
            j -= 1;
        }
        (j > 0 && self.marks[j] <= s).then(|| self.marks[j] - self.marks[j - 1])
    }

    /// Starts the grain at synthesis mark `s`; returns the next synthesis mark.
    fn start_grain(&mut self, s: f64, ctl: &mut impl FnMut(f64, f64) -> (f64, f64, f32)) -> f64 {
        self.extend_past(s);
        // the latest analysis mark at or before s
        let mut j = self.nmarks - 1;
        while j > 0 && self.marks[j] > s {
            j -= 1;
        }
        let a = self.marks[j];
        let (a, j) = if a > s { (s, usize::MAX) } else { (a, j) };
        let prev = if j != usize::MAX && j > 0 { self.marks[j - 1] } else { a - self.period };
        let nxt = if j != usize::MAX { self.marks[j + 1] } else { a + self.period };
        let left = (a - prev).clamp(2.0, self.t_max);
        let right = (nxt - a).clamp(2.0, self.t_max);
        let (r, alpha, gain) = ctl(a + 0.5 * right, right);
        let (r, alpha) = (r.clamp(0.25, 4.0), alpha.clamp(0.25, 4.0));
        let lo = if alpha > 1.0 { left / alpha } else { left };
        let ro = right / alpha;
        if self.ngrains == GRAINS {
            // the oldest grain gives way (only with extreme ratios)
            self.grains.copy_within(1.., 0);
            self.ngrains -= 1;
        }
        self.grains[self.ngrains] = Grain { s, a, lo, ro, alpha, gain };
        self.ngrains += 1;
        let mut next = s + right / r;
        // land exactly on the next analysis mark when the arithmetic only rounded away from it (r = 1)
        if j != usize::MAX && (next - nxt).abs() < 1e-6 {
            next = nxt;
        }
        next
    }

    /// The output for input time `t` (the newest sample written to `buf` at `t`): the voice at
    /// t − D, every grain with the interval ratio `r`, formant ratio `alpha` and `gain`. `buf` / `mask`
    /// is the history ring (the tracker's, or an anti-aliased copy).
    #[inline]
    pub fn process(&mut self, t: u64, buf: &[f32], mask: usize, r: f64, alpha: f64, gain: f32) -> f32 {
        self.process_with(t, buf, mask, &mut |_, _| (r, alpha, gain))
    }

    /// As [`Self::process`], with the grain's (r, α, gain) asked per grain as it starts:
    /// `ctl(source cycle centre, source period)`, both in input time (samples). The ratio sets the
    /// spacing to the next grain, so it is the one of the source cycle that follows the grain's mark.
    #[inline]
    pub fn process_with(&mut self, t: u64, buf: &[f32], mask: usize, ctl: &mut impl FnMut(f64, f64) -> (f64, f64, f32)) -> f32 {
        let tau = t as f64 - self.delay;
        if self.next.is_nan() {
            self.next = tau;
            if self.nmarks > 0 {
                // the first synthesis mark on the first analysis mark at or after now
                if let Some(&m) = self.marks[..self.nmarks].iter().find(|&&m| m >= tau) {
                    self.next = m;
                }
            }
        }
        // grains whose left half may begin by this sample: the left half of the next grain is the
        // cycle before its source mark when that mark is known, else at most 1.3 periods (the tracker
        // holds its marks within ±30 %), never more than T_max. Starting early costs nothing (the
        // window is zero before its left half)
        let mut guard = 0;
        while guard < GRAINS {
            let lo_bound = self.left_of(self.next).unwrap_or(1.3 * self.period).min(self.t_max);
            if self.next - lo_bound > tau + 1.0 && self.ngrains > 0 {
                break;
            }
            let s = self.next;
            self.next = self.start_grain(s, ctl);
            // a left half longer than the bound starts now, from zero
            let g = &mut self.grains[self.ngrains - 1];
            if tau - g.s > -g.lo {
                g.lo = (g.s - tau).max(1.0);
            }
            guard += 1;
        }
        // prune marks far behind
        let keep = tau - 3.0 * self.t_max;
        let mut drop = 0;
        while drop + 2 < self.nmarks && self.marks[drop + 1] < keep {
            drop += 1;
        }
        if drop > 0 {
            self.marks.copy_within(drop..self.nmarks, 0);
            self.nmarks -= drop;
        }
        let mut y = 0.0f32;
        let mut k = 0;
        while k < self.ngrains {
            let g = self.grains[k];
            let u = tau - g.s;
            if u > g.ro {
                self.grains.copy_within(k + 1..self.ngrains, k);
                self.ngrains -= 1;
                continue;
            }
            if u >= -g.lo {
                let w = if u < 0.0 {
                    0.5 * (1.0 + (std::f64::consts::PI * u / g.lo).cos())
                } else {
                    0.5 * (1.0 + (std::f64::consts::PI * u / g.ro).cos())
                };
                y += g.gain * (w as f32) * hermite(buf, mask, g.a + u * g.alpha);
            }
            k += 1;
        }
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic /a/: Rosenberg-like pulses through three formant resonators.
    fn vowel(f0: f64, sr: f64, n: usize, jitter: f64) -> Vec<f32> {
        let mut out = vec![0.0f32; n];
        let mut fs = [Svf::new(), Svf::new(), Svf::new()];
        for (s, (f, q)) in fs.iter_mut().zip([(700.0, 8.0), (1220.0, 10.0), (2600.0, 12.0)]) {
            s.set(SvfMode::BandPass, f, q, 0.0, sr as f32);
        }
        let mut ph = 0.0f64;
        let mut seed = 12345u32;
        let mut f = f0;
        for o in out.iter_mut() {
            ph += f / sr;
            if ph >= 1.0 {
                ph -= 1.0;
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                f = f0 * (1.0 + jitter * ((seed as f64 / u32::MAX as f64) * 2.0 - 1.0));
            }
            let g = if ph < 0.6 { (std::f64::consts::PI * ph / 0.6).sin().powi(2) } else { 0.0 };
            let x = g as f32;
            *o = 0.3 * (fs[0].process(x) + 0.6 * fs[1].process(x) + 0.3 * fs[2].process(x));
        }
        out
    }

    #[test]
    fn tracker_marks_follow_the_period() {
        let sr = 48000.0;
        let x = vowel(200.0, sr, 24000, 0.0);
        let mut tr = Tracker::new();
        tr.prepare(sr as f32, 0.4);
        for &v in &x {
            tr.push(v);
        }
        assert!(tr.active() && tr.gate_open());
        let m = tr.marks();
        assert!(m.len() >= 8);
        for w in m.windows(2) {
            let d = w[1] - w[0];
            assert!((d - 240.0).abs() < 1.0, "spacing {d}");
        }
        let p = tr.predicted_next().unwrap();
        assert!((p - m[m.len() - 1] - 240.0).abs() < 1.0);
    }

    #[test]
    fn backward_seeding_gives_marks_at_once() {
        let sr = 48000.0;
        let x = vowel(150.0, sr, 24000, 0.0);
        let mut first = [0usize; 2];
        for (i, seed) in [false, true].into_iter().enumerate() {
            let mut tr = Tracker::new();
            tr.set_backward_seed(seed);
            tr.prepare(sr as f32, 0.4);
            for &v in &x {
                tr.push(v);
                if tr.active() && tr.marks().len() >= 3 {
                    break;
                }
            }
            first[i] = tr.time() as usize;
            if seed {
                let m = tr.marks();
                for w in m.windows(2) {
                    assert!((w[1] - w[0] - 320.0).abs() < 4.0, "seeded spacing {:?}", m.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>());
                }
            }
        }
        // three marks known 1.5 periods sooner at least (the seeds stop at the vowel's onset)
        assert!(first[1] + 480 < first[0], "{first:?}");
    }

    #[test]
    fn reader_is_the_identity_at_unity() {
        let sr = 48000.0;
        let mut x = vowel(180.0, sr, 30000, 0.007);
        // an unvoiced burst in the middle
        let mut seed = 7u32;
        for v in x[12000..16000].iter_mut() {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            *v = 0.1 * ((seed as f32 / u32::MAX as f32) - 0.5);
        }
        let mut tr = Tracker::new();
        tr.prepare(sr as f32, 0.4);
        let mut rd = Reader::new();
        let d = 1200;
        rd.configure(d, sr / 90.0);
        let mut err = 0.0f64;
        for (i, &v) in x.iter().enumerate() {
            let ev = tr.push(v);
            if ev.mark {
                let m = tr.marks();
                rd.push_mark(m[m.len() - 1], tr.period());
            }
            let y = rd.process(i as u64, tr.history(), tr.mask(), 1.0, 1.0, 1.0);
            if i >= d {
                let e = (y - x[i - d]).abs() as f64;
                err = err.max(e);
            }
        }
        assert!(err < 1e-6, "max error {err}");
        assert!(rd.committed() > 0);
    }

    #[test]
    fn reader_shifts_the_pitch() {
        let sr = 48000.0;
        let x = vowel(200.0, sr, 48000, 0.0);
        let mut tr = Tracker::new();
        tr.prepare(sr as f32, 0.4);
        let mut rd = Reader::new();
        rd.configure(1500, sr / 90.0);
        let r = 2f64.powf(2.0 / 12.0);
        let mut y = Vec::new();
        for (i, &v) in x.iter().enumerate() {
            if tr.push(v).mark {
                let m = tr.marks();
                rd.push_mark(m[m.len() - 1], tr.period());
            }
            let rr = if tr.active() { r } else { 1.0 };
            y.push(rd.process(i as u64, tr.history(), tr.mask(), rr, 1.0, 1.0));
        }
        // the output's pitch, by the tracker itself
        let mut t2 = Tracker::new();
        t2.prepare(sr as f32, 0.4);
        for &v in &y {
            t2.push(v);
        }
        let f = sr / t2.period();
        assert!((f / 200.0 - r).abs() < 0.01, "pitch {f}");
    }
}
