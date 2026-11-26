// SPDX-License-Identifier: GPL-3.0-or-later
//! The Spectral core of gfx-polysynth (its README §2.2), moved here so that gfx-organ plays on the same
//! analysis (gfx-organ README §2.8), at the bank rate (22.05 – 24 kHz at every host rate).
//!
//! - **Banks** (`filterbank`, explicit band lists): **clean** = order 4, ⅓ ERB apart and ⅓ ERB wide
//!   (full −3 dB pole width, never under 20 Hz); **fast** = order 2, ½ ERB apart and wide (never under
//!   40 Hz). Both grids hang from 2 kHz downwards to 24 Hz; a band under `lowest` × 0.8 is faded out
//!   instead of redesigning the bank.
//! - **One band per partial**: the band levels L_b (attack 3 ms, release 50 ms) select local maxima,
//!   s_b = c(L_b − L_(b−1)) · c(L_b − L_(b+1)), c(d) = clamp(d / 6 dB + ½, 0, 1); each band's phasor is
//!   turned by its design rotation ρ_b, and its level divided by its skirt gain |H_b(f̂_b)|.
//! - **Per-band level**: P_b the peak of 2|z_b| since the last pick (attack 1 ms, held; a pick lets it
//!   go to the band's level), g_b = (P_b / 2|z_b|)^c capped at +30 dB; a band under `thresh` − 10 dB
//!   fades out over 30 ms (6 dB of hysteresis).
//! - **Handover fast → clean** (`bank` Dual) per clean band: an event (a pick; new energy; pitch
//!   motion) puts the region on the fast bank in 5 ms; the clean band comes back on a raised cosine
//!   from 3.5 τ_b to 7.5 τ_b after the last event, its phasor snapped onto the fast band's.
//! - **Ownership mask** (gfx-polysynth's Mono): the bands near the voice's partials 1 … 16 muted.
//! - **The shaper is pluggable** ([`Shaper`], static dispatch): per sounding band and sample the core
//!   hands it the turned unit phasor u_b, |z_b|, the band's gain and mask, f̂_b and the purity. gfx-polysynth's
//!   waveform series and gfx-organ's organ law are its two implementations.
//! - **Root sieve** (gfx-organ §2.3, [`Sieve`]): compiled in only when the shaper asks for it
//!   ([`Shaper::SIEVE`]); it reads the clean bank and tells which bands are a note's fundamental.
//!
//! The control work runs every [`TICK`] bank samples; the gains are interpolated across the tick.
//! f64 state, no allocation after `prepare`. The move from gfx-polysynth is guarded by a byte-identical
//! null of gfx-polysynth's renders (its `tests.rs`).

use crate::filterbank::{self, BandSpec, ComplexBank};
use std::f64::consts::{PI, TAU};

pub use crate::spectral_sieve::{Sieve, SieveParams, SieveStats};

/// The banks' top centre (Hz): nothing of the guitar above passes.
pub const TOP_HZ: f64 = 2000.0;
/// The grids' lowest centre (Hz): `lowest` 30 Hz × 0.8.
pub const GRID_LOW_HZ: f64 = 24.0;
/// Control work every this many bank samples.
pub const TICK: usize = 16;
/// fcap as a fraction of the bank rate.
pub const FCAP: f64 = 0.42;
/// A harmonic fades over its last this many Hz under fcap.
pub const FADE_HZ: f64 = 500.0;
/// The skirt gain's frequency clamp, in half spacings: the review prototype's ±0.6 of the full
/// spacing (its `lib.py`; the README's "±0.6 of the half-spacing" would leave a 7 dB dip at every
/// crossover of the clean bank).
const SKIRT_CLAMP: f64 = 1.2;
/// The compress gain's cap (+30 dB).
const COMP_CAP: f64 = 31.622_776_601_683_79;
/// The local handover events read the fast band only when its purity is over this: in a chord a
/// fast band 40 Hz wide holds a neighbour partial whose beat moves its frequency ±10 – 20 ¢ (0.5 let
/// the open-E's bands chatter onto the fast bank for good).
const PURE_EVENT: f64 = 0.9;
/// A band in pitch motion stays on the fast bank until the banks agree for this long (s).
const MOTION_HOLD_S: f64 = 0.040;
/// New energy: the fast band over the clean one by this (dB, README 3), the clean band rising.
const NEW_ENERGY_DB: f64 = 6.0;
/// The purity gate: w = exp(−σ²_IF / (PURITY_B · B)²).
const PURITY_B: f64 = 0.25;

// ---------------------------------------------------------------- fast, deterministic control math

/// log₂ x (x > 0; −1000 otherwise): exponent bits and an odd series in t = (m − 1) / (m + 1), |error| <
/// 2e-6. For the control tick, where the libm calls were a third of the core's cost.
#[inline]
pub fn fast_log2(x: f64) -> f64 {
    if x.is_nan() || x <= 0.0 || !x.is_finite() {
        return -1000.0;
    }
    let bits = x.to_bits();
    let e = ((bits >> 52) & 0x7ff) as i64 - 1023;
    let m = f64::from_bits((bits & 0x000f_ffff_ffff_ffff) | 0x3ff0_0000_0000_0000);
    let t = (m - 1.0) / (m + 1.0);
    let t2 = t * t;
    let p = t * (2.0 + t2 * (2.0 / 3.0 + t2 * (0.4 + t2 * (2.0 / 7.0 + t2 * (2.0 / 9.0 + t2 * (2.0 / 11.0))))));
    e as f64 + p * std::f64::consts::LOG2_E
}

/// 2^y: the integer part in the exponent bits, the fraction by a degree-8 series, relative error < 1e-7.
#[inline]
pub fn fast_exp2(y: f64) -> f64 {
    let y = y.clamp(-1000.0, 1000.0);
    let i = y.floor();
    let f = (y - i) * std::f64::consts::LN_2;
    let p = 1.0 + f * (1.0 + f * (0.5 + f * (1.0 / 6.0 + f * (1.0 / 24.0 + f * (1.0 / 120.0 + f * (1.0 / 720.0 + f * (1.0 / 5040.0 + f / 40320.0)))))));
    p * f64::from_bits(((i as i64 + 1023) as u64) << 52)
}

/// atan2(y, x) by a degree-9 odd polynomial on the reduced ratio, |error| < 1.2e-5 rad.
#[inline]
pub fn fast_atan2(y: f64, x: f64) -> f64 {
    let (ax, ay) = (x.abs(), y.abs());
    if ax == 0.0 && ay == 0.0 {
        return 0.0;
    }
    let (a, swap) = if ay > ax { (ax / ay, true) } else { (ay / ax, false) };
    let s = a * a;
    let mut r = a * (0.999_866_0 + s * (-0.330_299_5 + s * (0.180_141_0 + s * (-0.085_133_0 + s * 0.020_835_1))));
    if swap {
        r = std::f64::consts::FRAC_PI_2 - r;
    }
    if x < 0.0 {
        r = PI - r;
    }
    if y < 0.0 {
        -r
    } else {
        r
    }
}

#[inline]
pub fn erb_number(f: f64) -> f64 {
    21.4 * (1.0 + 0.00437 * f).log10()
}

#[inline]
fn erb_freq(e: f64) -> f64 {
    (10f64.powf(e / 21.4) - 1.0) / 0.00437
}

/// A grid hanging from [`TOP_HZ`] down by `step` ERB to [`GRID_LOW_HZ`], ascending; widths
/// max(`width_erb` · ERB(f), `min_width`).
pub fn grid(step: f64, width_erb: f64, min_width: f64, order: usize) -> Vec<BandSpec> {
    let top = erb_number(TOP_HZ);
    let mut v = Vec::new();
    let mut k = 0;
    loop {
        let f = erb_freq(top - step * k as f64);
        if f < GRID_LOW_HZ - 1e-9 {
            break;
        }
        v.push(BandSpec { centre: f, width: (width_erb * filterbank::erb(f)).max(min_width), order });
        k += 1;
    }
    v.reverse();
    v
}

/// The clean bank's grid (order 4, ⅓ ERB, ≥ 20 Hz).
pub fn clean_grid() -> Vec<BandSpec> {
    grid(1.0 / 3.0, 1.0 / 3.0, 20.0, 4)
}

/// The fast bank's grid (order 2, ½ ERB, ≥ 40 Hz).
pub fn fast_grid() -> Vec<BandSpec> {
    grid(0.5, 0.5, 40.0, 2)
}

/// `bank`: which bank sounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BankMode {
    Dual,
    Fast,
    Clean,
}

/// c(d) of the selection: 1 at a local maximum, 0 on a neighbour's skirt.
#[inline]
fn sel_c(d: f64) -> f64 {
    (d * (1.0 / 6.0) + 0.5).clamp(0.0, 1.0)
}

/// The branch of √u (u a unit phasor) nearest the last one.
#[inline]
pub fn sqrt_branch(ur: f64, ui: f64, sr: &mut f64, si: &mut f64) -> (f64, f64) {
    let mut a = ((1.0 + ur) * 0.5).max(0.0).sqrt();
    let mut b = ((1.0 - ur) * 0.5).max(0.0).sqrt();
    if ui < 0.0 {
        b = -b;
    }
    if a * *sr + b * *si < 0.0 {
        a = -a;
        b = -b;
    }
    *sr = a;
    *si = b;
    (a, b)
}

/// What the owner sets on the core's analysis at each tick (its shaper has its own settings).
#[derive(Clone, Copy, Debug)]
pub struct CoreSettings {
    pub bank: BankMode,
    /// `compress` (polysynth) / `hold` (organ) 0 – 1: g_b = (P_b / 2|z_b|)^c.
    pub compress: f64,
    /// `thresh` (dBFS), `lowest` (Hz).
    pub thresh_db: f64,
    pub lowest: f64,
    /// The ownership fade (polysynth: 0 Spectral, 1 Mono) and `residue` (0 – 1).
    pub mono: f64,
    pub residue: f64,
}

impl Default for CoreSettings {
    fn default() -> Self {
        Self { bank: BankMode::Dual, compress: 0.4, thresh_db: -66.0, lowest: 70.0, mono: 0.0, residue: 1.0 }
    }
}

/// The per-band shaper of the core: what each sounding band plays. Static dispatch; every method is
/// called from the core's loops.
pub trait Shaper {
    /// The root sieve runs (gfx-organ); compiled out otherwise.
    const SIEVE: bool = false;
    /// The band lists of both banks at the bank rate `fb` (allocates).
    fn prepare(&mut self, clean: &[BandSpec], fast: &[BandSpec], fb: f64);
    /// Back to the initial state (fixed seeds).
    fn reset(&mut self);
    /// Once per control tick, after the analysis, the sieve and the gains.
    fn tick(&mut self, a: &Analysis);
    /// One sample of band `b` of bank `bank` (0 clean, 1 fast): `ur`, `ui` its turned unit phasor,
    /// `m` = |z_b|, `kk` its gain (selection, floor, enable, handover, hold, skirt, × 2), `mk` its mask,
    /// `f` = f̂_b, `pur` its purity. Adds into `out` (main, copy).
    #[allow(clippy::too_many_arguments)]
    fn band(&mut self, bank: usize, b: usize, ur: f64, ui: f64, m: f64, kk: f64, mk: f64, f: f64, pur: f64, out: &mut (f64, f64));
}

/// One bank with its per-band analysis.
#[derive(Clone, Debug, Default)]
pub struct Bank {
    pub bank: ComplexBank,
    pub n: usize,
    pub spec: Vec<BandSpec>,
    // constants
    pub hs: Vec<f64>,
    rot_r: Vec<f64>,
    rot_i: Vec<f64>,
    dem_r: Vec<f64>,
    dem_i: Vec<f64>,
    pub tau: Vec<f64>,
    inv_bw2: Vec<f64>,
    pole: Vec<f64>,
    // tick state
    zp_r: Vec<f64>,
    zp_i: Vec<f64>,
    pub lvl2: Vec<f64>,
    pub ldb: Vec<f64>,
    fm1: Vec<f64>,
    fm2: Vec<f64>,
    pub fhat: Vec<f64>,
    pub fslow: Vec<f64>,
    pub pur: Vec<f64>,
    pub skirt: Vec<f64>,
    pub skirt_db: Vec<f64>,
    pub peak: Vec<f64>,
    pub gcomp: Vec<f64>,
    pub gate: Vec<f64>,
    pub gate_open: Vec<bool>,
    pub sel: Vec<f64>,
    pub en: Vec<f64>,
    /// handover weight (clean: x; fast: 1 − x at its centre)
    pub wb: Vec<f64>,
    own: Vec<f64>,
    pub own_t: Vec<bool>,
    // per-sample gains: pre-mask gain k and mask mk, each with its step over the tick
    k: Vec<f64>,
    pub kt: Vec<f64>,
    ks: Vec<f64>,
    mk: Vec<f64>,
    pub mkt: Vec<f64>,
    mks: Vec<f64>,
    /// the handover snap (clean bank): the band's phasor turned by q onto the fast bank's at the start
    /// of its fade-in, and held (gfx-polysynth README §2.2 relaxes it to 0 at ≤ 5 ¢: that cost 1 –
    /// 12 dB of chord cleanliness, the bands of one partial no longer turned alike)
    q_ang: Vec<f64>,
    qr: Vec<f64>,
    qi: Vec<f64>,
}

impl Bank {
    fn prepare(&mut self, grid: &[BandSpec], fb: f64) {
        let n = grid.len();
        self.n = n;
        self.bank.prepare(n);
        self.bank.design(grid, fb);
        self.spec = grid.to_vec();
        let z = || vec![0.0; n];
        self.hs = z();
        self.rot_r = z();
        self.rot_i = z();
        self.dem_r = z();
        self.dem_i = z();
        self.tau = z();
        self.inv_bw2 = z();
        self.pole = z();
        for v in [
            &mut self.zp_r, &mut self.zp_i, &mut self.lvl2, &mut self.ldb, &mut self.fm1, &mut self.fm2, &mut self.fhat, &mut self.fslow, &mut self.pur, &mut self.skirt, &mut self.skirt_db, &mut self.peak, &mut self.gcomp, &mut self.gate, &mut self.sel, &mut self.en, &mut self.wb, &mut self.own, &mut self.k, &mut self.kt, &mut self.ks, &mut self.mk, &mut self.mkt, &mut self.mks, &mut self.q_ang, &mut self.qr, &mut self.qi,
        ] {
            *v = vec![0.0; n];
        }
        self.gate_open = vec![false; n];
        self.own_t = vec![false; n];
        let mut rho = 0.0;
        for b in 0..n {
            let s = grid[b];
            let below = if b > 0 { s.centre - grid[b - 1].centre } else { grid[1.min(n - 1)].centre - s.centre };
            let above = if b + 1 < n { grid[b + 1].centre - s.centre } else { below };
            self.hs[b] = 0.25 * (below + above);
            if b > 0 {
                rho += filterbank::design_rotation(&grid[b - 1], &s, fb);
            }
            self.rot_r[b] = rho.cos();
            self.rot_i[b] = rho.sin();
            let w = -TAU * s.centre * TICK as f64 / fb;
            self.dem_r[b] = w.cos();
            self.dem_i[b] = w.sin();
            self.tau[b] = 1.0 / (PI * s.width);
            self.inv_bw2[b] = 1.0 / (PURITY_B * s.width).powi(2);
            self.pole[b] = filterbank::pole_radius(s.width, fb);
        }
        self.reset();
    }

    fn reset(&mut self) {
        self.bank.reset();
        for v in [
            &mut self.zp_r, &mut self.zp_i, &mut self.lvl2, &mut self.fm1, &mut self.fm2, &mut self.pur, &mut self.peak, &mut self.gate, &mut self.sel, &mut self.wb, &mut self.own, &mut self.k, &mut self.kt, &mut self.ks, &mut self.mk, &mut self.mkt, &mut self.mks, &mut self.q_ang, &mut self.qi,
        ] {
            v.iter_mut().for_each(|x| *x = 0.0);
        }
        self.qr.iter_mut().for_each(|x| *x = 1.0);
        for b in 0..self.n {
            self.ldb[b] = -300.0;
            self.fhat[b] = self.spec[b].centre;
            self.fslow[b] = self.spec[b].centre;
            self.skirt[b] = 1.0;
            self.gcomp[b] = 1.0;
            self.en[b] = 0.0;
        }
        self.gate_open.iter_mut().for_each(|x| *x = false);
        self.own_t.iter_mut().for_each(|x| *x = false);
    }

    /// Band b's phasor now, turned by its design rotation (and its snap), or None under 1e-12.
    pub fn phasor(&self, b: usize) -> Option<(f64, f64)> {
        let (zr, zi) = (self.bank.re()[b], self.bank.im()[b]);
        let m = (zr * zr + zi * zi).sqrt();
        if m < 1e-12 {
            return None;
        }
        let (rr, ri) = (self.rot_r[b] * self.qr[b] - self.rot_i[b] * self.qi[b], self.rot_r[b] * self.qi[b] + self.rot_i[b] * self.qr[b]);
        Some(((zr * rr - zi * ri) / m, (zr * ri + zi * rr) / m))
    }

    /// Band b's raw complex output (no rotation).
    #[inline]
    pub fn z(&self, b: usize) -> (f64, f64) {
        (self.bank.re()[b], self.bank.im()[b])
    }

    /// The per-band analysis of a tick: levels, frequency, purity, skirt, hold, floor, enable.
    fn analyse(&mut self, k: &TickConsts, s: &CoreSettings, pick: bool) {
        let re = self.bank.re();
        let im = self.bank.im();
        let close = 10f64.powf((s.thresh_db - 10.0) / 20.0);
        let open = close * 2.0;
        let lo = 0.8 * s.lowest;
        let c = s.compress.clamp(0.0, 1.0);
        for b in 0..self.n {
            let (zr, zi) = (re[b], im[b]);
            let m2 = zr * zr + zi * zi;
            let a = if m2 > self.lvl2[b] { k.a_att } else { k.a_rel };
            self.lvl2[b] += (m2 - self.lvl2[b]) * a;
            self.ldb[b] = 10.0 * std::f64::consts::LOG10_2 * fast_log2(self.lvl2[b] + 1e-30);
            // frequency: the rotation over the tick, demodulated by the centre
            let pr = zr * self.zp_r[b] + zi * self.zp_i[b];
            let pi = zi * self.zp_r[b] - zr * self.zp_i[b];
            let qr = pr * self.dem_r[b] - pi * self.dem_i[b];
            let qi = pr * self.dem_i[b] + pi * self.dem_r[b];
            if qr * qr + qi * qi > 1e-40 {
                let df = fast_atan2(qi, qr) * k.hz_per_rad;
                self.fm1[b] += (df - self.fm1[b]) * k.a_f;
                self.fm2[b] += (df * df - self.fm2[b]) * k.a_f;
            }
            self.zp_r[b] = zr;
            self.zp_i[b] = zi;
            let var = (self.fm2[b] - self.fm1[b] * self.fm1[b]).max(0.0);
            self.pur[b] = fast_exp2(-var * self.inv_bw2[b] * std::f64::consts::LOG2_E);
            let hs = self.hs[b];
            let fc = self.spec[b].centre;
            self.fhat[b] = fc + self.fm1[b].clamp(-2.0 * hs, 2.0 * hs);
            self.fslow[b] += (self.fhat[b] - self.fslow[b]) * k.a_slow;
            // skirt: 1 / |H(offset)|, the offset clamped
            let off = self.fm1[b].clamp(-SKIRT_CLAMP * hs, SKIRT_CLAMP * hs);
            let th = TAU * off / k.fb;
            let r = self.pole[b];
            let cos = 1.0 - 0.5 * th * th * (1.0 - th * th / 12.0);
            let g2 = (1.0 - r) * (1.0 - r) / (1.0 - 2.0 * r * cos + r * r);
            // |H| = (per-pole |H|²)^(N / 2)
            let h = if self.spec[b].order >= 4 { g2 * g2 } else { g2 };
            self.skirt[b] = 1.0 / h.max(1e-6);
            self.skirt_db[b] = -20.0 * std::f64::consts::LOG10_2 * fast_log2(h.max(1e-6));
            // hold and compress
            let amp = 2.0 * m2.sqrt();
            if amp > self.peak[b] {
                self.peak[b] += (amp - self.peak[b]) * k.a_pk;
            } else {
                // held: a pick lets it go to the band's level now (below)
                self.peak[b] *= k.pk_rel;
            }
            if pick {
                self.peak[b] = amp;
            }
            self.gcomp[b] = if c > 0.0 && amp > 1e-12 && self.peak[b] > amp { fast_exp2(c * fast_log2(self.peak[b] / amp)).min(COMP_CAP) } else { 1.0 };
            // the floor, on the smoothed level
            let lv = 2.0 * self.lvl2[b].sqrt();
            if self.gate_open[b] && lv < close {
                self.gate_open[b] = false;
            } else if !self.gate_open[b] && lv > open {
                self.gate_open[b] = true;
            }
            self.gate[b] = if self.gate_open[b] { (self.gate[b] + k.g_up).min(1.0) } else { (self.gate[b] - k.g_down).max(0.0) };
            // enabled above `lowest` × 0.8
            let on = fc >= lo * (1.0 - 1e-9);
            self.en[b] = if on { (self.en[b] + k.en_step).min(1.0) } else { (self.en[b] - k.en_step).max(0.0) };
        }
        // selection: local maxima of the levels
        for b in 0..self.n {
            let l = self.ldb[b];
            let lower = if b > 0 { sel_c(l - self.ldb[b - 1]) } else { 1.0 };
            let upper = if b + 1 < self.n { sel_c(l - self.ldb[b + 1]) } else { 1.0 };
            self.sel[b] = lower * upper;
        }
    }

    /// The gains for the next tick and the mask.
    fn targets(&mut self, k: &TickConsts, s: &CoreSettings, owner: f64) {
        let mono = s.mono.clamp(0.0, 1.0);
        let residue = s.residue.clamp(0.0, 1.0);
        for b in 0..self.n {
            // ownership (Mono): within max(⅙ ERB, half a spacing) of the voice's partials 1 … 16
            let fc = self.spec[b].centre;
            let owned = owner > 0.0 && {
                let h = (fc / owner).round();
                (1.0..=16.0).contains(&h) && (fc - h * owner).abs() <= (filterbank::erb(fc) / 6.0).max(self.hs[b])
            };
            self.own_t[b] = owned;
            self.own[b] = if owned { (self.own[b] + k.own_up).min(1.0) } else { (self.own[b] - k.own_down).max(0.0) };
            let target = self.sel[b] * self.gate[b] * self.en[b] * self.wb[b] * self.gcomp[b] * self.skirt[b] * 2.0;
            let target = if target < 1e-9 { 0.0 } else { target };
            let mask = (1.0 - mono) + mono * residue * (1.0 - self.own[b]);
            // land exactly on the last target, then step to the new one
            self.k[b] = self.kt[b];
            self.kt[b] = target;
            self.ks[b] = (target - self.k[b]) * k.inv_tick;
            self.mk[b] = self.mkt[b];
            self.mkt[b] = mask;
            self.mks[b] = (mask - self.mk[b]) * k.inv_tick;
        }
    }

    /// One bank sample of every sounding band, through the shaper: (main, copy).
    #[inline(always)]
    fn render<S: Shaper>(&mut self, which: usize, sh: &mut S) -> (f64, f64) {
        let re = self.bank.re();
        let im = self.bank.im();
        let mut out = (0.0, 0.0);
        for b in 0..self.n {
            let kk = self.k[b] + self.ks[b];
            if kk == 0.0 && self.k[b] == 0.0 {
                continue;
            }
            self.k[b] = kk;
            let mk = self.mk[b] + self.mks[b];
            self.mk[b] = mk;
            let (zr, zi) = (re[b], im[b]);
            let m = (zr * zr + zi * zi).sqrt();
            if m < 1e-15 {
                continue;
            }
            let inv = 1.0 / m;
            let (u0r, u0i) = (zr * inv, zi * inv);
            let (rr, ri) = (self.rot_r[b] * self.qr[b] - self.rot_i[b] * self.qi[b], self.rot_r[b] * self.qi[b] + self.rot_i[b] * self.qr[b]);
            let ur = u0r * rr - u0i * ri;
            let ui = u0r * ri + u0i * rr;
            sh.band(which, b, ur, ui, m, kk, mk, self.fhat[b], self.pur[b], &mut out);
        }
        out
    }
}

/// Coefficients of a tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct TickConsts {
    pub fb: f64,
    a_att: f64,
    a_rel: f64,
    a_f: f64,
    hz_per_rad: f64,
    a_pk: f64,
    pk_rel: f64,
    g_up: f64,
    g_down: f64,
    en_step: f64,
    own_up: f64,
    own_down: f64,
    inv_tick: f64,
    pub tick_s: f64,
    a_rise: f64,
    a_slow: f64,
}

/// The analysis: both banks, the handover, the sieve. What a [`Shaper`] reads at each tick.
#[derive(Clone, Debug, Default)]
pub struct Analysis {
    pub fb: f64,
    pub clean: Bank,
    pub fast: Bank,
    /// clean band → nearest fast band; fast band → (clean index below, fraction)
    pub near: Vec<usize>,
    pub interp: Vec<(usize, f64)>,
    /// handover state per clean band
    pub x: Vec<f64>,
    t_ev: Vec<u64>,
    ne: Vec<u32>,
    pm: Vec<u32>,
    lc_prev: Vec<f64>,
    moving: Vec<bool>,
    quiet: Vec<u32>,
    pub ticks: u64,
    pub k: TickConsts,
    pub s: CoreSettings,
    owner: f64,
    pick: bool,
    /// a pick arrived at this tick (read by the shaper's tick)
    pub picked: bool,
    ev_ticks: u32,
    /// The root sieve (run only when the shaper asks for it).
    pub sieve: Sieve,
}

/// The Spectral core with its shaper.
#[derive(Clone, Debug, Default)]
pub struct Core<S: Shaper> {
    pub a: Analysis,
    pub shaper: S,
    phase: usize,
}

impl<S: Shaper + Default> Core<S> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<S: Shaper> Core<S> {
    /// Designs both banks for the bank rate `fb` (allocates).
    pub fn prepare(&mut self, fb: f64) {
        let a = &mut self.a;
        a.fb = fb;
        let cg = clean_grid();
        let fg = fast_grid();
        a.clean.prepare(&cg, fb);
        a.fast.prepare(&fg, fb);
        a.near = cg
            .iter()
            .map(|c| {
                let mut best = 0;
                for (j, f) in fg.iter().enumerate() {
                    if (f.centre - c.centre).abs() < (fg[best].centre - c.centre).abs() {
                        best = j;
                    }
                }
                best
            })
            .collect();
        let ce: Vec<f64> = cg.iter().map(|b| erb_number(b.centre)).collect();
        a.interp = fg
            .iter()
            .map(|f| {
                let e = erb_number(f.centre);
                let mut i = 0;
                while i + 2 < ce.len() && ce[i + 1] <= e {
                    i += 1;
                }
                let fr = ((e - ce[i]) / (ce[i + 1] - ce[i])).clamp(0.0, 1.0);
                (i, fr)
            })
            .collect();
        let n = cg.len();
        a.x = vec![1.0; n];
        a.t_ev = vec![0; n];
        a.ne = vec![0; n];
        a.pm = vec![0; n];
        a.lc_prev = vec![-200.0; n];
        a.moving = vec![false; n];
        a.quiet = vec![0; n];
        let t = TICK as f64;
        let ms = |v: f64| 1.0 - (-t / (v * 1e-3 * fb)).exp();
        a.k = TickConsts {
            fb,
            a_att: ms(3.0),
            a_rel: ms(50.0),
            a_f: ms(10.0),
            hz_per_rad: fb / (TAU * t),
            a_pk: ms(1.0),
            // the body is held between picks (polysynth README: "release 3 s", which lost 6.7 dB in
            // 2 s of a T60 3 s string, against the 1.5 dB of the Hold row)
            pk_rel: 1.0,
            g_up: t / (0.005 * fb),
            g_down: t / (0.030 * fb),
            en_step: t / (0.020 * fb),
            own_up: t / (0.010 * fb),
            own_down: t / (0.030 * fb),
            inv_tick: 1.0 / t,
            tick_s: t / fb,
            a_rise: ms(5.0),
            a_slow: ms(15.0),
        };
        a.ev_ticks = (0.005 / a.k.tick_s).ceil() as u32;
        if S::SIEVE {
            a.sieve.prepare(&a.clean, a.k.tick_s);
        }
        self.shaper.prepare(&cg, &fg, fb);
        self.reset();
    }

    pub fn reset(&mut self) {
        let a = &mut self.a;
        a.clean.reset();
        a.fast.reset();
        a.x.iter_mut().for_each(|v| *v = 1.0);
        // no event yet: as if the last one were long ago
        a.ticks = 1 << 40;
        a.t_ev.iter_mut().for_each(|v| *v = 0);
        a.ne.iter_mut().for_each(|v| *v = 0);
        a.pm.iter_mut().for_each(|v| *v = 0);
        a.lc_prev.iter_mut().for_each(|v| *v = -200.0);
        a.moving.iter_mut().for_each(|v| *v = false);
        a.quiet.iter_mut().for_each(|v| *v = 0);
        self.phase = 0;
        a.owner = 0.0;
        a.pick = false;
        a.picked = false;
        if S::SIEVE {
            a.sieve.reset();
        }
        self.shaper.reset();
        // bands enabled at once on the first tick
        let lo = 0.8 * a.s.lowest;
        for bk in [&mut a.clean, &mut a.fast] {
            for b in 0..bk.n {
                bk.en[b] = if bk.spec[b].centre >= lo * (1.0 - 1e-9) { 1.0 } else { 0.0 };
            }
        }
    }

    pub fn bank_rate(&self) -> f64 {
        self.a.fb
    }

    /// The analysis settings for the coming ticks.
    pub fn set(&mut self, s: CoreSettings) {
        self.a.s = s;
    }

    /// A pick: every band goes to the fast bank (Dual).
    pub fn pick(&mut self) {
        self.a.pick = true;
    }

    /// The voice's note (Hz, polysynth's Mono): the bands it owns are masked, 0 = none.
    pub fn set_owner(&mut self, hz: f64) {
        self.a.owner = hz;
    }

    /// One bank sample → (main, copy).
    #[inline]
    pub fn process(&mut self, x: f64) -> (f64, f64) {
        self.a.clean.bank.process(x);
        self.a.fast.bank.process(x);
        if self.phase == 0 {
            self.tick();
        }
        self.phase += 1;
        if self.phase == TICK {
            self.phase = 0;
        }
        let (a1, b1) = self.a.clean.render(0, &mut self.shaper);
        let (a2, b2) = self.a.fast.render(1, &mut self.shaper);
        (a1 + a2, b1 + b2)
    }

    fn tick(&mut self) {
        let a = &mut self.a;
        a.ticks += 1;
        let k = a.k;
        let s = a.s;
        let pick = a.pick;
        a.clean.analyse(&k, &s, pick);
        a.fast.analyse(&k, &s, pick);
        // ---- events (Dual)
        let now = a.ticks;
        a.picked = std::mem::take(&mut a.pick);
        if a.picked {
            a.t_ev.iter_mut().for_each(|t| *t = now);
        }
        // ---- the root sieve, on the clean bank (gfx-organ §2.3)
        if S::SIEVE {
            a.sieve.tick(&a.clean, a.picked, now);
        }
        let n = a.clean.n;
        if s.bank == BankMode::Dual {
            for b in 0..n {
                // rising: the level over its own 5 ms mean by 1 dB (a hammer-on's clean band climbs
                // ~0.5 dB / ms; a steady partial's skirt band moves 0.1 dB / ms under a vibrato)
                let l = a.clean.ldb[b].max(-200.0);
                a.lc_prev[b] += (l - a.lc_prev[b]) * k.a_rise;
                let rising = l > a.lc_prev[b] + 1.0;
                if !a.clean.gate_open[b] {
                    a.ne[b] = 0;
                    a.pm[b] = 0;
                    continue;
                }
                // the fast band holding this partial: the loudest around the nearest centre
                let j0 = a.near[b];
                let mut j = j0;
                for jj in j0.saturating_sub(1)..=(j0 + 1).min(a.fast.n - 1) {
                    if a.fast.ldb[jj] > a.fast.ldb[j] {
                        j = jj;
                    }
                }
                // new energy: the fast bank's level over the clean band's by 3 dB while the clean band
                // is still rising
                let lc = a.clean.ldb[b] + a.clean.skirt_db[b];
                let lf = a.fast.ldb[j] + a.fast.skirt_db[j];
                a.ne[b] = if rising && lf - lc > NEW_ENERGY_DB { a.ne[b] + 1 } else { 0 };
                // pitch motion: on a partial the clean band holds, read in a pure fast band, 15 ¢ for
                // 5 ms; then held while the banks stay 6 ¢ apart. A root of the sieve is never handed
                // to the fast bank by pitch motion (gfx-organ §2.6: "organs do not bend")
                let root = S::SIEVE && a.sieve.holds_root(b);
                let cents = 1200.0 * fast_log2((a.fast.fslow[j] / a.clean.fhat[b]).abs().max(1e-9));
                if a.moving[b] {
                    if cents.abs() > 6.0 && !root {
                        for bb in b.saturating_sub(1)..=(b + 1).min(n - 1) {
                            a.t_ev[bb] = now;
                        }
                        a.quiet[b] = 0;
                    } else {
                        a.quiet[b] += 1;
                        if a.quiet[b] as f64 * k.tick_s > MOTION_HOLD_S {
                            a.moving[b] = false;
                        }
                    }
                    a.pm[b] = 0;
                } else {
                    let pure = a.fast.pur[j] > PURE_EVENT && a.clean.sel[b] >= 0.5;
                    a.pm[b] = if pure && cents.abs() > 15.0 && !root { a.pm[b] + 1 } else { 0 };
                    if a.pm[b] >= a.ev_ticks {
                        a.moving[b] = true;
                        a.quiet[b] = 0;
                    }
                }
                if a.ne[b] >= a.ev_ticks || a.pm[b] >= a.ev_ticks {
                    for bb in b.saturating_sub(1)..=(b + 1).min(n - 1) {
                        a.t_ev[bb] = now;
                    }
                }
            }
        }
        // ---- the handover x per clean band
        let down = match s.bank {
            BankMode::Dual => k.tick_s / 0.005,
            _ => k.tick_s / 0.020,
        };
        let up = k.tick_s / 0.020;
        for b in 0..n {
            let target = match s.bank {
                BankMode::Fast => 0.0,
                BankMode::Clean => 1.0,
                BankMode::Dual => {
                    let tau = a.clean.tau[b];
                    let t = (now - a.t_ev[b].min(now)) as f64 * k.tick_s;
                    let p = ((t - 3.5 * tau) / (4.0 * tau)).clamp(0.0, 1.0);
                    0.5 - 0.5 * (PI * p).cos()
                }
            };
            let x = a.x[b];
            a.x[b] = if target < x { (x - down).max(target) } else { (x + up).min(target) };
            a.clean.wb[b] = a.x[b];
            if x <= 0.0 && a.x[b] > 0.0 {
                // the fade-in starts: the clean band's phasor is snapped onto the fast band holding
                // the partial (the banks' phases differ by up to 75° off the centres, a 2 dB dip
                // through the crossfade at E2)
                let j0 = a.near[b];
                let mut j = j0;
                for jj in j0.saturating_sub(1)..=(j0 + 1).min(a.fast.n - 1) {
                    if a.fast.ldb[jj] > a.fast.ldb[j] {
                        j = jj;
                    }
                }
                a.clean.q_ang[b] = 0.0;
                a.clean.qr[b] = 1.0;
                a.clean.qi[b] = 0.0;
                // (only from a pure fast band: a beating one gives a random phase)
                if let (Some(c), Some(f), true) = (a.clean.phasor(b), a.fast.phasor(j), a.fast.pur[j] > 0.5) {
                    let ang = (f.1 * c.0 - f.0 * c.1).atan2(f.0 * c.0 + f.1 * c.1);
                    a.clean.q_ang[b] = ang;
                    a.clean.qr[b] = ang.cos();
                    a.clean.qi[b] = ang.sin();
                }
            }
        }
        for j in 0..a.fast.n {
            let (i, fr) = a.interp[j];
            let xi = a.x[i] * (1.0 - fr) + a.x[(i + 1).min(n - 1)] * fr;
            a.fast.wb[j] = 1.0 - xi;
        }
        // ---- gains for the next tick
        let owner = if s.mono > 0.0 { a.owner } else { 0.0 };
        a.clean.targets(&k, &s, owner);
        a.fast.targets(&k, &s, owner);
        // ---- the shaper's own tick
        self.shaper.tick(&self.a);
    }

    // ---------------------------------------------------------------- diagnostics (tests)

    /// The handover x of the clean band nearest `f` (1 = clean).
    pub fn x_at(&self, f: f64) -> f64 {
        let mut best = 0;
        for b in 0..self.a.clean.n {
            if (self.a.clean.spec[b].centre - f).abs() < (self.a.clean.spec[best].centre - f).abs() {
                best = b;
            }
        }
        self.a.x[best]
    }

    /// The bank designs (clean, fast).
    pub fn designs(&self) -> (&[BandSpec], &[BandSpec]) {
        (&self.a.clean.spec, &self.a.fast.spec)
    }

    /// The owned bands' mask-weighted gain (0 – 1 sounding share), max over the bands the owner holds.
    pub fn owned_mask(&self) -> f64 {
        let mut m: f64 = 0.0;
        for bk in [&self.a.clean, &self.a.fast] {
            for b in 0..bk.n {
                if bk.own_t[b] {
                    m = m.max(bk.mkt[b]);
                }
            }
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_math_accuracy() {
        let mut worst = [0.0f64; 3];
        for i in 0..20000 {
            let x = 1e-12 * 1.0021f64.powi(i);
            worst[0] = worst[0].max((fast_log2(x) - x.log2()).abs());
            let y = -60.0 + 120.0 * i as f64 / 20000.0;
            worst[1] = worst[1].max((fast_exp2(y) / y.exp2() - 1.0).abs());
            let a = TAU * i as f64 / 20000.0 - PI;
            let (r, im) = (0.3 * a.cos(), 0.3 * a.sin());
            worst[2] = worst[2].max((fast_atan2(im, r) - im.atan2(r)).abs());
        }
        assert!(worst[0] < 2e-6 && worst[1] < 1e-7 && worst[2] < 1.2e-5, "{worst:?}");
    }
}
