// SPDX-License-Identifier: GPL-3.0-or-later
//! The Spectral core (README §2.2), at the bank rate (22.05 – 24 kHz at every host rate).
//!
//! - **Banks** (gfx-dsp `filterbank`, explicit band lists): **clean** = order 4, ⅓ ERB apart and ⅓ ERB
//!   wide (full −3 dB pole width, never under 20 Hz); **fast** = order 2, ½ ERB apart and wide (never
//!   under 40 Hz). Both grids hang from 2 kHz downwards to 24 Hz (`lowest` × 0.8 at its minimum); a
//!   band under `lowest` × 0.8 is faded out instead of redesigning the bank, so a `lowest` change
//!   moves nothing else (no bank crossfade needed).
//! - **One band per partial**: the band levels L_b (attack 3 ms, release 50 ms) select local maxima,
//!   s_b = c(L_b − L_(b−1)) · c(L_b − L_(b+1)), c(d) = clamp(d / 6 dB + ½, 0, 1); each band's phasor is
//!   turned by its design rotation ρ_b (gfx-dsp `filterbank::design_rotation`), and its level divided by
//!   its skirt gain |H_b(f̂_b)| (f̂_b the band's frequency, 10 ms).
//! - **Harmonic shaper**: y_b = s_b · g_b · 2|z_b| / |H_b(f̂_b)| · Σ_n Re(c_n uⁿ), n ≤ 9 and n f̂ under
//!   fcap = 0.42 × the bank rate (each harmonic faded by its margin to fcap over 500 Hz); purity gate
//!   w_b = exp(−σ²_IF / (0.25 B_b)²) between the shaped band and the band itself; `octave` (u², u⁴,
//!   branch-tracked √u, ⁴√u), `sub` (Re √u under 400 Hz), `detune` (a copy of every harmonic turned by
//!   a rotor running `detune` cents sharp of the band), `noise` (a band of noise around the partial).
//! - **Per-band level**: P_b the peak of 2|z_b| since the last pick (attack 1 ms, held; a pick lets it
//!   go to the band's level), g_b = (P_b / 2|z_b|)^c capped at +30 dB; a band under `thresh` − 10 dB
//!   fades out over 30 ms (6 dB of hysteresis).
//! - **Handover fast → clean** (`bank` Dual) per clean band: an event (a pick; new energy: the fast
//!   bank's level over the clean band's by 6 dB for 5 ms while the clean band rises; pitch motion: the
//!   two banks' frequencies 15 ¢ apart for 5 ms in a pure fast band, then held while they stay 6 ¢
//!   apart) puts the region on the fast bank in 5 ms; the clean band comes back on a raised cosine from
//!   3.5 τ_b to 7.5 τ_b after the last event, τ_b = 1 / (π B_b), its phasor snapped onto the fast
//!   band's at the start of the fade-in. A fast band sounds with 1 − x interpolated at its centre.
//! - **Ownership mask** (Mono): the bands within max(⅙ ERB, half a spacing) of the voice's partials
//!   1 … 16 are muted (10 ms) while the voice sounds, given back over 30 ms.
//!
//! The control work (levels, frequencies, selection, gains, handover) runs every [`TICK`] bank samples;
//! the gains are interpolated across the tick. f64 state, no allocation after `prepare`.

use gfx_dsp::filterbank::{self, BandSpec, ComplexBank};
use gfx_dsp::noise::White;
use std::f64::consts::{PI, TAU};

/// The banks' top centre (Hz): nothing of the guitar above passes.
pub const TOP_HZ: f64 = 2000.0;
/// The grids' lowest centre (Hz): `lowest` 30 Hz × 0.8.
pub const GRID_LOW_HZ: f64 = 24.0;
/// Control work every this many bank samples.
pub const TICK: usize = 16;
/// Harmonics per band.
pub const NH: usize = 9;
/// fcap as a fraction of the bank rate.
pub const FCAP: f64 = 0.42;
/// A harmonic fades over its last this many Hz under fcap.
const FADE_HZ: f64 = 500.0;
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
/// Sub and the branch-tracked roots: bands under this (Hz) carry `sub`.
const SUB_BELOW: f64 = 400.0;

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
fn erb_number(f: f64) -> f64 {
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
fn sqrt_branch(ur: f64, ui: f64, sr: &mut f64, si: &mut f64) -> (f64, f64) {
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

/// Σ_n w_n Re(c_n vⁿ), n ≤ [`NH`], w_n = clamp((fcap − n f) / 500 Hz, 0, 1).
#[inline(always)]
fn series(vr: f64, vi: f64, f: f64, fcap: f64, c: &[(f64, f64); NH]) -> f64 {
    let (mut pr, mut pi) = (vr, vi);
    let mut s = 0.0;
    let mut fnn = f;
    for cn in c.iter() {
        let margin = fcap - fnn;
        if margin <= 0.0 {
            break;
        }
        let w = (margin * (1.0 / FADE_HZ)).min(1.0);
        s += w * (cn.0 * pr - cn.1 * pi);
        let qr = pr * vr - pi * vi;
        pi = pr * vi + pi * vr;
        pr = qr;
        fnn += f;
    }
    s
}

/// The waveform coefficients c_n (README §2.2), normalised so Σ |c_n|² = 1 (the RMS of a unit sine).
pub fn coefs(wave: usize, pw: f64) -> [(f64, f64); NH] {
    let mut c = [(0.0, 0.0); NH];
    for (i, cn) in c.iter_mut().enumerate() {
        let n = (i + 1) as f64;
        *cn = match wave {
            // Saw: −j / n (Σ sin nθ / n, the falling saw)
            0 => (0.0, -1.0 / n),
            // Pulse: sin(π n pw) / n (a pulse centred on θ = 0)
            1 => ((PI * n * pw).sin() / n, 0.0),
            // Triangle: −j (−1)^((n−1)/2) / n², odd n
            2 => {
                if i % 2 == 0 {
                    let s = if (i / 2) % 2 == 0 { 1.0 } else { -1.0 };
                    (0.0, -s / (n * n))
                } else {
                    (0.0, 0.0)
                }
            }
            // Sine: c₁ = 1
            _ => {
                if i == 0 {
                    (1.0, 0.0)
                } else {
                    (0.0, 0.0)
                }
            }
        };
    }
    let e: f64 = c.iter().map(|x| x.0 * x.0 + x.1 * x.1).sum();
    let g = if e > 1e-12 { 1.0 / e.sqrt() } else { 0.0 };
    for cn in c.iter_mut() {
        cn.0 *= g;
        cn.1 *= g;
    }
    c
}

/// What the engine sets on the core at each tick.
#[derive(Clone, Copy, Debug)]
pub struct CoreSettings {
    pub bank: BankMode,
    /// The harmonic law (already crossfaded by the engine).
    pub c: [(f64, f64); NH],
    /// `octave` now and the one being faded to, and the fade (0 = `oct_a` only).
    pub oct_a: i32,
    pub oct_b: i32,
    pub oct_x: f64,
    /// `sub` 0 – 1, `noise` 0 – 1, `compress` 0 – 1.
    pub sub: f64,
    pub noise: f64,
    pub compress: f64,
    /// `detune` (cents).
    pub detune: f64,
    /// `thresh` (dBFS), `lowest` (Hz).
    pub thresh_db: f64,
    pub lowest: f64,
    /// The engine fade (0 Spectral, 1 Mono) and `residue` (0 – 1).
    pub mono: f64,
    pub residue: f64,
}

impl Default for CoreSettings {
    fn default() -> Self {
        Self { bank: BankMode::Dual, c: coefs(0, 0.5), oct_a: 0, oct_b: 0, oct_x: 0.0, sub: 0.0, noise: 0.0, compress: 0.4, detune: 0.0, thresh_db: -66.0, lowest: 70.0, mono: 0.0, residue: 1.0 }
    }
}

/// One bank with its per-band state.
#[derive(Clone, Debug, Default)]
struct Bank {
    bank: ComplexBank,
    n: usize,
    spec: Vec<BandSpec>,
    // constants
    hs: Vec<f64>,
    rot_r: Vec<f64>,
    rot_i: Vec<f64>,
    dem_r: Vec<f64>,
    dem_i: Vec<f64>,
    tau: Vec<f64>,
    inv_bw2: Vec<f64>,
    pole: Vec<f64>,
    low: Vec<bool>,
    // tick state
    zp_r: Vec<f64>,
    zp_i: Vec<f64>,
    lvl2: Vec<f64>,
    ldb: Vec<f64>,
    fm1: Vec<f64>,
    fm2: Vec<f64>,
    fhat: Vec<f64>,
    fslow: Vec<f64>,
    pur: Vec<f64>,
    skirt: Vec<f64>,
    skirt_db: Vec<f64>,
    peak: Vec<f64>,
    gcomp: Vec<f64>,
    gate: Vec<f64>,
    gate_open: Vec<bool>,
    sel: Vec<f64>,
    en: Vec<f64>,
    /// handover weight (clean: x; fast: 1 − x at its centre)
    wb: Vec<f64>,
    own: Vec<f64>,
    own_t: Vec<bool>,
    // per-sample gains: pre-mask gain k and mask mk, each with its step over the tick
    k: Vec<f64>,
    kt: Vec<f64>,
    ks: Vec<f64>,
    mk: Vec<f64>,
    mkt: Vec<f64>,
    mks: Vec<f64>,
    // per-sample state
    vr_last: Vec<f64>,
    vi_last: Vec<f64>,
    s1r: Vec<f64>,
    s1i: Vec<f64>,
    s2r: Vec<f64>,
    s2i: Vec<f64>,
    dr: Vec<f64>,
    di: Vec<f64>,
    der: Vec<f64>,
    dei: Vec<f64>,
    nr: Vec<f64>,
    ni: Vec<f64>,
    na: Vec<f64>,
    pwr: Vec<f64>,
    /// the handover snap (clean bank): the band's phasor turned by q onto the fast bank's at the start
    /// of its fade-in, and held (README §2.2 relaxes it to 0 at ≤ 5 ¢: that cost 1 – 12 dB of chord
    /// cleanliness, the bands of one partial no longer turned alike)
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
        self.low = vec![false; n];
        for v in [
            &mut self.zp_r, &mut self.zp_i, &mut self.lvl2, &mut self.ldb, &mut self.fm1, &mut self.fm2, &mut self.fhat, &mut self.fslow, &mut self.pur, &mut self.skirt, &mut self.skirt_db, &mut self.peak, &mut self.gcomp, &mut self.gate, &mut self.sel, &mut self.en, &mut self.wb, &mut self.own, &mut self.k, &mut self.kt, &mut self.ks, &mut self.mk, &mut self.mkt, &mut self.mks, &mut self.vr_last, &mut self.vi_last, &mut self.s1r, &mut self.s1i, &mut self.s2r, &mut self.s2i, &mut self.dr, &mut self.di, &mut self.der, &mut self.dei, &mut self.nr, &mut self.ni, &mut self.na, &mut self.pwr, &mut self.q_ang, &mut self.qr, &mut self.qi,
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
            self.low[b] = s.centre < SUB_BELOW;
            // the noise band: a complex one-pole of corner max(B, f / 4)
            let nc = s.width.max(0.25 * s.centre);
            self.na[b] = 1.0 - (-TAU * nc / fb).exp();
        }
        self.reset();
    }

    fn reset(&mut self) {
        self.bank.reset();
        for v in [
            &mut self.zp_r, &mut self.zp_i, &mut self.lvl2, &mut self.fm1, &mut self.fm2, &mut self.pur, &mut self.peak, &mut self.gate, &mut self.sel, &mut self.wb, &mut self.own, &mut self.k, &mut self.kt, &mut self.ks, &mut self.mk, &mut self.mkt, &mut self.mks, &mut self.vr_last, &mut self.vi_last, &mut self.s1i, &mut self.s2i, &mut self.di, &mut self.nr, &mut self.ni, &mut self.pwr, &mut self.q_ang, &mut self.qi,
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
            self.s1r[b] = 1.0;
            self.s2r[b] = 1.0;
            self.dr[b] = 1.0;
            self.der[b] = 1.0;
            self.dei[b] = 0.0;
        }
        self.gate_open.iter_mut().for_each(|x| *x = false);
        self.own_t.iter_mut().for_each(|x| *x = false);
    }

    /// Band b's phasor now, turned by its design rotation (and its snap), or None under 1e-12.
    fn phasor(&self, b: usize) -> Option<(f64, f64)> {
        let (zr, zi) = (self.bank.re()[b], self.bank.im()[b]);
        let m = (zr * zr + zi * zi).sqrt();
        if m < 1e-12 {
            return None;
        }
        let (rr, ri) = (self.rot_r[b] * self.qr[b] - self.rot_i[b] * self.qi[b], self.rot_r[b] * self.qi[b] + self.rot_i[b] * self.qr[b]);
        Some(((zr * rr - zi * ri) / m, (zr * ri + zi * rr) / m))
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

    /// The gains for the next tick, the detune rotors and the mask.
    fn targets(&mut self, k: &TickConsts, s: &CoreSettings, owner: f64) {
        let delta = (s.detune / 1200.0).exp2() - 1.0;
        let fac = (s.oct_b as f64).exp2();
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
            // the detune rotor's step: δ times the (transposed) band's phase increment
            if s.detune > 0.0 {
                let w = delta * TAU * self.fhat[b] * fac / k.fb;
                self.der[b] = w.cos();
                self.dei[b] = w.sin();
                let m = (self.dr[b] * self.dr[b] + self.di[b] * self.di[b]).sqrt();
                if m > 1e-12 {
                    self.dr[b] /= m;
                    self.di[b] /= m;
                } else {
                    self.dr[b] = 1.0;
                    self.di[b] = 0.0;
                }
            } else {
                self.dr[b] = 1.0;
                self.di[b] = 0.0;
            }
        }
    }

    /// One bank sample of every sounding band: (main, copy).
    #[inline]
    fn render(&mut self, r: &RenderConsts, noise: &mut White) -> (f64, f64) {
        let re = self.bank.re();
        let im = self.bank.im();
        let (mut out_a, mut out_b) = (0.0, 0.0);
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
            // the roots, when an octave down or `sub` needs them
            let (mut r1, mut r2) = ((1.0, 0.0), (1.0, 0.0));
            if r.need_r1 {
                r1 = sqrt_branch(ur, ui, &mut self.s1r[b], &mut self.s1i[b]);
                if r.need_r2 {
                    r2 = sqrt_branch(r1.0, r1.1, &mut self.s2r[b], &mut self.s2i[b]);
                }
            }
            let f = self.fhat[b];
            let pur = self.pur[b];
            let amp = m * kk;
            let detune_on = r.copy > 0.0;
            let (mut s_main, mut s_copy) = (0.0, 0.0);
            let (dr, di) = (self.dr[b], self.di[b]);
            for (oct, w) in [(r.oct_a, 1.0 - r.oct_x), (r.oct_b, r.oct_x)] {
                if w <= 0.0 {
                    continue;
                }
                let (vr, vi) = match oct {
                    0 => (ur, ui),
                    1 => (ur * ur - ui * ui, 2.0 * ur * ui),
                    2 => {
                        let (ar, ai) = (ur * ur - ui * ui, 2.0 * ur * ui);
                        (ar * ar - ai * ai, 2.0 * ar * ai)
                    }
                    -1 => r1,
                    _ => r2,
                };
                let fv = f * (oct as f64).exp2();
                let w1 = ((r.fcap - fv) * (1.0 / FADE_HZ)).clamp(0.0, 1.0);
                let sh = series(vr, vi, fv, r.fcap, &r.c);
                // the band unshaped: its fundamental at the wave's phase
                s_main += w * (pur * sh + (1.0 - pur) * w1 * (r.c1r * vr - r.c1i * vi));
                if detune_on {
                    let (cr, ci) = (vr * dr - vi * di, vr * di + vi * dr);
                    let fd = fv * r.ratio;
                    let w1d = ((r.fcap - fd) * (1.0 / FADE_HZ)).clamp(0.0, 1.0);
                    let sd = series(cr, ci, fd, r.fcap, &r.c);
                    s_copy += w * (pur * sd + (1.0 - pur) * w1d * (r.c1r * cr - r.c1i * ci));
                }
                if oct == r.oct_b {
                    self.vr_last[b] = vr;
                    self.vi_last[b] = vi;
                }
            }
            if detune_on {
                let nr = dr * self.der[b] - di * self.dei[b];
                self.di[b] = dr * self.dei[b] + di * self.der[b];
                self.dr[b] = nr;
            }
            let mut y = r.main * s_main;
            if r.sub > 0.0 && self.low[b] {
                y += r.sub * r1.0;
            }
            if r.noise > 0.0 {
                let a = self.na[b];
                self.nr[b] += (noise.next_f64() - self.nr[b]) * a;
                self.ni[b] += (noise.next_f64() - self.ni[b]) * a;
                // unit RMS of Re(u ξ): the one-pole's variance a / (2 − a) of a uniform (1 / 3) source
                let g = ((2.0 - a) * 3.0 / a).sqrt() * std::f64::consts::FRAC_1_SQRT_2;
                y += r.noise * g * (ur * self.nr[b] - ui * self.ni[b]);
            }
            let ya = amp * y;
            if r.measure {
                // the band's power from its envelope (the shaped wave's RMS is that of a sine of the
                // same amplitude): no per-period ripple
                let p2 = 0.5 * amp * amp;
                self.pwr[b] += (p2 - self.pwr[b]) * if p2 > self.pwr[b] { r.a_pw_att } else { r.a_pw };
            }
            out_a += ya * mk;
            if detune_on {
                out_b += amp * r.copy * s_copy * mk;
            }
        }
        (out_a, out_b)
    }
}

/// Coefficients of a tick.
#[derive(Clone, Copy, Debug, Default)]
struct TickConsts {
    fb: f64,
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
    tick_s: f64,
    a_rise: f64,
    a_slow: f64,
}

/// What every band of a sample shares.
#[derive(Clone, Copy, Debug, Default)]
struct RenderConsts {
    c: [(f64, f64); NH],
    /// c₁ / |c₁| (the unshaped band's phase)
    c1r: f64,
    c1i: f64,
    fcap: f64,
    oct_a: i32,
    oct_b: i32,
    oct_x: f64,
    need_r1: bool,
    need_r2: bool,
    main: f64,
    copy: f64,
    ratio: f64,
    sub: f64,
    noise: f64,
    measure: bool,
    a_pw: f64,
    a_pw_att: f64,
}

/// The Spectral core.
#[derive(Clone, Debug, Default)]
pub struct Core {
    fb: f64,
    clean: Bank,
    fast: Bank,
    /// clean band → nearest fast band; fast band → (clean index below, fraction)
    near: Vec<usize>,
    interp: Vec<(usize, f64)>,
    /// handover state per clean band
    x: Vec<f64>,
    t_ev: Vec<u64>,
    ne: Vec<u32>,
    pm: Vec<u32>,
    lc_prev: Vec<f64>,
    moving: Vec<bool>,
    quiet: Vec<u32>,
    ticks: u64,
    phase: usize,
    k: TickConsts,
    r: RenderConsts,
    s: CoreSettings,
    owner: f64,
    pick: bool,
    noise: White,
    ev_ticks: u32,
}

impl Core {
    pub fn new() -> Self {
        Self::default()
    }

    /// Designs both banks for the bank rate `fb` (allocates).
    pub fn prepare(&mut self, fb: f64) {
        self.fb = fb;
        let cg = clean_grid();
        let fg = fast_grid();
        self.clean.prepare(&cg, fb);
        self.fast.prepare(&fg, fb);
        self.near = cg
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
        self.interp = fg
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
        self.x = vec![1.0; n];
        self.t_ev = vec![0; n];
        self.ne = vec![0; n];
        self.pm = vec![0; n];
        self.lc_prev = vec![-200.0; n];
        self.moving = vec![false; n];
        self.quiet = vec![0; n];
        let t = TICK as f64;
        let ms = |v: f64| 1.0 - (-t / (v * 1e-3 * fb)).exp();
        self.k = TickConsts {
            fb,
            a_att: ms(3.0),
            a_rel: ms(50.0),
            a_f: ms(10.0),
            hz_per_rad: fb / (TAU * t),
            a_pk: ms(1.0),
            // the body is held between picks (README: "release 3 s", which lost 6.7 dB in 2 s of a
            // T60 3 s string, against the 1.5 dB of the Hold row)
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
        self.ev_ticks = (0.005 / self.k.tick_s).ceil() as u32;
        self.r.fcap = FCAP * fb;
        // the owned bands' power: attack 2 ms, release 20 ms (a 10 ms mean lagged the attack by 3 dB)
        self.r.a_pw = 1.0 - (-1.0 / (0.020 * fb)).exp();
        self.r.a_pw_att = 1.0 - (-1.0 / (0.002 * fb)).exp();
        self.reset();
    }

    pub fn reset(&mut self) {
        self.clean.reset();
        self.fast.reset();
        self.x.iter_mut().for_each(|v| *v = 1.0);
        // no event yet: as if the last one were long ago
        self.ticks = 1 << 40;
        self.t_ev.iter_mut().for_each(|v| *v = 0);
        self.ne.iter_mut().for_each(|v| *v = 0);
        self.pm.iter_mut().for_each(|v| *v = 0);
        self.lc_prev.iter_mut().for_each(|v| *v = -200.0);
        self.moving.iter_mut().for_each(|v| *v = false);
        self.quiet.iter_mut().for_each(|v| *v = 0);
        self.phase = 0;
        self.owner = 0.0;
        self.pick = false;
        self.noise = White::new(0x5917_C0DE);
        self.noise.reset();
        // bands enabled at once on the first tick
        let lo = 0.8 * self.s.lowest;
        for bk in [&mut self.clean, &mut self.fast] {
            for b in 0..bk.n {
                bk.en[b] = if bk.spec[b].centre >= lo * (1.0 - 1e-9) { 1.0 } else { 0.0 };
            }
        }
    }

    pub fn bank_rate(&self) -> f64 {
        self.fb
    }

    /// The settings for the coming ticks.
    pub fn set(&mut self, s: CoreSettings) {
        self.s = s;
    }

    /// A pick: every band goes to the fast bank (Dual).
    pub fn pick(&mut self) {
        self.pick = true;
    }

    /// The voice's note (Hz, after `octave`'s transposition is undone: the string's f0), 0 = none.
    pub fn set_owner(&mut self, hz: f64) {
        self.owner = hz;
    }

    /// One bank sample → (main, detune copy).
    #[inline]
    pub fn process(&mut self, x: f64) -> (f64, f64) {
        self.clean.bank.process(x);
        self.fast.bank.process(x);
        if self.phase == 0 {
            self.tick();
        }
        self.phase += 1;
        if self.phase == TICK {
            self.phase = 0;
        }
        let (a1, b1) = self.clean.render(&self.r, &mut self.noise);
        let (a2, b2) = self.fast.render(&self.r, &mut self.noise);
        (a1 + a2, b1 + b2)
    }

    fn tick(&mut self) {
        self.ticks += 1;
        let k = self.k;
        let s = self.s;
        let pick = self.pick;
        self.clean.analyse(&k, &s, pick);
        self.fast.analyse(&k, &s, pick);
        // ---- events (Dual)
        let now = self.ticks;
        if std::mem::take(&mut self.pick) {
            self.t_ev.iter_mut().for_each(|t| *t = now);
        }
        let n = self.clean.n;
        if s.bank == BankMode::Dual {
            for b in 0..n {
                // rising: the level over its own 5 ms mean by 1 dB (a hammer-on's clean band climbs
                // ~0.5 dB / ms; a steady partial's skirt band moves 0.1 dB / ms under a vibrato)
                let l = self.clean.ldb[b].max(-200.0);
                self.lc_prev[b] += (l - self.lc_prev[b]) * k.a_rise;
                let rising = l > self.lc_prev[b] + 1.0;
                if !self.clean.gate_open[b] {
                    self.ne[b] = 0;
                    self.pm[b] = 0;
                    continue;
                }
                // the fast band holding this partial: the loudest around the nearest centre
                let j0 = self.near[b];
                let mut j = j0;
                for jj in j0.saturating_sub(1)..=(j0 + 1).min(self.fast.n - 1) {
                    if self.fast.ldb[jj] > self.fast.ldb[j] {
                        j = jj;
                    }
                }
                // new energy: the fast bank's level over the clean band's by 3 dB while the clean band
                // is still rising (a steady chord's beating fast band does not count; a new note's
                // fast band is not pure for its first 50 ms, so purity is not asked here)
                let lc = self.clean.ldb[b] + self.clean.skirt_db[b];
                let lf = self.fast.ldb[j] + self.fast.skirt_db[j];
                self.ne[b] = if rising && lf - lc > NEW_ENERGY_DB { self.ne[b] + 1 } else { 0 };
                // pitch motion: on a partial the clean band holds, read in a pure fast band, 15 ¢ for
                // 5 ms; then held while the banks stay 6 ¢ apart (a vibrato crosses both bounds twice a
                // cycle; without the hold the clean bank came back between and the crossfades chirped).
                // The fast band's frequency is read through a further 15 ms: a chord's beat (≥ 30 Hz)
                // wobbles it ±10 – 20 ¢, a vibrato (≤ 8 Hz) passes
                let cents = 1200.0 * fast_log2((self.fast.fslow[j] / self.clean.fhat[b]).abs().max(1e-9));
                if self.moving[b] {
                    if cents.abs() > 6.0 {
                        for bb in b.saturating_sub(1)..=(b + 1).min(n - 1) {
                            self.t_ev[bb] = now;
                        }
                        self.quiet[b] = 0;
                    } else {
                        self.quiet[b] += 1;
                        if self.quiet[b] as f64 * k.tick_s > MOTION_HOLD_S {
                            self.moving[b] = false;
                        }
                    }
                    self.pm[b] = 0;
                } else {
                    let pure = self.fast.pur[j] > PURE_EVENT && self.clean.sel[b] >= 0.5;
                    self.pm[b] = if pure && cents.abs() > 15.0 { self.pm[b] + 1 } else { 0 };
                    if self.pm[b] >= self.ev_ticks {
                        self.moving[b] = true;
                        self.quiet[b] = 0;
                    }
                }
                if self.ne[b] >= self.ev_ticks || self.pm[b] >= self.ev_ticks {
                    for bb in b.saturating_sub(1)..=(b + 1).min(n - 1) {
                        self.t_ev[bb] = now;
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
                    let tau = self.clean.tau[b];
                    let t = (now - self.t_ev[b].min(now)) as f64 * k.tick_s;
                    let p = ((t - 3.5 * tau) / (4.0 * tau)).clamp(0.0, 1.0);
                    0.5 - 0.5 * (PI * p).cos()
                }
            };
            let x = self.x[b];
            self.x[b] = if target < x { (x - down).max(target) } else { (x + up).min(target) };
            self.clean.wb[b] = self.x[b];
            if x <= 0.0 && self.x[b] > 0.0 {
                // the fade-in starts: the clean band's phasor is snapped onto the fast band holding
                // the partial (README §2.2's fallback: the banks' phases differ by up to 75° off the
                // centres, a 2 dB dip through the crossfade at E2)
                let j0 = self.near[b];
                let mut j = j0;
                for jj in j0.saturating_sub(1)..=(j0 + 1).min(self.fast.n - 1) {
                    if self.fast.ldb[jj] > self.fast.ldb[j] {
                        j = jj;
                    }
                }
                self.clean.q_ang[b] = 0.0;
                self.clean.qr[b] = 1.0;
                self.clean.qi[b] = 0.0;
                // (only from a pure fast band: a beating one gives a random phase)
                if let (Some(c), Some(f), true) = (self.clean.phasor(b), self.fast.phasor(j), self.fast.pur[j] > 0.5) {
                    let a = (f.1 * c.0 - f.0 * c.1).atan2(f.0 * c.0 + f.1 * c.1);
                    self.clean.q_ang[b] = a;
                    self.clean.qr[b] = a.cos();
                    self.clean.qi[b] = a.sin();
                }
            }
        }
        for j in 0..self.fast.n {
            let (i, fr) = self.interp[j];
            let xi = self.x[i] * (1.0 - fr) + self.x[(i + 1).min(n - 1)] * fr;
            self.fast.wb[j] = 1.0 - xi;
        }
        // ---- gains for the next tick
        let owner = if s.mono > 0.0 { self.owner } else { 0.0 };
        self.clean.targets(&k, &s, owner);
        self.fast.targets(&k, &s, owner);
        // ---- what every band shares this tick
        let r = &mut self.r;
        r.c = s.c;
        let m1 = (s.c[0].0 * s.c[0].0 + s.c[0].1 * s.c[0].1).sqrt();
        (r.c1r, r.c1i) = if m1 > 1e-9 { (s.c[0].0 / m1, s.c[0].1 / m1) } else { (1.0, 0.0) };
        r.oct_a = s.oct_a;
        r.oct_b = s.oct_b;
        r.oct_x = s.oct_x.clamp(0.0, 1.0);
        r.need_r1 = s.sub > 0.0 || (r.oct_a < 0 && r.oct_x < 1.0) || (r.oct_b < 0 && r.oct_x > 0.0);
        r.need_r2 = (r.oct_a == -2 && r.oct_x < 1.0) || (r.oct_b == -2 && r.oct_x > 0.0);
        // detune: the copy and the original at −3 dB each, faded in over the first 2 ¢
        let th = 0.25 * PI * (s.detune / 2.0).clamp(0.0, 1.0);
        r.main = th.cos();
        r.copy = if s.detune > 0.0 { th.sin() } else { 0.0 };
        r.ratio = (s.detune / 1200.0).exp2();
        r.sub = s.sub;
        r.noise = s.noise;
        r.measure = s.mono > 0.0;
    }

    // ---------------------------------------------------------------- the bridge

    /// The RMS (power follower 2 / 20 ms) of the core bands the owner takes, before the mask: the level the voice must
    /// start at (README §2.4, the bridge's step 4).
    pub fn owned_rms(&self, f0: f64) -> f64 {
        let mut p = 0.0;
        for bk in [&self.clean, &self.fast] {
            for b in 0..bk.n {
                let fc = bk.spec[b].centre;
                let h = (fc / f0).round();
                if (1.0..=16.0).contains(&h) && (fc - h * f0).abs() <= (filterbank::erb(fc) / 6.0).max(bk.hs[b]) {
                    p += bk.pwr[b];
                }
            }
        }
        p.sqrt()
    }

    /// The phase (radians) of the loudest sounding band at `f` (the transposed fundamental), if any:
    /// the voice's oscillator starts there (the bridge's step 6).
    pub fn phase_at(&self, f: f64) -> Option<f64> {
        let mut best: Option<(f64, f64)> = None;
        for bk in [&self.clean, &self.fast] {
            for b in 0..bk.n {
                if (bk.spec[b].centre - f).abs() > 1.5 * bk.hs[b] {
                    continue;
                }
                let w = bk.kt[b];
                if w > 0.0 && best.is_none_or(|(bw, _)| w > bw) {
                    best = Some((w, bk.vi_last[b].atan2(bk.vr_last[b])));
                }
            }
        }
        best.map(|(_, p)| p)
    }

    // ---------------------------------------------------------------- diagnostics (tests)

    /// The handover x of the clean band nearest `f` (1 = clean).
    pub fn x_at(&self, f: f64) -> f64 {
        let mut best = 0;
        for b in 0..self.clean.n {
            if (self.clean.spec[b].centre - f).abs() < (self.clean.spec[best].centre - f).abs() {
                best = b;
            }
        }
        self.x[best]
    }

    /// The bank designs (clean, fast).
    pub fn designs(&self) -> (&[BandSpec], &[BandSpec]) {
        (&self.clean.spec, &self.fast.spec)
    }

    /// The owned bands' mask-weighted gain (0 – 1 sounding share), max over the bands the owner holds.
    pub fn owned_mask(&self) -> f64 {
        let mut m: f64 = 0.0;
        for bk in [&self.clean, &self.fast] {
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
