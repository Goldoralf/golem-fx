// SPDX-License-Identifier: GPL-3.0-or-later
//! The root sieve of the Spectral core (gfx-organ README §2.3, as rewritten by its review): which
//! selected bands of the **clean** bank are a note's fundamental, and which are an overtone of a lower
//! sounding band. Ported from the reviewed prototype (`gfx-organ/proto/sieve_review.py`, `Reviewed.sieve`)
//! step by step, causally: the prototype's pick snapshot 5 ms before the onset is a ring of the last
//! ticks here.
//!
//! Per control tick:
//! 1. **Candidates**: selected (s_b > 0.5), above the absolute floor and within `depth_db` of the
//!    loudest band.
//! 2. **Overtone test** of candidate p against each lower candidate q, k = 2 … `kmax`:
//!    −tol − e < 1200 log₂(f̂_p / (k f̂_q)) < tol + e + 866 · B_max · (k² − 1) ¢, and L_q > L_p − `parent_db`.
//! 3. **Settling allowance** e = e_p + e_q, e_b = clamp(15 ¢ × (9.5 − age_b / τ_b), 0, 80 ¢); the *exact*
//!    test has e = 0.
//! 4. **Impure** candidates (purity < 0.5) are never roots.
//! 5. **Same pick**: q counts against p only if q was struck by the pick that brought p in (complex
//!    novelty over 20 ms + 3 τ of the widest band after each pick, or entering the selection within
//!    150 ms of it; q itself, or one of its partials m ≤ 8 that is not a partial of p).
//! 6. **Confirmation**: a root once the exact test passed for 4 τ_b without a break and the widened
//!    one passes now; once confirmed the exact test alone keeps it.
//! 7. **Status travels with the partial**: into the neighbouring band within 50 ¢ (at once under
//!    20 ms of gap), or back after a gap of the selection up to 250 ms (pick and age restored).
//! 8. **Undecided**: a candidate not confirmed, during its first max(2 × 4 τ_b, 220 ms).
//!
//! Outputs per clean band: [`Sieve::root`] (F_b, 0 – 1, a 10 ms one-pole of the decision) and
//! [`Sieve::undecided`] (U_b, 5 ms). No allocation after `prepare`.

use crate::spectral::{fast_log2, Bank};
use std::f64::consts::TAU;

/// The sieve's constants (gfx-organ §2.3; the reviewed prototype's values).
#[derive(Clone, Copy, Debug)]
pub struct SieveParams {
    /// The exact window (¢).
    pub tol_c: f64,
    /// String stretch allowed (B_max).
    pub b_max: f64,
    /// Highest k tested.
    pub kmax: usize,
    /// Candidates within this of the loudest band (dB).
    pub depth_db: f64,
    /// A parent within this of the candidate's level (dB).
    pub parent_db: f64,
    /// Confirmation (τ_b).
    pub confirm: f64,
    /// Purity under this: never a root.
    pub pure_min: f64,
    /// Settling allowance: slope (¢ per τ_b), end (τ_b), cap (¢).
    pub settle_slope: f64,
    pub settle_end: f64,
    pub settle_cap: f64,
    /// A band belongs to a pick when it gained more than this × what it held.
    pub nov_thr: f64,
    /// The same-pick rule (step 5) on.
    pub same_pick: bool,
    /// The "lowest candidate" fast path (README §1.4 (6), open question 1): off in the design.
    pub lowest_fast: bool,
}

impl Default for SieveParams {
    fn default() -> Self {
        Self { tol_c: 6.0, b_max: 2e-4, kmax: 48, depth_db: 40.0, parent_db: 40.0, confirm: 4.0, pure_min: 0.5, settle_slope: 15.0, settle_end: 9.5, settle_cap: 80.0, nov_thr: 1.0, same_pick: true, lowest_fast: false }
    }
}

/// What the sieve measured at its last tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct SieveStats {
    /// Confirmed roots now.
    pub roots: usize,
    /// Candidates now.
    pub candidates: usize,
}

const MAX_K: usize = 64;
/// Events kept (their "struck" masks).
const EVENTS: usize = 32;
/// Ticks of phasor history (the snapshot is taken this far back, ≥ 5 ms at every bank rate).
const HIST: usize = 8;

/// The root sieve's state per clean band.
#[derive(Clone, Debug, Default)]
pub struct Sieve {
    pub p: SieveParams,
    n: usize,
    tick_s: f64,
    /// −20 dB per decade… one-pole coefficients of F (10 ms) and U (5 ms) at the tick rate
    a_f: f64,
    a_u: f64,
    // constants per band
    tau_t: Vec<f64>,
    need: Vec<u32>,
    lim: Vec<u32>,
    // tick constants
    gmax: u32,
    hold_t: u64,
    late_t: u64,
    ev_len: u64,
    back: usize,
    log_k: Vec<f64>,
    st_k: Vec<f64>,
    // state per band
    cand: Vec<bool>,
    prev_c: Vec<bool>,
    gap: Vec<u32>,
    age: Vec<u32>,
    run: Vec<u32>,
    runl: Vec<u32>,
    conf: Vec<bool>,
    birth: Vec<i64>,
    lastc: Vec<u64>,
    lastf: Vec<f64>,
    lastb: Vec<i64>,
    lasta: Vec<u32>,
    touched: Vec<bool>,
    f_out: Vec<f64>,
    u_out: Vec<f64>,
    // phasor history (ring of HIST ticks)
    hz_r: Vec<f64>,
    hz_i: Vec<f64>,
    hf: Vec<f64>,
    h_at: usize,
    // the open pick window
    snap_r: Vec<f64>,
    snap_i: Vec<f64>,
    snap_f: Vec<f64>,
    snap_dt: u64,
    t_open: u64,
    e_open: i64,
    /// events seen; the last pick's tick
    ne: i64,
    last_on: u64,
    /// struck masks of the last EVENTS events (bit b = clean band b)
    struck: [u128; EVENTS],
    // per tick scratch
    pk: Vec<usize>,
    fexact: Vec<bool>,
    fwide: Vec<bool>,
    low: Vec<bool>,
    inherit: Vec<bool>,
    got: Vec<bool>,
    ex: Vec<f64>,
    stats: SieveStats,
    /// test hook: a band forced out of the roots (the ghost measurement's "keep" mask), all true by default
    pub keep: Vec<bool>,
}

#[inline]
fn bit(b: usize) -> u128 {
    1u128 << b
}

impl Sieve {
    pub fn prepare(&mut self, clean: &Bank, tick_s: f64) {
        let n = clean.n;
        assert!(n <= 128, "the sieve's masks hold 128 bands");
        self.n = n;
        self.tick_s = tick_s;
        self.a_f = 1.0 - (-tick_s / 0.010).exp();
        self.a_u = 1.0 - (-tick_s / 0.005).exp();
        let ticks = |s: f64| s / tick_s;
        self.tau_t = clean.tau.iter().map(|t| t / tick_s).collect();
        self.need = self.tau_t.iter().map(|t| (self.p.confirm * t).ceil() as u32).collect();
        self.lim = self.need.iter().map(|nd| (2 * nd).max(ticks(0.22) as u32)).collect();
        self.gmax = ticks(0.020) as u32;
        self.hold_t = ticks(0.250) as u64;
        self.late_t = ticks(0.150) as u64;
        let tau_max = clean.tau.iter().cloned().fold(0.0, f64::max);
        self.ev_len = ticks(0.020 + 3.0 * tau_max) as u64;
        self.back = (ticks(0.005) as usize).clamp(1, HIST - 1);
        self.log_k = vec![0.0; MAX_K + 1];
        self.st_k = vec![0.0; MAX_K + 1];
        for k in 0..=MAX_K {
            self.log_k[k] = if k > 0 { 1200.0 * (k as f64).log2() } else { 0.0 };
            self.st_k[k] = 1200.0 / std::f64::consts::LN_2 * self.p.b_max * ((k * k) as f64 - 1.0) / 2.0;
        }
        let z = || vec![0.0; n];
        let zb = || vec![false; n];
        let zu = || vec![0u32; n];
        self.cand = zb();
        self.prev_c = zb();
        self.gap = zu();
        self.age = zu();
        self.run = zu();
        self.runl = zu();
        self.conf = zb();
        self.birth = vec![-1; n];
        self.lastc = vec![0; n];
        self.lastf = z();
        self.lastb = vec![-1; n];
        self.lasta = zu();
        self.touched = zb();
        self.f_out = z();
        self.u_out = z();
        self.hz_r = vec![0.0; n * HIST];
        self.hz_i = vec![0.0; n * HIST];
        self.hf = vec![0.0; n * HIST];
        self.snap_r = z();
        self.snap_i = z();
        self.snap_f = z();
        self.pk = Vec::with_capacity(n);
        self.fexact = zb();
        self.fwide = zb();
        self.low = zb();
        self.inherit = zb();
        self.got = zb();
        self.ex = z();
        self.keep = vec![true; n];
        self.reset();
    }

    pub fn reset(&mut self) {
        for v in [&mut self.cand, &mut self.prev_c, &mut self.conf, &mut self.touched] {
            v.iter_mut().for_each(|x| *x = false);
        }
        for v in [&mut self.gap, &mut self.age, &mut self.run, &mut self.runl, &mut self.lasta] {
            v.iter_mut().for_each(|x| *x = 0);
        }
        self.birth.iter_mut().for_each(|x| *x = -1);
        self.lastb.iter_mut().for_each(|x| *x = -1);
        // "long ago": lastc such that t − lastc > hold_t at once
        self.lastc.iter_mut().for_each(|x| *x = 0);
        for v in [&mut self.lastf, &mut self.f_out, &mut self.u_out, &mut self.hz_r, &mut self.hz_i, &mut self.hf, &mut self.snap_r, &mut self.snap_i, &mut self.snap_f] {
            v.iter_mut().for_each(|x| *x = 0.0);
        }
        self.h_at = 0;
        self.snap_dt = 0;
        self.t_open = 0;
        self.e_open = -1;
        self.ne = 0;
        self.last_on = 0;
        self.struck = [0; EVENTS];
        self.stats = SieveStats::default();
    }

    /// Band b is a confirmed root.
    #[inline]
    pub fn holds_root(&self, b: usize) -> bool {
        self.conf[b]
    }

    /// F_b, the root weight (0 – 1).
    #[inline]
    pub fn root(&self, b: usize) -> f64 {
        self.f_out[b]
    }

    /// U_b, the undecided weight (0 – 1).
    #[inline]
    pub fn undecided(&self, b: usize) -> f64 {
        self.u_out[b]
    }

    /// Band b is a candidate now.
    #[inline]
    pub fn candidate(&self, b: usize) -> bool {
        self.cand[b]
    }

    pub fn stats(&self) -> SieveStats {
        self.stats
    }

    #[inline]
    fn settle(&self, b: usize) -> f64 {
        let p = &self.p;
        (p.settle_slope * (p.settle_end - self.age[b] as f64 / self.tau_t[b])).clamp(0.0, p.settle_cap)
    }

    #[inline]
    fn cents(a: f64, b: f64) -> f64 {
        1200.0 * fast_log2(a / b)
    }

    /// Was band q struck by event e? Itself, or one of its partials m ≤ 8 (not a multiple of k) that
    /// the event struck and that is a candidate now.
    fn excited(&self, clean: &Bank, q: usize, k: usize, e: i64, tolx: f64) -> bool {
        if e < 0 || self.ne - e > EVENTS as i64 {
            return false;
        }
        let mask = self.struck[(e as usize) % EVENTS];
        if mask & bit(q) != 0 {
            return true;
        }
        let fq = clean.fhat[q];
        let mut m = mask >> (q + 1);
        let mut b = q + 1;
        while m != 0 {
            let tz = m.trailing_zeros() as usize;
            b += tz;
            m >>= tz;
            if self.cand[b] {
                let mm = (clean.fhat[b] / fq).round() as usize;
                if (2..=8).contains(&mm) && !mm.is_multiple_of(k) {
                    let c = Self::cents(clean.fhat[b], mm as f64 * fq);
                    if -self.p.tol_c - tolx < c && c < self.p.tol_c + tolx + self.st_k[mm] {
                        return true;
                    }
                }
            }
            m >>= 1;
            b += 1;
        }
        false
    }

    /// One control tick on the clean bank. `picked`: a pick (detector A) arrived at this tick; `t` the
    /// tick count; `thresh_db` the absolute floor (amplitude dBFS).
    pub fn tick(&mut self, clean: &Bank, picked: bool, t: u64, thresh_db: f64) {
        let n = self.n;
        let p = self.p;
        // phasor history
        let re = clean.bank.re();
        let im = clean.bank.im();
        self.h_at = (self.h_at + 1) % HIST;
        let base = self.h_at * n;
        self.hz_r[base..base + n].copy_from_slice(&re[..n]);
        self.hz_i[base..base + n].copy_from_slice(&im[..n]);
        self.hf[base..base + n].copy_from_slice(&clean.fhat[..n]);
        // 1. candidates (ldb is 10 log |z|², a sine's amplitude is 2 |z|: +6.02 dB)
        let lmax = clean.ldb.iter().cloned().fold(-1000.0, f64::max);
        let floor = thresh_db - 6.0206;
        let mut ncand = 0;
        for b in 0..n {
            let c = clean.sel[b] > 0.5 && clean.ldb[b] > lmax - p.depth_db && clean.ldb[b] > floor;
            self.cand[b] = c;
            ncand += c as usize;
            self.gap[b] = if c { 0 } else { self.gap[b] + 1 };
            self.age[b] = if c {
                self.age[b] + 1
            } else if self.gap[b] > self.gmax {
                0
            } else {
                self.age[b]
            };
        }
        // picks: snapshot 5 ms back, running novelty, decision at the window's end
        if picked {
            let hb = ((self.h_at + HIST - self.back) % HIST) * n;
            self.snap_r.copy_from_slice(&self.hz_r[hb..hb + n]);
            self.snap_i.copy_from_slice(&self.hz_i[hb..hb + n]);
            self.snap_f.copy_from_slice(&self.hf[hb..hb + n]);
            self.snap_dt = self.back as u64;
            self.e_open = self.ne;
            self.struck[(self.ne as usize) % EVENTS] = 0;
            self.touched.iter_mut().for_each(|x| *x = false);
            self.ne += 1;
            self.t_open = t;
            self.last_on = t;
        }
        if self.e_open >= 0 {
            let fb = 1.0 / (self.tick_s / crate::spectral::TICK as f64);
            let dt = (t - self.t_open + self.snap_dt) as f64 * crate::spectral::TICK as f64;
            let mut lvmax: f64 = 0.0;
            for b in 0..n {
                lvmax = lvmax.max(re[b] * re[b] + im[b] * im[b]);
            }
            let lfloor = lvmax.sqrt() * 10f64.powf(-p.depth_db / 20.0);
            let close = t - self.t_open >= self.ev_len;
            let mut nw_mask = 0u128;
            for b in 0..n {
                let (z0r, z0i) = (self.snap_r[b], self.snap_i[b]);
                let w = TAU * self.snap_f[b] * dt / fb;
                let (s, c) = w.sin_cos();
                let (pr, pi) = (z0r * c - z0i * s, z0r * s + z0i * c);
                let (dr, di) = (re[b] - pr, im[b] - pi);
                let d = (dr * dr + di * di).sqrt();
                let h = (z0r * z0r + z0i * z0i).sqrt();
                let lv = (re[b] * re[b] + im[b] * im[b]).sqrt();
                let audible = lv > lfloor;
                let dist = d > 0.25 * h && audible;
                if dist && !self.touched[b] {
                    self.age[b] = self.age[b].min((t - self.t_open) as u32);
                }
                self.touched[b] |= dist;
                if close && d > p.nov_thr * h && audible && self.cand[b] {
                    nw_mask |= bit(b);
                }
            }
            if close {
                let e = self.e_open;
                self.struck[(e as usize) % EVENTS] = nw_mask;
                for b in 0..n {
                    if nw_mask & bit(b) != 0 && !self.conf[b] && t.saturating_sub(self.lastc[b]) > self.hold_t {
                        self.birth[b] = e;
                    }
                }
                self.e_open = -1;
            }
        }
        // 7. the status travels with the partial
        for b in 0..n {
            self.inherit[b] = false;
            self.got[b] = false;
        }
        if p.same_pick {
            for b in 0..n {
                if !self.cand[b] || self.conf[b] {
                    continue;
                }
                for d in [b as i64, b as i64 - 1, b as i64 + 1] {
                    if d < 0 || d >= n as i64 {
                        continue;
                    }
                    let d = d as usize;
                    let since = t.saturating_sub(self.lastc[d]);
                    if !self.conf[d] && self.lastc[d] > 0 && since <= self.hold_t && self.lastf[d] > 0.0 && Self::cents(clean.fhat[b], self.lastf[d]).abs() < 50.0 {
                        self.birth[b] = self.lastb[d];
                        self.age[b] = self.age[b].max(self.lasta[d]);
                        self.inherit[b] = since <= self.gmax as u64;
                        self.got[b] = true;
                        break;
                    }
                }
            }
            // a band that enters the selection within 150 ms of a pick was struck by it
            if self.ne > 0 && t - self.last_on <= self.late_t {
                let e = self.ne - 1;
                for b in 0..n {
                    let fresh = self.cand[b] && !self.prev_c[b] && !self.got[b] && !self.conf[b] && (self.lastc[b] == 0 || t - self.lastc[b] > self.hold_t);
                    if fresh {
                        self.struck[(e as usize) % EVENTS] |= bit(b);
                        self.birth[b] = e;
                    }
                }
            }
        }
        self.prev_c.copy_from_slice(&self.cand);
        // 2 – 3. the overtone tests, exact and widened
        self.pk.clear();
        for b in 0..n {
            if self.cand[b] {
                self.pk.push(b);
            }
        }
        for b in 0..n {
            self.fexact[b] = true;
            self.fwide[b] = true;
            self.low[b] = false;
            self.ex[b] = self.settle(b);
        }
        if let Some(&first) = self.pk.first() {
            self.low[first] = true;
        }
        let kmax = p.kmax.min(MAX_K);
        for i in 1..self.pk.len() {
            let bi = self.pk[i];
            let fi = clean.fhat[bi];
            let li = clean.ldb[bi];
            let mut any_lev = false;
            for wide in [false, true] {
                let mut overtone = false;
                for j in 0..i {
                    let bj = self.pk[j];
                    if clean.ldb[bj] <= li - p.parent_db {
                        continue;
                    }
                    any_lev = true;
                    let tx = if wide { self.ex[bj] + self.ex[bi] } else { 0.0 };
                    let l = 1200.0 * fast_log2(fi / clean.fhat[bj]);
                    // k runs from where the stretched window could reach down to where it cannot
                    let k_lo = (fast_exp2_c((l - p.tol_c - tx - self.st_k[kmax]) / 1200.0).floor() as usize).max(2);
                    let mut k = k_lo;
                    while k <= kmax {
                        let c = l - self.log_k[k];
                        if c <= -p.tol_c - tx {
                            break;
                        }
                        if c < p.tol_c + tx + self.st_k[k] {
                            let bi_birth = self.birth[bi];
                            if !p.same_pick || bi_birth < 0 || self.excited(clean, bj, k, bi_birth, tx) {
                                overtone = true;
                                break;
                            }
                        }
                        k += 1;
                    }
                    if overtone {
                        break;
                    }
                }
                if overtone {
                    if wide {
                        self.fwide[bi] = false;
                    } else {
                        self.fexact[bi] = false;
                    }
                }
            }
            if !any_lev {
                self.low[bi] = true;
            }
        }
        // 4 – 6, 8. confirmation, outputs
        let mut roots = 0;
        for b in 0..n {
            let c = self.cand[b];
            let fx = self.fexact[b] && c && clean.pur[b] >= p.pure_min;
            let fw = self.fwide[b] && c;
            self.run[b] = if fx { self.run[b] + 1 } else { 0 };
            self.runl[b] = if self.low[b] && c { self.runl[b] + 1 } else { 0 };
            let mut ok = (self.run[b] >= self.need[b] && fw) || (self.conf[b] && fx) || (self.inherit[b] && fx);
            if p.lowest_fast {
                let since = t.saturating_sub(self.last_on);
                ok |= self.runl[b] >= self.need[b] / 2 && self.ne > 0 && since as f64 >= 3.0 * self.tau_t.iter().cloned().fold(0.0, f64::max);
            }
            self.conf[b] = ok;
            if ok {
                self.lastc[b] = t;
                self.lastf[b] = clean.fhat[b];
                self.lastb[b] = self.birth[b];
                self.lasta[b] = self.age[b];
                roots += 1;
            }
            let und = c && !ok && self.age[b] < self.lim[b];
            let r = if ok && self.keep[b] { 1.0 } else { 0.0 };
            self.f_out[b] += (r - self.f_out[b]) * self.a_f;
            self.u_out[b] += ((und as u8 as f64) - self.u_out[b]) * self.a_u;
        }
        self.stats = SieveStats { roots, candidates: ncand };
    }
}

/// 2^y (control rate, not on the audio path).
#[inline]
fn fast_exp2_c(y: f64) -> f64 {
    y.clamp(-60.0, 60.0).exp2()
}
