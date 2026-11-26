// SPDX-License-Identifier: GPL-3.0-or-later
//! The organ law (README §2.2) as the Spectral core's shaper, with the percussion and the key click's
//! contact bounce (§2.4), at the bank rate.
//!
//! Per band b, with u_b = e^{jθ_b} (the core's turned phasor), its gain (hold, skirt, selection,
//! floor, handover: the core's), F_b (root) and U_b (undecided) from the sieve:
//!
//! y_b = amp_b · [ F_b · Σ_f d_f w_f Re(u_b^{r_f} ρ_f) + U_b d_max Re(u_b) + (1 − F_b − U_b) σ Re(u_b) ]
//!       + amp_b (F_b + U_b) p(t) Re(u_b^h)
//!
//! - r_f = ½, 3⁄2, 1, 2, 3, 4, 5, 6, 8 (16' … 1'); u^½ by the core's branch tracker, u^{3/2} = u^½ u.
//! - ρ_f the tonewheel rotors (`tuning` Tonewheel): 5⅓' runs −1.36 ¢ (ρ_a), 2⅔' ρ_a², 1⅓' ρ_a⁴, 1⅗'
//!   +14.1 ¢ (ρ_b), each advanced per sample by its offset × the band's f̂ (polysynth's detune rotor).
//! - Foldback: a footage under 65.4 Hz plays at 2 r_f, one over 5.9 kHz at r_f / 2 (dropped when a
//!   lower footage already plays there), each crossfaded over ±50 ¢; then the fcap fade (0.42 × the bank
//!   rate, 500 Hz). Folded components play without their tuning rotor.
//! - Fast-bank bands read F and U interpolated from the clean bands at their centre (the core's map).
//! - p(t): the percussion envelope (§2.4), restarted by a pick (or, `Single`, only after silence).
//! - The click's contact bounce gates each footage's slot for the first 3 – 4 ms after a pick.

use gfx_dsp::filterbank::BandSpec;
use gfx_dsp::spectral::{self as sp, Analysis, Shaper};
use std::f64::consts::TAU;

/// The nine footages' ratios to the played note (16' … 1').
pub const RATIOS: [f64; 9] = [0.5, 1.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0];
/// The power slots: u^½, u, u^{3/2}, u², u^{5/2}, u³, u⁴, u⁵, u⁶, u⁸.
const SLOT_R: [f64; NS] = [0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 8.0];
const NS: usize = 10;
/// Each footage's own slot.
const FOOT_SLOT: [usize; 9] = [0, 2, 1, 3, 5, 6, 7, 8, 9];
/// Tonewheel offsets (Electric Druid's gear ratios): 5⅓' 1.49882 (and 2⅔', 1⅓' as its powers), 1⅗'
/// 5.04094 (+14.1 ¢: a tempered third).
const GEAR_FIFTH: f64 = 1.49882;
const GEAR_THIRD: f64 = 5.04094;
/// Foldback bounds (Hz): the manual's lowest wheel, the top wheel.
pub const FOLD_LO: f64 = 65.406;
pub const FOLD_HI: f64 = 5900.0;
/// The click's bounce table: per footage (close time, then bounces as (start, length)), ms; fixed
/// seed (setBfree's model: contacts closing at slightly different moments, 1 – 3 bounces of 0.2 –
/// 0.6 ms).
const BOUNCE: [(f64, [(f64, f64); 3]); 9] = [
    (0.42, [(0.71, 0.33), (1.52, 0.21), (0.0, 0.0)]),
    (1.37, [(1.66, 0.48), (0.0, 0.0), (0.0, 0.0)]),
    (0.18, [(0.47, 0.26), (1.05, 0.55), (2.11, 0.24)]),
    (2.06, [(2.39, 0.37), (3.02, 0.22), (0.0, 0.0)]),
    (0.93, [(1.18, 0.59), (0.0, 0.0), (0.0, 0.0)]),
    (2.71, [(2.95, 0.31), (3.47, 0.28), (0.0, 0.0)]),
    (1.64, [(1.92, 0.44), (2.63, 0.20), (3.10, 0.35)]),
    (0.61, [(0.88, 0.52), (0.0, 0.0), (0.0, 0.0)]),
    (2.98, [(3.21, 0.27), (0.0, 0.0), (0.0, 0.0)]),
];
/// Bounce window length (ms): every gate is closed by then.
const BOUNCE_END_MS: f64 = 4.0;

/// What the processor sets on the law at each control tick.
#[derive(Clone, Copy, Debug)]
pub struct LawSettings {
    /// Drawbar gains (linear, after the 3 dB law, the percussion's 1' mute and volume drop).
    pub d: [f64; 9],
    /// `strings` σ (0 – 1).
    pub strings: f64,
    /// Percussion on, its harmonic (2 or 3), level (linear), τ (s), single trigger.
    pub perc: bool,
    pub perc_h: u32,
    pub perc_gain: f64,
    pub perc_tau: f64,
    pub perc_single: bool,
    /// `tuning` Tonewheel.
    pub tonewheel: bool,
    /// `click` 0 – 1 (the bounce depth).
    pub click: f64,
}

impl Default for LawSettings {
    fn default() -> Self {
        Self { d: [1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], strings: 0.0, perc: false, perc_h: 3, perc_gain: 1.0, perc_tau: 0.145, perc_single: false, tonewheel: true, click: 0.3 }
    }
}

/// Per-band state and per-tick weights of one bank.
#[derive(Clone, Debug, Default)]
struct LawBands {
    /// √ branch
    s1r: Vec<f64>,
    s1i: Vec<f64>,
    /// tonewheel rotors (fifth, third) and their steps
    ar: Vec<f64>,
    ai: Vec<f64>,
    asr: Vec<f64>,
    asi: Vec<f64>,
    br: Vec<f64>,
    bi: Vec<f64>,
    bsr: Vec<f64>,
    bsi: Vec<f64>,
    /// per-tick weights: plain slots [b * NS + s], the four rotor terms [b * 4 + i], the undecided
    /// sine, the overtone sine, the percussion weight; and whether the band carries a registration
    plain: Vec<f64>,
    rot: Vec<f64>,
    und: Vec<f64>,
    ov: Vec<f64>,
    perc: Vec<f64>,
    full: Vec<bool>,
}

impl LawBands {
    fn prepare(&mut self, n: usize) {
        for v in [&mut self.s1r, &mut self.s1i, &mut self.ar, &mut self.ai, &mut self.asr, &mut self.asi, &mut self.br, &mut self.bi, &mut self.bsr, &mut self.bsi, &mut self.und, &mut self.ov, &mut self.perc] {
            *v = vec![0.0; n];
        }
        self.plain = vec![0.0; n * NS];
        self.rot = vec![0.0; n * 4];
        self.full = vec![false; n];
    }

    fn reset(&mut self) {
        for v in [&mut self.s1i, &mut self.ai, &mut self.asi, &mut self.bi, &mut self.bsi, &mut self.und, &mut self.ov, &mut self.perc, &mut self.plain, &mut self.rot] {
            v.iter_mut().for_each(|x| *x = 0.0);
        }
        for v in [&mut self.s1r, &mut self.ar, &mut self.asr, &mut self.br, &mut self.bsr] {
            v.iter_mut().for_each(|x| *x = 1.0);
        }
        self.full.iter_mut().for_each(|x| *x = false);
    }
}

/// The organ shaper.
#[derive(Clone, Debug, Default)]
pub struct OrganLaw {
    pub s: LawSettings,
    fb: f64,
    fcap: f64,
    bands: [LawBands; 2],
    /// drawbars now (10 ms linear towards `s.d`)
    d: [f64; 9],
    d_step: f64,
    /// percussion envelope (per sample decay), its armed state for Single
    pe: f64,
    pe_k: f64,
    perc_armed: bool,
    silent_ticks: u32,
    /// samples since the last pick (bounce), and whether the bounce is running
    since_pick: u64,
    bounce_n: u64,
    bounce_ms_per: f64,
    gates: [f64; NS],
    // meters and the touch normaliser
    roots: usize,
    undecided: usize,
    p_max: f64,
    loudest_f: f64,
    /// 0 silence · 1 attack (undecided partials only) · 2 organ (roots sounding)
    state: u8,
}

impl OrganLaw {
    /// Confirmed roots now (the `roots` meter).
    pub fn roots(&self) -> usize {
        self.roots
    }

    /// 0 silence · 1 attack · 2 organ.
    pub fn state(&self) -> u8 {
        self.state
    }

    /// The loudest candidate's body (amplitude, skirt taken out): `touch` normalises to it.
    pub fn p_max(&self) -> f64 {
        self.p_max
    }

    /// f̂ of the loudest root or undecided band (Hz, 0 = none): the click's tail.
    pub fn loudest_hz(&self) -> f64 {
        self.loudest_f
    }

    /// The drawbars now (linear).
    pub fn drawbars(&self) -> [f64; 9] {
        self.d
    }

    #[inline]
    fn fade(&self, f: f64) -> f64 {
        ((self.fcap - f) * (1.0 / sp::FADE_HZ)).clamp(0.0, 1.0)
    }

    /// One band's per-tick weights.
    #[allow(clippy::too_many_arguments)]
    fn weights(&self, lb: &mut LawBands, b: usize, f: f64, froot: f64, fund: f64, dmax: f64, sigma: f64, pg: f64, ph: u32, fb: f64) {
        let base = b * NS;
        lb.plain[base..base + NS].iter_mut().for_each(|x| *x = 0.0);
        let rb = b * 4;
        lb.rot[rb..rb + 4].iter_mut().for_each(|x| *x = 0.0);
        lb.full[b] = froot > 1e-6;
        if lb.full[b] {
            for (i, &r) in RATIOS.iter().enumerate() {
                let d = self.d[i] * froot;
                if d <= 0.0 {
                    continue;
                }
                let fr = r * f;
                let gl = ((1200.0 * (fr / FOLD_LO).log2() + 50.0) / 100.0).clamp(0.0, 1.0);
                let gh = ((1200.0 * (fr / FOLD_HI).log2() + 50.0) / 100.0).clamp(0.0, 1.0);
                // the normal component
                let g_norm = d * gl * (1.0 - gh) * self.fade(fr);
                let rot_i = match i {
                    1 => Some(0),
                    4 => Some(1),
                    7 => Some(2),
                    6 => Some(3),
                    _ => None,
                };
                match rot_i {
                    Some(k) => lb.rot[rb + k] += g_norm,
                    None => lb.plain[base + FOOT_SLOT[i]] += g_norm,
                }
                // folded up (under 65.4 Hz)
                if gl < 1.0 {
                    if let Some(s) = SLOT_R.iter().position(|&x| (x - 2.0 * r).abs() < 1e-9) {
                        lb.plain[base + s] += d * (1.0 - gl) * self.fade(2.0 * fr);
                    } else {
                        lb.plain[base + FOOT_SLOT[i]] += d * (1.0 - gl) * self.fade(fr);
                    }
                }
                // folded down (over 5.9 kHz), dropped where a lower footage already plays
                if gh > 0.0 {
                    let half = 0.5 * r;
                    let taken = RATIOS.iter().enumerate().any(|(j, &q)| (q - half).abs() < 1e-9 && self.d[j] > 0.0);
                    if !taken {
                        if let Some(s) = SLOT_R.iter().position(|&x| (x - half).abs() < 1e-9) {
                            lb.plain[base + s] += d * gh * self.fade(0.5 * fr);
                        }
                    }
                }
            }
        }
        lb.und[b] = fund * dmax * self.fade(f);
        lb.ov[b] = (1.0 - froot - fund).max(0.0) * sigma * self.fade(f);
        lb.perc[b] = if pg > 0.0 { (froot + fund) * pg * self.fade(ph as f64 * f) } else { 0.0 };
        // the tonewheel rotors' steps
        if self.s.tonewheel && lb.full[b] {
            let wa = TAU * (GEAR_FIFTH - 1.5) * f / fb;
            let wb = TAU * (GEAR_THIRD - 5.0) * f / fb;
            lb.asr[b] = wa.cos();
            lb.asi[b] = wa.sin();
            lb.bsr[b] = wb.cos();
            lb.bsi[b] = wb.sin();
            let m = (lb.ar[b] * lb.ar[b] + lb.ai[b] * lb.ai[b]).sqrt();
            if m > 1e-12 {
                lb.ar[b] /= m;
                lb.ai[b] /= m;
            } else {
                lb.ar[b] = 1.0;
                lb.ai[b] = 0.0;
            }
            let m = (lb.br[b] * lb.br[b] + lb.bi[b] * lb.bi[b]).sqrt();
            if m > 1e-12 {
                lb.br[b] /= m;
                lb.bi[b] /= m;
            } else {
                lb.br[b] = 1.0;
                lb.bi[b] = 0.0;
            }
        } else {
            lb.ar[b] = 1.0;
            lb.ai[b] = 0.0;
            lb.br[b] = 1.0;
            lb.bi[b] = 0.0;
            lb.asr[b] = 1.0;
            lb.asi[b] = 0.0;
            lb.bsr[b] = 1.0;
            lb.bsi[b] = 0.0;
        }
    }

    /// The bounce gates at `ms` after the pick (1 = closed contact, sounding).
    fn bounce_gates(&mut self, ms: f64) {
        let depth = self.s.click.clamp(0.0, 1.0);
        let mut g = [1.0f64; 9];
        for (i, (close, bounces)) in BOUNCE.iter().enumerate() {
            let mut open = ms < *close;
            for &(st, len) in bounces.iter() {
                if len > 0.0 && ms >= st && ms < st + len {
                    open = true;
                }
            }
            g[i] = if open { 1.0 - depth } else { 1.0 };
        }
        for (i, &s) in FOOT_SLOT.iter().enumerate() {
            self.gates[s] = g[i];
        }
        // u^{5/2} (1⅗' folded) takes the 1⅗' gate
        self.gates[4] = g[6];
    }
}

impl Shaper for OrganLaw {
    const SIEVE: bool = true;

    /// The bounce gates, for 4 ms after a pick.
    #[inline(always)]
    fn sample(&mut self) {
        self.since_pick = self.since_pick.saturating_add(1);
        if self.since_pick < self.bounce_n && self.s.click > 0.0 {
            let ms = self.since_pick as f64 * self.bounce_ms_per;
            self.bounce_gates(ms);
        } else if self.gates != [1.0; NS] {
            self.gates = [1.0; NS];
        }
    }

    fn prepare(&mut self, clean: &[BandSpec], fast: &[BandSpec], fb: f64) {
        self.fb = fb;
        self.fcap = sp::FCAP * fb;
        self.bands[0].prepare(clean.len());
        self.bands[1].prepare(fast.len());
        self.d_step = sp::TICK as f64 / (0.010 * fb);
        self.bounce_n = (BOUNCE_END_MS * 1e-3 * fb) as u64;
        self.bounce_ms_per = 1e3 / fb;
    }

    fn reset(&mut self) {
        self.bands[0].reset();
        self.bands[1].reset();
        self.d = self.s.d;
        self.pe = 0.0;
        self.pe_k = (-1.0 / (self.s.perc_tau.max(1e-3) * self.fb.max(1.0))).exp();
        self.perc_armed = true;
        self.silent_ticks = 0;
        self.since_pick = u64::MAX / 2;
        self.gates = [1.0; NS];
        self.roots = 0;
        self.undecided = 0;
        self.p_max = 0.0;
        self.loudest_f = 0.0;
        self.state = 0;
    }

    fn tick(&mut self, a: &Analysis) {
        let s = self.s;
        // drawbars: 10 ms linear
        for i in 0..9 {
            let t = s.d[i];
            let c = self.d[i];
            self.d[i] = if (t - c).abs() <= self.d_step { t } else { c + self.d_step * (t - c).signum() };
        }
        let dmax = self.d.iter().cloned().fold(0.0, f64::max);
        // percussion: restarted by a pick (Single: only after every note has stopped)
        self.pe_k = (-1.0 / (s.perc_tau.max(1e-3) * self.fb)).exp();
        let sieve = &a.sieve;
        let n = a.clean.n;
        let mut cands = 0;
        let mut roots = 0;
        let mut und = 0;
        let mut p_max: f64 = 0.0;
        let mut loud = (0.0f64, 0.0f64);
        for b in 0..n {
            if sieve.candidate(b) {
                cands += 1;
                let body = a.clean.peak[b] * a.clean.skirt[b];
                p_max = p_max.max(body);
                if (sieve.holds_root(b) || sieve.undecided(b) > 0.5) && body > loud.0 {
                    loud = (body, a.clean.fhat[b]);
                }
            }
            if sieve.holds_root(b) && a.clean.sel[b] > 0.5 {
                roots += 1;
            }
            if sieve.undecided(b) > 0.5 {
                und += 1;
            }
        }
        self.silent_ticks = if cands == 0 { self.silent_ticks + 1 } else { 0 };
        if self.silent_ticks as f64 * a.k.tick_s > 0.020 {
            self.perc_armed = true;
        }
        if a.picked {
            self.since_pick = 0;
            if !s.perc_single || self.perc_armed {
                self.pe = 1.0;
                self.perc_armed = false;
            }
        }
        if !s.perc {
            self.pe = 0.0;
        }
        self.roots = roots;
        self.undecided = und;
        self.p_max = p_max;
        self.loudest_f = loud.1;
        self.state = if roots > 0 { 2 } else if und > 0 || cands > 0 { 1 } else { 0 };
        let pg = if s.perc { s.perc_gain * self.pe } else { 0.0 };
        let ph = s.perc_h;
        let fb = self.fb;
        let sigma = s.strings;
        // clean bands from the sieve, fast bands interpolated at their centre
        let mut lb0 = std::mem::take(&mut self.bands[0]);
        for b in 0..n {
            self.weights(&mut lb0, b, a.clean.fhat[b], sieve.root(b), sieve.undecided(b), dmax, sigma, pg, ph, fb);
        }
        self.bands[0] = lb0;
        let mut lb1 = std::mem::take(&mut self.bands[1]);
        for j in 0..a.fast.n {
            let (i, fr) = a.interp[j];
            let i1 = (i + 1).min(n - 1);
            let fro = sieve.root(i) * (1.0 - fr) + sieve.root(i1) * fr;
            let fu = sieve.undecided(i) * (1.0 - fr) + sieve.undecided(i1) * fr;
            self.weights(&mut lb1, j, a.fast.fhat[j], fro, fu, dmax, sigma, pg, ph, fb);
        }
        self.bands[1] = lb1;
        // the percussion's decay across the coming tick
        if s.perc {
            self.pe *= self.pe_k.powi(sp::TICK as i32);
        }
    }

    #[inline(always)]
    fn band(&mut self, bank: usize, b: usize, ur: f64, ui: f64, m: f64, kk: f64, mk: f64, _f: f64, _pur: f64, out: &mut (f64, f64)) {
        let gates = self.gates;
        let lb = &mut self.bands[bank];
        let amp = m * kk;
        let mut y = lb.ov[b] * ur + lb.und[b] * gates[1] * ur;
        let need_perc = lb.perc[b] != 0.0;
        if lb.full[b] || need_perc {
            let (u2r, u2i) = (ur * ur - ui * ui, 2.0 * ur * ui);
            let (u3r, u3i) = (u2r * ur - u2i * ui, u2r * ui + u2i * ur);
            if need_perc {
                y += lb.perc[b] * if self.s.perc_h == 2 { u2r * gates[3] } else { u3r * gates[5] };
            }
            if lb.full[b] {
                let (hr, hi) = sp::sqrt_branch(ur, ui, &mut lb.s1r[b], &mut lb.s1i[b]);
                let (u4r, u4i) = (u2r * u2r - u2i * u2i, 2.0 * u2r * u2i);
                let (u15r, u15i) = (hr * ur - hi * ui, hr * ui + hi * ur);
                let (u25r, _u25i) = (u2r * hr - u2i * hi, u2r * hi + u2i * hr);
                let (u5r, u5i) = (u4r * ur - u4i * ui, u4r * ui + u4i * ur);
                let (u6r, u6i) = (u3r * u3r - u3i * u3i, 2.0 * u3r * u3i);
                let u8r = u4r * u4r - u4i * u4i;
                let p = &lb.plain[b * NS..b * NS + NS];
                y += gates[0] * p[0] * hr + gates[1] * p[1] * ur + gates[2] * p[2] * u15r + gates[3] * p[3] * u2r + gates[4] * p[4] * u25r + gates[5] * p[5] * u3r + gates[6] * p[6] * u4r + gates[7] * p[7] * u5r + gates[8] * p[8] * u6r + gates[9] * p[9] * u8r;
                // the tonewheel terms: 5⅓' ρ_a, 2⅔' ρ_a², 1⅓' ρ_a⁴, 1⅗' ρ_b
                let rw = &lb.rot[b * 4..b * 4 + 4];
                let (ar, ai) = (lb.ar[b], lb.ai[b]);
                let (a2r, a2i) = (ar * ar - ai * ai, 2.0 * ar * ai);
                let (a4r, a4i) = (a2r * a2r - a2i * a2i, 2.0 * a2r * a2i);
                let (br, bi) = (lb.br[b], lb.bi[b]);
                y += gates[2] * rw[0] * (u15r * ar - u15i * ai) + gates[5] * rw[1] * (u3r * a2r - u3i * a2i) + gates[8] * rw[2] * (u6r * a4r - u6i * a4i) + gates[7] * rw[3] * (u5r * br - u5i * bi);
                // advance the rotors
                let (sr, si) = (lb.asr[b], lb.asi[b]);
                lb.ar[b] = ar * sr - ai * si;
                lb.ai[b] = ar * si + ai * sr;
                let (sr, si) = (lb.bsr[b], lb.bsi[b]);
                lb.br[b] = br * sr - bi * si;
                lb.bi[b] = br * si + bi * sr;
            }
        }
        out.0 += amp * y * mk;
    }
}
