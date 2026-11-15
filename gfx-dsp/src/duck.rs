// SPDX-License-Identifier: GPL-3.0-or-later
//! The ducking engine (design: gfx-ducker/README.md §2.2 – 2.7): what lowers a signal while another
//! one plays, shared so gfx-delay, gfx-reverb and gfx-shimmer can adopt it later.
//!
//! - [`KeyGate`]: the activation target a* = clamp((L − T_eff) / `knee`, 0, 1) (a step at `knee` 0),
//!   with hysteresis that shifts the whole curve by `hyst` on the way down.
//! - [`Activation`]: a moves **linearly** toward a* (up at 1 / attack, down at 1 / release after
//!   `hold`), so the duck is depth · S(a) with S the raised cosine [`s_curve`]: no steps, no corners.
//! - [`Dip`]: the fixed-shape dip of Trigger and Pump (a raised-cosine fall over `attack`, a stay for
//!   `hold`, a recovery in linear gain shaped by `tension`), restartable from the current gain, and
//!   [`dip_db`], the same curve read at any time (Pump's phase from the host position).
//! - Spectral: [`SpectralAnalysis`] (16 bands of two cascaded band-passes, Q 2, peak followers in
//!   dB), [`weights`], and [`SpectralPlan`]: the bells' interaction matrix, the cut-only projected
//!   Gauss-Seidel solve (bells in series add up), and the per-band coefficient tables (0 – 60 dB in
//!   0.5 dB steps) built in `prepare`, so a tick is an interpolation, not a matched design.
//!
//! Allocation in `prepare` only; deterministic.

use crate::envelope::PeakFollower;
use crate::matched::{design, Analog};
use crate::svf::{Svf, SvfCoefs, SvfMode};
use std::f64::consts::PI;

/// The raised-cosine map S(a) = (1 − cos πa) / 2, a ∈ [0, 1]: exactly 0 at 0 and 1 at 1.
#[inline]
pub fn s_curve(a: f64) -> f64 {
    if a <= 0.0 {
        0.0
    } else if a >= 1.0 {
        1.0
    } else {
        // (1 + sin x) / 2 with x = π (a − ½) ∈ (−π/2, π/2): an odd Taylor polynomial to x¹³
        // (error < 7e−10, a per-sample cost of a few multiplies instead of a libm cosine)
        let x = PI * (a - 0.5);
        let x2 = x * x;
        let s = x
            * (1.0
                + x2 * (-1.0 / 6.0
                    + x2 * (1.0 / 120.0
                        + x2 * (-1.0 / 5040.0 + x2 * (1.0 / 362_880.0 + x2 * (-1.0 / 39_916_800.0 + x2 * (1.0 / 6_227_020_800.0)))))));
        (0.5 * (1.0 + s)).clamp(0.0, 1.0)
    }
}

/// The inverse of [`s_curve`]: the activation whose duck is the share `s` of the depth.
#[inline]
pub fn s_inverse(s: f64) -> f64 {
    if s <= 0.0 {
        0.0
    } else if s >= 1.0 {
        1.0
    } else {
        (1.0 - 2.0 * s).acos() / PI
    }
}

/// The recovery shape r(u), u ∈ [0, 1] → [0, 1], with p = 4^(|`tension`| / 100):
/// `tension` ≥ 0 → u^p (stays low, returns late, smooth join with the hold);
/// `tension` < 0 → 1 − (1 − u)^p (snaps back early, lands smoothly on 0 dB). 0 is linear.
#[inline]
pub fn recovery(u: f64, tension: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    if tension == 0.0 {
        return u;
    }
    let p = 4f64.powf(tension.abs().min(100.0) / 100.0);
    if tension >= 0.0 {
        u.powf(p)
    } else {
        1.0 - (1.0 - u).powf(p)
    }
}

#[inline]
fn db_to_lin(db: f64) -> f64 {
    (db * (std::f64::consts::LN_10 / 20.0)).exp()
}

#[inline]
fn lin_to_db(g: f64) -> f64 {
    20.0 * g.max(1e-12).log10()
}

// ---------------------------------------------------------------------------------------------
// Follow: target and activation

/// The activation target with knee and hysteresis (§2.2). T_eff = T while closed, T + `hyst`
/// (`hyst` ≤ 0) from the moment a* leaves 0 until the level falls below T + `hyst`.
#[derive(Clone, Copy, Debug, Default)]
pub struct KeyGate {
    open: bool,
}

impl KeyGate {
    pub fn reset(&mut self) {
        self.open = false;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// a* for the key level `level_db` (dB), threshold `thresh`, `knee` (dB, 0 = a step) and `hyst`.
    #[inline]
    pub fn target(&mut self, level_db: f64, thresh: f64, knee: f64, hyst: f64) -> f64 {
        let hyst = hyst.min(0.0);
        if self.open && level_db < thresh + hyst {
            self.open = false;
        }
        let t_eff = if self.open { thresh + hyst } else { thresh };
        let a = if knee <= 0.0 {
            if level_db > t_eff {
                1.0
            } else {
                0.0
            }
        } else {
            ((level_db - t_eff) / knee).clamp(0.0, 1.0)
        };
        if a > 0.0 {
            self.open = true;
        }
        a
    }
}

/// The activation a ∈ [0, 1]: linear ramps toward a*, up at 1 / `attack`, down at 1 / `release` once
/// a* has stayed below a for `hold`. A new rise during hold or release resumes from the current a.
#[derive(Clone, Copy, Debug, Default)]
pub struct Activation {
    a: f64,
    up: f64,
    down: f64,
    hold_n: u32,
    hold: u32,
}

impl Activation {
    /// Times in seconds (attack and release floored at one sample).
    pub fn set_times(&mut self, attack_s: f64, hold_s: f64, release_s: f64, sample_rate: f64) {
        self.up = 1.0 / (attack_s * sample_rate).max(1.0);
        self.down = 1.0 / (release_s * sample_rate).max(1.0);
        self.hold_n = (hold_s.max(0.0) * sample_rate).round() as u32;
    }

    pub fn reset(&mut self) {
        self.a = 0.0;
        self.hold = 0;
    }

    #[inline]
    pub fn value(&self) -> f64 {
        self.a
    }

    /// One sample toward `target`; returns a.
    #[inline]
    pub fn next(&mut self, target: f64) -> f64 {
        if target > self.a {
            self.a = (self.a + self.up).min(target);
            self.hold = self.hold_n;
        } else if target < self.a {
            if self.hold > 0 {
                self.hold -= 1;
            } else {
                self.a = (self.a - self.down).max(target);
            }
        } else {
            self.hold = self.hold_n;
        }
        self.a
    }
}

// ---------------------------------------------------------------------------------------------
// Trigger and Pump: the dip

/// A dip's segments in samples: fall, stay, recovery.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DipShape {
    pub fall: f64,
    pub hold: f64,
    pub recover: f64,
}

impl DipShape {
    /// The curve of `length` samples with the user's fall and stay: if they exceed half the length,
    /// both are scaled down so the recovery keeps at least half the dip (§2.4).
    pub fn new(attack: f64, hold: f64, length: f64) -> Self {
        let (mut a, mut h) = (attack.max(1.0), hold.max(0.0));
        let length = length.max(2.0);
        if a + h > 0.5 * length {
            let k = 0.5 * length / (a + h);
            a *= k;
            h *= k;
        }
        Self { fall: a.max(1.0), hold: h, recover: (length - a - h).max(1.0) }
    }

    pub fn length(&self) -> f64 {
        self.fall + self.hold + self.recover
    }
}

/// The duck (dB, ≥ 0) at `t` samples into a dip that started from the activation `a0` (0 = from
/// 0 dB): the fall is the raised cosine in dB (a = a0 + t / fall, the remaining part of a full
/// fall), the stay is at `depth`, the recovery is drawn in linear gain from 10^(−depth / 20) to 1
/// along [`recovery`]. `None` once the dip is over.
#[inline]
pub fn dip_db(t: f64, a0: f64, shape: &DipShape, depth: f64, tension: f64) -> Option<f64> {
    let fall = (1.0 - a0.clamp(0.0, 1.0)) * shape.fall;
    if t < fall {
        return Some(depth * s_curve(a0 + t / shape.fall));
    }
    let t = t - fall;
    if t < shape.hold {
        return Some(depth);
    }
    let t = t - shape.hold;
    if t < shape.recover {
        let g_min = db_to_lin(-depth);
        let g = g_min + (1.0 - g_min) * recovery(t / shape.recover, tension);
        return Some((-lin_to_db(g)).clamp(0.0, depth));
    }
    None
}

/// Trigger's dip generator: each onset restarts the curve from the current gain.
#[derive(Clone, Copy, Debug, Default)]
pub struct Dip {
    t: f64,
    a0: f64,
    shape: DipShape,
    active: bool,
    last_db: f64,
}

impl Dip {
    pub fn reset(&mut self) {
        self.active = false;
        self.t = 0.0;
        self.last_db = 0.0;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Starts a dip of `shape` from the current duck (`depth`: the depth in force now).
    pub fn fire(&mut self, shape: DipShape, depth: f64) {
        let s = if depth > 0.0 { (self.last_db / depth).clamp(0.0, 1.0) } else { 0.0 };
        // from inside the recovery the gain is above the raised cosine's: the fall resumes from the
        // activation that gives the current duck
        self.a0 = s_inverse(s);
        self.t = 0.0;
        self.shape = shape;
        self.active = true;
    }

    /// One sample: the duck in dB (0 when idle).
    #[inline]
    pub fn next(&mut self, depth: f64, tension: f64) -> f64 {
        if !self.active {
            self.last_db = 0.0;
            return 0.0;
        }
        match dip_db(self.t, self.a0, &self.shape, depth, tension) {
            Some(d) => {
                self.t += 1.0;
                self.last_db = d;
                d
            }
            None => {
                self.active = false;
                self.last_db = 0.0;
                0.0
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Spectral

/// Bands of the Spectral shape (internal, not a parameter).
pub const BANDS: usize = 16;
/// The lowest centre (Hz); the others are half an octave apart, up to 10.2 kHz.
pub const F_LOW: f64 = 56.6;
/// Q of the bells and of each analysis band-pass.
pub const Q: f64 = 2.0;
/// The tables' step and range (dB of cut).
pub const TABLE_STEP_DB: f64 = 0.5;
pub const TABLE_MAX_DB: f64 = 60.0;
const TABLE_N: usize = (TABLE_MAX_DB / TABLE_STEP_DB) as usize + 1;
/// The cut at which the interaction matrix is taken (dB).
pub const MATRIX_DB: f64 = 12.0;

/// The centre of band `k` (Hz): 56.6 · 2^(k / 2).
pub fn centre(k: usize) -> f64 {
    F_LOW * 2f64.powf(k as f64 / 2.0)
}

/// The key's spectrum: per band, two cascaded SVF band-passes (4th order) and a 2 / 60 ms peak
/// follower. Bands whose centre is above 0.45 fs are left silent.
#[derive(Clone, Debug)]
pub struct SpectralAnalysis {
    bp: [[Svf; 2]; BANDS],
    env: [PeakFollower; BANDS],
    n: usize,
}

impl Default for SpectralAnalysis {
    fn default() -> Self {
        Self { bp: Default::default(), env: core::array::from_fn(|_| PeakFollower::new(2.0, 60.0)), n: BANDS }
    }
}

impl SpectralAnalysis {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.n = (0..BANDS).take_while(|&k| centre(k) < 0.45 * sample_rate as f64).count();
        for k in 0..BANDS {
            for f in self.bp[k].iter_mut() {
                f.set(SvfMode::BandPass, centre(k) as f32, Q as f32, 0.0, sample_rate);
            }
            self.env[k].prepare(sample_rate);
        }
        self.reset();
    }

    pub fn reset(&mut self) {
        for k in 0..BANDS {
            self.bp[k][0].reset();
            self.bp[k][1].reset();
            self.env[k].reset();
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) {
        for k in 0..self.n {
            let h = self.bp[k][0].process(x);
            let y = self.bp[k][1].process(h);
            self.env[k].next(y);
        }
    }

    /// The bands' levels in dB (−120 floor).
    pub fn levels_db(&self, out: &mut [f64; BANDS]) {
        for k in 0..BANDS {
            out[k] = if k < self.n { lin_to_db(self.env[k].value() as f64).max(-120.0) } else { -120.0 };
        }
    }
}

/// The weights w_k = clamp(1 − (K_max − K_k) / `spread`, 0, 1): the key's loudest band takes the full
/// duck, bands `spread` dB below it none.
pub fn weights(levels_db: &[f64; BANDS], spread: f64, out: &mut [f64; BANDS]) {
    let k_max = levels_db.iter().copied().fold(f64::MIN, f64::max);
    let spread = spread.max(1e-3);
    for k in 0..BANDS {
        out[k] = (1.0 - (k_max - levels_db[k]) / spread).clamp(0.0, 1.0);
    }
}

/// The fixed part of the Spectral gain stage at one sample rate: the interaction matrix and the
/// coefficient tables.
#[derive(Clone, Debug, Default)]
pub struct SpectralPlan {
    /// b[j][k]: the response of bell k at centre f_j, per dB of its own cut (taken at 12 dB)
    b: [[f64; BANDS]; BANDS],
    /// table[k * TABLE_N + i]: bell k at a cut of i · 0.5 dB
    table: Vec<SvfCoefs>,
    sr: f64,
}

impl SpectralPlan {
    /// Fills the matrix and the tables (1,936 matched designs, a few ms: never on the audio path).
    pub fn prepare(&mut self, sample_rate: f32) {
        let sr = sample_rate as f64;
        if self.sr == sr && !self.table.is_empty() {
            return;
        }
        self.sr = sr;
        for k in 0..BANDS {
            let bq = Self::design(k, MATRIX_DB, sr);
            for j in 0..BANDS {
                let r_db = 10.0 * bq.mag2(centre(j).min(0.49 * sr), sr).max(1e-30).log10();
                self.b[j][k] = (-r_db / MATRIX_DB).max(0.0);
            }
        }
        let mut table = Vec::with_capacity(BANDS * TABLE_N);
        for k in 0..BANDS {
            for i in 0..TABLE_N {
                let bq = Self::design(k, i as f64 * TABLE_STEP_DB, sr);
                table.push(SvfCoefs::from_biquad(&bq).unwrap_or_default());
            }
        }
        self.table = table;
    }

    /// Bell k's matched design for a cut of `cut_db` (≥ 0).
    pub fn design(k: usize, cut_db: f64, sample_rate: f64) -> crate::matched::Biquad {
        design(Analog::bell(Q, -cut_db), centre(k).min(0.45 * sample_rate), sample_rate)
    }

    /// The interaction b[j][k].
    pub fn interaction(&self, j: usize, k: usize) -> f64 {
        self.b[j][k]
    }

    /// Cut-only projected Gauss-Seidel: for each band in turn, g_k = max(0, t_k − Σ_{j≠k} B_kj g_j),
    /// `sweeps` times, warm-started from `g`. No bell boosts; g_k ≤ t_k.
    pub fn solve(&self, targets: &[f64; BANDS], g: &mut [f64; BANDS], sweeps: usize) {
        for _ in 0..sweeps {
            for k in 0..BANDS {
                if targets[k] <= 0.0 {
                    g[k] = 0.0;
                    continue;
                }
                let row = &self.b[k];
                let mut s = 0.0;
                for j in 0..BANDS {
                    if j != k {
                        s += row[j] * g[j];
                    }
                }
                g[k] = (targets[k] - s).clamp(0.0, targets[k]);
            }
        }
    }

    /// Bell k's coefficients for a cut of `cut_db`, interpolated in the table.
    #[inline]
    pub fn coefs(&self, k: usize, cut_db: f64) -> SvfCoefs {
        let x = (cut_db / TABLE_STEP_DB).clamp(0.0, (TABLE_N - 1) as f64);
        let i = (x as usize).min(TABLE_N - 2);
        let t = x - i as f64;
        let base = k * TABLE_N;
        self.table[base + i].lerp(&self.table[base + i + 1], t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn raised_cosine_and_inverse() {
        assert_eq!(s_curve(0.0), 0.0);
        assert_eq!(s_curve(1.0), 1.0);
        assert!((s_curve(0.5) - 0.5).abs() < 1e-12);
        for i in 0..=100 {
            let a = i as f64 / 100.0;
            assert!((s_inverse(s_curve(a)) - a).abs() < 1e-6, "{a}");
        }
    }

    #[test]
    fn activation_times() {
        // full depth after `attack` ± 1 sample; no fall during `hold`; back to 0 after `release` ± 1
        let mut act = Activation::default();
        act.set_times(0.010, 0.050, 0.200, SR);
        let att = (0.010 * SR) as usize;
        let mut n = 0;
        while act.next(1.0) < 1.0 {
            n += 1;
        }
        assert!((n as i64 + 1 - att as i64).abs() <= 1, "attack {n} vs {att}");
        let hold = (0.050 * SR) as usize;
        for _ in 0..hold {
            assert_eq!(act.next(0.0), 1.0);
        }
        let rel = (0.200 * SR) as usize;
        let mut m = 0;
        while act.next(0.0) > 0.0 {
            m += 1;
        }
        assert!((m as i64 + 1 - rel as i64).abs() <= 1, "release {m} vs {rel}");
        assert_eq!(act.value(), 0.0);
        // a new rise during the release resumes from the current value: no jump
        act.set_times(0.010, 0.0, 0.100, SR);
        for _ in 0..1000 {
            act.next(1.0);
        }
        for _ in 0..1000 {
            act.next(0.0);
        }
        let before = act.value();
        let after = act.next(1.0);
        assert!(after > before && after - before <= 1.0 / (0.010 * SR) + 1e-12);
    }

    #[test]
    fn knee_and_hysteresis() {
        let mut g = KeyGate::default();
        // closed: the curve starts at T
        assert_eq!(g.target(-40.0, -40.0, 6.0, -3.0), 0.0);
        assert!((g.target(-37.0, -40.0, 6.0, -3.0) - 0.5).abs() < 1e-12);
        // open: the curve sits 3 dB lower
        assert!((g.target(-37.0, -40.0, 6.0, -3.0) - 1.0).abs() < 1e-12);
        assert!((g.target(-41.5, -40.0, 6.0, -3.0) - 0.25).abs() < 1e-12);
        // closes below T + hyst
        assert_eq!(g.target(-43.5, -40.0, 6.0, -3.0), 0.0);
        assert!(!g.is_open());
        assert_eq!(g.target(-41.0, -40.0, 6.0, -3.0), 0.0);
        // knee 0: a gate's open / close pair
        let mut g = KeyGate::default();
        assert_eq!(g.target(-40.5, -40.0, 0.0, -3.0), 0.0);
        assert_eq!(g.target(-39.9, -40.0, 0.0, -3.0), 1.0);
        assert_eq!(g.target(-42.9, -40.0, 0.0, -3.0), 1.0);
        assert_eq!(g.target(-43.1, -40.0, 0.0, -3.0), 0.0);
    }

    #[test]
    fn dip_curve_and_scaling() {
        let s = DipShape::new(480.0, 7200.0, 18000.0);
        assert_eq!(s.length(), 18000.0);
        // attack + hold over half the length: scaled to half
        let s2 = DipShape::new(4800.0, 9600.0, 18000.0);
        assert!((s2.fall + s2.hold - 9000.0).abs() < 1e-9 && (s2.recover - 9000.0).abs() < 1e-9);
        // fall, stay, recovery, end
        assert_eq!(dip_db(0.0, 0.0, &s, 12.0, 0.0), Some(0.0));
        assert!((dip_db(240.0, 0.0, &s, 12.0, 0.0).unwrap() - 6.0).abs() < 1e-9);
        assert_eq!(dip_db(1000.0, 0.0, &s, 12.0, 0.0), Some(12.0));
        let mid = dip_db(480.0 + 7200.0 + 5160.0, 0.0, &s, 12.0, 0.0).unwrap();
        // linear in amplitude: half way between 0.251 and 1
        let g = 10f64.powf(-mid / 20.0);
        assert!((g - (0.2512 + 1.0) / 2.0).abs() < 1e-3, "{g}");
        assert_eq!(dip_db(18000.0, 0.0, &s, 12.0, 0.0), None);
        // tension: + stays low, − snaps back
        let at = |t: f64| dip_db(480.0 + 7200.0 + 0.25 * 10320.0, 0.0, &s, 12.0, t).unwrap();
        assert!(at(100.0) > at(0.0) && at(0.0) > at(-100.0));
    }

    #[test]
    fn dip_retrigger_continues_from_the_current_gain() {
        let shape = DipShape::new(480.0, 0.0, 9600.0);
        let mut d = Dip::default();
        d.fire(shape, 18.0);
        let mut last = 0.0;
        for _ in 0..6000 {
            last = d.next(18.0, 0.0);
        }
        assert!(last > 0.0 && last < 18.0);
        d.fire(shape, 18.0);
        let first = d.next(18.0, 0.0);
        assert!((first - last).abs() < 0.2, "{first} vs {last}");
        let mut max_step = 0.0f64;
        let mut prev = first;
        while d.is_active() {
            let v = d.next(18.0, 0.0);
            max_step = max_step.max((v - prev).abs());
            prev = v;
        }
        assert!(max_step < 0.1, "{max_step}");
    }

    #[test]
    fn spectral_solver_meets_its_depth() {
        let mut plan = SpectralPlan::default();
        plan.prepare(SR as f32);
        // the sum of the bells' responses (dB of cut) at a frequency
        let cut_at = |g: &[f64; BANDS], f: f64| -> f64 {
            (0..BANDS)
                .filter(|&k| g[k] > 0.0)
                .map(|k| -10.0 * SpectralPlan::design(k, g[k], SR).mag2(f, SR).log10())
                .sum()
        };
        for depth in [6.0, 12.0, 18.0] {
            let mut targets: Vec<[f64; BANDS]> = vec![[depth; BANDS]];
            let mut one = [0.0; BANDS];
            one[7] = depth;
            targets.push(one);
            targets.push(core::array::from_fn(|k| depth * (1.0 - (k as f64 - 8.0).abs() / 5.0).clamp(0.0, 1.0)));
            let mut blocks = [0.0; BANDS];
            for k in 4..8 {
                blocks[k] = depth;
            }
            blocks[11] = depth * 0.5;
            blocks[12] = depth * 0.5;
            targets.push(blocks);
            for (ti, t) in targets.iter().enumerate() {
                let mut g = [0.0; BANDS];
                plan.solve(t, &mut g, 4);
                let freqs: Vec<f64> = (0..=600).map(|i| centre(0) * 2f64.powf(i as f64 * 7.5 / 600.0)).collect();
                let deepest = freqs.iter().map(|&f| cut_at(&g, f)).fold(0.0, f64::max);
                eprintln!("target {ti} depth {depth}: deepest {deepest:.2}");
                assert!((deepest - depth).abs() <= 0.5, "target {ti} depth {depth}: deepest {deepest:.2}");
                for k in 0..BANDS {
                    if g[k] > 0.0 {
                        let c = cut_at(&g, centre(k));
                        assert!((c - t[k]).abs() <= 1.0, "target {ti} depth {depth} band {k}: {c:.2} vs {:.2}", t[k]);
                    }
                }
                if ti == 0 {
                    let inner: Vec<f64> = freqs.iter().filter(|&&f| f >= centre(1) && f <= centre(14)).map(|&f| cut_at(&g, f)).collect();
                    let ripple = inner.iter().copied().fold(f64::MIN, f64::max) - inner.iter().copied().fold(f64::MAX, f64::min);
                    let bound = match depth as i32 {
                        6 => 1.0,
                        12 => 1.5,
                        _ => 2.0,
                    };
                    eprintln!("uniform {depth} dB: deepest {deepest:.2}, ripple {ripple:.2} dB");
                    assert!(ripple <= bound, "uniform {depth}: ripple {ripple:.2}");
                }
            }
        }
    }

    #[test]
    fn tables_match_designs() {
        let mut plan = SpectralPlan::default();
        plan.prepare(SR as f32);
        // an interpolated entry against the live design, through the SVF
        for (k, cut) in [(3usize, 7.25f64), (9, 18.8), (15, 41.1)] {
            let mut a = Svf::new();
            a.set_coefs(&plan.coefs(k, cut));
            let mut b = Svf::new();
            b.set_biquad(&SpectralPlan::design(k, cut, SR));
            for f in [centre(k) * 0.7, centre(k), centre(k) * 1.4] {
                let ga = crate::svf::measure_gain_db(|x| a.process(x), f as f32, SR as f32);
                let gb = crate::svf::measure_gain_db(|x| b.process(x), f as f32, SR as f32);
                assert!((ga - gb).abs() < 0.05, "band {k} cut {cut} at {f:.0}: {ga:.3} vs {gb:.3}");
            }
        }
    }

    #[test]
    fn analysis_resolves_a_tone() {
        let mut an = SpectralAnalysis::default();
        an.prepare(SR as f32);
        // a sine on a centre: the next band −9.5 dB, the one after −20 dB (4th order, Q 2)
        let f = centre(8);
        for i in 0..(SR as usize / 2) {
            an.process((0.5 * (2.0 * PI * f * i as f64 / SR).sin()) as f32);
        }
        let mut l = [0.0; BANDS];
        an.levels_db(&mut l);
        let (n1, n2) = (l[9] - l[8], l[10] - l[8]);
        assert!((n1 + 9.5).abs() < 1.5 && (n2 + 20.0).abs() < 2.5, "{n1:.1} {n2:.1}");
        let mut w = [0.0; BANDS];
        weights(&l, 12.0, &mut w);
        assert_eq!(w[8], 1.0);
        assert!(w[10] == 0.0 && w[6] == 0.0);
    }
}
