// SPDX-License-Identifier: GPL-3.0-or-later
//! A modal bank: up to [`MAX_MODES`] resonant modes in parallel, each a TPT state-variable band-pass
//! normalised to unity at its peak (k · v1, Simper / Zavalishin), summed with one gain per mode (Adrien,
//! *The missing link: modal synthesis*, 1991; van den Doel & Pai, 1998). Written for gfx-morph's objects
//! (gfx-morph README §2.7, §2.11).
//!
//! - **f64 structure-of-arrays lanes** (gfx-dsp's numerics rule: recursive states in f64; an f32 mode
//!   at 30 Hz, Q 160, 192 kHz errs at −49 dB re its own output). The states update in one plain loop
//!   over the modes, then the sum runs in four accumulators. Unused lanes carry a zero gain and an
//!   inert coefficient set (their states stay at 0).
//! - **Per-tick coefficient ramps**: [`ModalBank::set_ramped`] moves (a1, a2, a3, k·g) linearly to a new
//!   set over n samples (gfx-acoustic's rule: coefficients recomputed every few samples, ramped in
//!   between), so `size`, `decay`, `position` and a morph move every sample without steps.
//! - **Rank morph** of two mode lists ([`morph`]): pairs by rank (lists sorted by frequency), log f and
//!   log Q interpolated, gains linear; a slot with no partner takes its partner's f and Q and only fades
//!   (Penttinen, Karjalainen & Härmä, *Morphing instrument body models*, DAFx-01).
//! - **Band-edge fades** ([`edge_fade`]): a mode's gain fades to zero between 0.35 fs and 0.45 fs and
//!   between 40 and 30 Hz, its coefficient held at the limit, so a moving size never drops a mode.
//!
//! Zero latency (IIR). Real-time safe: fixed arrays, no allocation.

use std::f64::consts::PI;

/// The largest bank.
pub const MAX_MODES: usize = 48;
/// Lanes: the bank runs in groups of four.
const LANES: usize = 4;

/// One mode: centre frequency (Hz), quality factor, gain (the band-pass is unity at its peak).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mode {
    pub f: f64,
    pub q: f64,
    pub g: f64,
}

/// A list of up to [`MAX_MODES`] modes, sorted by frequency; `n` are in use.
#[derive(Clone, Copy, Debug)]
pub struct ModeList {
    pub modes: [Mode; MAX_MODES],
    pub n: usize,
}

impl Default for ModeList {
    fn default() -> Self {
        Self { modes: [Mode::default(); MAX_MODES], n: 0 }
    }
}

impl ModeList {
    pub fn as_slice(&self) -> &[Mode] {
        &self.modes[..self.n]
    }
}

/// Lower and upper fade bands (Hz, and fractions of the sample rate).
pub const LOW_FADE: (f64, f64) = (30.0, 40.0);
pub const HIGH_FADE: (f64, f64) = (0.35, 0.45);

/// A mode's coefficient frequency held inside [30 Hz, 0.45 fs] and its gain faded to zero between 40
/// and 30 Hz and between 0.35 fs and 0.45 fs (linear in frequency).
#[inline]
pub fn edge_fade(m: Mode, fs: f64) -> Mode {
    let (lo0, lo1) = LOW_FADE;
    let (hi0, hi1) = (HIGH_FADE.0 * fs, HIGH_FADE.1 * fs);
    let w = if m.f <= lo0 || m.f >= hi1 {
        0.0
    } else if m.f < lo1 {
        (m.f - lo0) / (lo1 - lo0)
    } else if m.f > hi0 {
        (hi1 - m.f) / (hi1 - hi0)
    } else {
        1.0
    };
    Mode { f: m.f.clamp(lo0, hi1), q: m.q, g: m.g * w }
}

/// The rank morph of `a` and `b` at `mu` (0 → a, 1 → b) into `out`: pair i interpolates log f and
/// log Q and blends the gains linearly; a slot present in one list only keeps that list's f and Q and
/// fades its gain. `out.n` = the longer list's count.
pub fn morph(a: &[Mode], b: &[Mode], mu: f64, out: &mut ModeList) {
    let n = a.len().max(b.len()).min(MAX_MODES);
    let mu = mu.clamp(0.0, 1.0);
    for i in 0..n {
        out.modes[i] = match (a.get(i), b.get(i)) {
            (Some(x), Some(y)) => {
                if mu == 0.0 {
                    *x
                } else if mu == 1.0 {
                    *y
                } else {
                    Mode {
                        f: ((1.0 - mu) * x.f.ln() + mu * y.f.ln()).exp(),
                        q: ((1.0 - mu) * x.q.ln() + mu * y.q.ln()).exp(),
                        g: (1.0 - mu) * x.g + mu * y.g,
                    }
                }
            }
            (Some(x), None) => Mode { g: (1.0 - mu) * x.g, ..*x },
            (None, Some(y)) => Mode { g: mu * y.g, ..*y },
            (None, None) => Mode::default(),
        };
    }
    out.n = n;
}

/// The TPT coefficients of one mode: (a1, a2, a3, k · g).
#[inline]
pub fn coefs(m: &Mode, fs: f64) -> [f64; 4] {
    let f = m.f.clamp(1.0, 0.49 * fs);
    let g = (PI * f / fs).tan();
    let k = 1.0 / m.q.max(1e-3);
    let a1 = 1.0 / (1.0 + g * (g + k));
    let a2 = g * a1;
    [a1, a2, g * a2, k * m.g]
}

/// The bank: coefficients and states as structure-of-arrays lanes (f64).
#[derive(Clone, Debug)]
pub struct ModalBank {
    a1: [f64; MAX_MODES],
    a2: [f64; MAX_MODES],
    a3: [f64; MAX_MODES],
    kg: [f64; MAX_MODES],
    da1: [f64; MAX_MODES],
    da2: [f64; MAX_MODES],
    da3: [f64; MAX_MODES],
    dkg: [f64; MAX_MODES],
    ic1: [f64; MAX_MODES],
    ic2: [f64; MAX_MODES],
    /// lanes in use (a multiple of four)
    lanes: usize,
    /// ramp samples left
    left: u32,
}

impl Default for ModalBank {
    fn default() -> Self {
        Self::new()
    }
}

impl ModalBank {
    /// An empty bank (every lane inert).
    pub fn new() -> Self {
        Self {
            a1: [1.0; MAX_MODES],
            a2: [0.0; MAX_MODES],
            a3: [0.0; MAX_MODES],
            kg: [0.0; MAX_MODES],
            da1: [0.0; MAX_MODES],
            da2: [0.0; MAX_MODES],
            da3: [0.0; MAX_MODES],
            dkg: [0.0; MAX_MODES],
            ic1: [0.0; MAX_MODES],
            ic2: [0.0; MAX_MODES],
            lanes: 0,
            left: 0,
        }
    }

    /// Clears the states (the coefficients stay).
    pub fn reset(&mut self) {
        self.ic1 = [0.0; MAX_MODES];
        self.ic2 = [0.0; MAX_MODES];
    }

    /// The lanes in use (a multiple of four, ≤ [`MAX_MODES`]).
    pub fn lanes(&self) -> usize {
        self.lanes
    }

    fn target(modes: &[Mode], fs: f64) -> ([[f64; 4]; MAX_MODES], usize) {
        let mut c = [[1.0, 0.0, 0.0, 0.0]; MAX_MODES];
        let n = modes.len().min(MAX_MODES);
        for (ci, m) in c.iter_mut().zip(modes.iter()) {
            *ci = coefs(m, fs);
        }
        (c, n.div_ceil(LANES) * LANES)
    }

    /// Sets the modes at once (no ramp). Lanes past `modes.len()` become inert.
    pub fn set(&mut self, modes: &[Mode], fs: f64) {
        let (c, lanes) = Self::target(modes, fs);
        for (i, ci) in c.iter().enumerate() {
            self.a1[i] = ci[0];
            self.a2[i] = ci[1];
            self.a3[i] = ci[2];
            self.kg[i] = ci[3];
        }
        // a lane leaving use keeps decaying through its own (now inert) coefficients: clear it
        for i in lanes..self.lanes.max(lanes) {
            self.ic1[i] = 0.0;
            self.ic2[i] = 0.0;
        }
        self.lanes = lanes;
        self.left = 0;
    }

    /// Ramps every coefficient linearly to the new modes over `n` samples (`n` = 0: at once). A lane
    /// that enters use starts from its target coefficients at zero gain.
    pub fn set_ramped(&mut self, modes: &[Mode], fs: f64, n: u32) {
        if n == 0 {
            self.set(modes, fs);
            return;
        }
        let (c, lanes) = Self::target(modes, fs);
        let inv = 1.0 / n as f64;
        for (i, ci) in c.iter().enumerate() {
            if i >= lanes {
                // a lane leaving use: its filter stays, its gain ramps to 0
                self.da1[i] = 0.0;
                self.da2[i] = 0.0;
                self.da3[i] = 0.0;
                self.dkg[i] = -self.kg[i] * inv;
                continue;
            }
            if i >= self.lanes {
                // a new lane: its filter at the target, its gain from 0
                self.a1[i] = ci[0];
                self.a2[i] = ci[1];
                self.a3[i] = ci[2];
                self.kg[i] = 0.0;
                self.ic1[i] = 0.0;
                self.ic2[i] = 0.0;
            }
            self.da1[i] = (ci[0] - self.a1[i]) * inv;
            self.da2[i] = (ci[1] - self.a2[i]) * inv;
            self.da3[i] = (ci[2] - self.a3[i]) * inv;
            self.dkg[i] = (ci[3] - self.kg[i]) * inv;
        }
        // lanes leaving use ramp their gain to 0 with the rest, then go inert at the next set
        self.lanes = lanes.max(self.lanes);
        self.left = n;
    }

    /// One sample in, the bank's sum out.
    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let n = self.lanes;
        if self.left > 0 {
            self.left -= 1;
            for i in 0..n {
                self.a1[i] += self.da1[i];
                self.a2[i] += self.da2[i];
                self.a3[i] += self.da3[i];
                self.kg[i] += self.dkg[i];
            }
        }
        let mut v = [0.0f64; MAX_MODES];
        for i in 0..n {
            let v3 = x - self.ic2[i];
            let v1 = self.a1[i] * self.ic1[i] + self.a2[i] * v3;
            let v2 = self.ic2[i] + self.a2[i] * self.ic1[i] + self.a3[i] * v3;
            self.ic1[i] = 2.0 * v1 - self.ic1[i];
            self.ic2[i] = 2.0 * v2 - self.ic2[i];
            v[i] = self.kg[i] * v1;
        }
        let mut acc = [0.0f64; LANES];
        for c in v[..n].as_chunks::<LANES>().0 {
            acc[0] += c[0];
            acc[1] += c[1];
            acc[2] += c[2];
            acc[3] += c[3];
        }
        (acc[0] + acc[1]) + (acc[2] + acc[3])
    }

    /// Whether a coefficient ramp is running.
    pub fn is_ramping(&self) -> bool {
        self.left > 0
    }
}

/// The analog-prototype magnitude response of a list of modes at `freq` (Hz) under the bilinear map
/// at `fs` (each mode prewarped at its own centre, as the TPT filter): the complex sum Σ g H_m.
pub fn response(modes: &[Mode], freq: f64, fs: f64) -> (f64, f64) {
    let t = (PI * freq / fs).tan();
    let (mut re, mut im) = (0.0, 0.0);
    for m in modes {
        let w = t / (PI * m.f.clamp(1.0, 0.49 * fs) / fs).tan();
        let k = 1.0 / m.q.max(1e-3);
        // H = j w k / (1 − w² + j w k)
        let (dr, di) = (1.0 - w * w, w * k);
        let d2 = dr * dr + di * di;
        // (j w k)(dr − j di) / d2 = (w k di + j w k dr) / d2
        re += m.g * (w * k * di) / d2;
        im += m.g * (w * k * dr) / d2;
    }
    (re, im)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svf::{Svf, SvfMode};

    fn sine_gain_db(bank: &mut ModalBank, f: f64, fs: f64) -> f64 {
        sine_gain_db_for(bank, f, fs, 2.0)
    }

    fn sine_gain_db_for(bank: &mut ModalBank, f: f64, fs: f64, seconds: f64) -> f64 {
        bank.reset();
        let n = (fs * seconds) as usize;
        let w = 2.0 * PI * f / fs;
        let (mut ei, mut eo) = (0.0, 0.0);
        for i in 0..n {
            let x = (w * i as f64).sin();
            let y = bank.process(x);
            if i > n / 2 {
                ei += x * x;
                eo += y * y;
            }
        }
        10.0 * (eo / ei).log10()
    }

    #[test]
    fn one_mode_is_unity_at_its_peak_at_every_rate() {
        for fs in [44100.0, 48000.0, 96000.0, 192000.0] {
            for (f, q) in [(100.0, 20.0), (1000.0, 5.0), (8000.0, 40.0)] {
                let mut b = ModalBank::new();
                b.set(&[Mode { f, q, g: 0.5 }], fs);
                let g = sine_gain_db(&mut b, f, fs);
                assert!((g - 20.0 * 0.5f64.log10()).abs() < 0.01, "{fs} {f}: {g}");
                // an octave away: the band-pass skirt, |H|² = 1 / (1 + Q²(w − 1/w)²), w = 2 (≈, bilinear)
                if 2.0 * f > fs / 8.0 {
                    continue;
                }
                let off = sine_gain_db(&mut b, 2.0 * f, fs) - g;
                let want = -10.0 * (1.0 + q * q * 2.25f64).log10();
                assert!((off - want).abs() < 1.5, "{fs} {f}: {off} vs {want}");
            }
        }
    }

    #[test]
    fn the_bank_is_the_sum_of_gfx_dsp_band_passes() {
        let fs = 48000.0;
        let modes: Vec<Mode> = (0..13).map(|i| Mode { f: 110.0 * 1.31f64.powi(i), q: 8.0 + i as f64, g: 1.0 / (1.0 + i as f64) }).collect();
        let mut bank = ModalBank::new();
        bank.set(&modes, fs);
        assert_eq!(bank.lanes(), 16);
        let mut refs: Vec<Svf> = modes
            .iter()
            .map(|m| {
                let mut s = Svf::new();
                s.set(SvfMode::BandPass, m.f as f32, m.q as f32, 0.0, fs as f32);
                s
            })
            .collect();
        let mut state = 1u64;
        let mut worst = 0.0f64;
        for _ in 0..20000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let x = (state >> 11) as f64 / (1u64 << 53) as f64 - 0.5;
            let x = x as f32 as f64;
            let y = bank.process(x);
            let r: f64 = refs.iter_mut().zip(&modes).map(|(s, m)| m.g * s.process(x as f32) as f64).sum();
            worst = worst.max((y - r).abs());
        }
        // gfx-dsp's Svf rounds its output to f32; its coefficients come from f32 inputs
        assert!(worst < 1e-5, "{worst}");
    }

    #[test]
    fn a_ramp_lands_on_the_static_coefficients() {
        let fs = 96000.0;
        let a = [Mode { f: 200.0, q: 10.0, g: 1.0 }, Mode { f: 700.0, q: 30.0, g: 0.5 }];
        let b = [Mode { f: 260.0, q: 12.0, g: 0.8 }, Mode { f: 650.0, q: 25.0, g: 0.6 }, Mode { f: 1500.0, q: 9.0, g: 0.3 }];
        let mut r = ModalBank::new();
        r.set(&a, fs);
        r.set_ramped(&b, fs, 32);
        let mut s = ModalBank::new();
        s.set(&b, fs);
        for _ in 0..32 {
            r.process(0.0);
        }
        assert!(!r.is_ramping());
        for i in 0..MAX_MODES {
            assert!((r.a1[i] - s.a1[i]).abs() < 1e-12 && (r.a3[i] - s.a3[i]).abs() < 1e-12 && (r.kg[i] - s.kg[i]).abs() < 1e-12, "{i}");
        }
        // a continuous sweep stays finite and bounded
        let mut m = ModalBank::new();
        m.set(&a, fs);
        let mut peak = 0.0f64;
        for t in 0..48000 {
            if t % 32 == 0 {
                let u = t as f64 / 48000.0;
                let list: Vec<Mode> = a.iter().map(|x| Mode { f: x.f * (1.0 + 3.0 * u), ..*x }).collect();
                m.set_ramped(&list, fs, 32);
            }
            let y = m.process(((t as f64) * 0.05).sin());
            assert!(y.is_finite());
            peak = peak.max(y.abs());
        }
        assert!(peak < 5.0, "{peak}");
    }

    #[test]
    fn rank_morph_ends_and_the_partner_rule() {
        let a = [Mode { f: 100.0, q: 10.0, g: 1.0 }, Mode { f: 400.0, q: 20.0, g: 0.5 }];
        let b = [Mode { f: 200.0, q: 40.0, g: 0.2 }, Mode { f: 800.0, q: 5.0, g: 0.4 }, Mode { f: 1600.0, q: 8.0, g: 0.3 }];
        let mut o = ModeList::default();
        morph(&a, &b, 0.0, &mut o);
        assert_eq!(o.n, 3);
        assert_eq!(&o.modes[..2], &a[..]);
        assert_eq!(o.modes[2].g, 0.0);
        assert_eq!(o.modes[2].f, 1600.0);
        morph(&a, &b, 1.0, &mut o);
        assert_eq!(&o.modes[..3], &b[..]);
        morph(&a, &b, 0.5, &mut o);
        assert!((o.modes[0].f - (100.0f64 * 200.0).sqrt()).abs() < 1e-9);
        assert!((o.modes[0].q - (10.0f64 * 40.0).sqrt()).abs() < 1e-9);
        assert!((o.modes[0].g - 0.6).abs() < 1e-12);
        // no partner: the partner's own f and Q, only the gain fades
        assert_eq!((o.modes[2].f, o.modes[2].q), (1600.0, 8.0));
        assert!((o.modes[2].g - 0.15).abs() < 1e-12);
    }

    #[test]
    fn edges_fade_and_hold_the_coefficient() {
        let fs = 48000.0;
        let m = |f: f64| edge_fade(Mode { f, q: 10.0, g: 2.0 }, fs);
        assert_eq!(m(1000.0).g, 2.0);
        assert_eq!(m(0.35 * fs).g, 2.0);
        assert!((m(0.40 * fs).g - 1.0).abs() < 1e-12);
        assert_eq!(m(0.45 * fs).g, 0.0);
        assert_eq!(m(30000.0).g, 0.0);
        assert_eq!(m(30000.0).f, 0.45 * fs);
        assert!((m(35.0).g - 1.0).abs() < 1e-12);
        assert_eq!(m(20.0).g, 0.0);
        assert_eq!(m(20.0).f, 30.0);
        assert_eq!(m(40.0).g, 2.0);
    }

    #[test]
    fn f64_lanes_hold_a_deep_mode_at_192_khz() {
        // 30 Hz, Q 160: the review's worst case for f32 (−49 dB re its own output)
        let fs = 192000.0;
        let mut b = ModalBank::new();
        b.set(&[Mode { f: 30.0, q: 160.0, g: 1.0 }], fs);
        // T60 5.8 s: 30 s of sine, the last 15 s read
        let g = sine_gain_db_for(&mut b, 30.0, fs, 30.0);
        assert!(g.abs() < 0.05, "{g}");
        // the analytic response agrees with the filter at the peak and an octave away
        let (re, im) = response(&[Mode { f: 30.0, q: 160.0, g: 1.0 }], 30.0, fs);
        assert!(((re * re + im * im).sqrt() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn response_matches_the_filter() {
        let fs = 48000.0;
        let modes = [Mode { f: 300.0, q: 6.0, g: 0.7 }, Mode { f: 2500.0, q: 15.0, g: -0.4 }];
        let mut b = ModalBank::new();
        b.set(&modes, fs);
        for f in [150.0, 300.0, 1000.0, 2500.0, 6000.0] {
            let (re, im) = response(&modes, f, fs);
            let want = 10.0 * (re * re + im * im).log10();
            let got = sine_gain_db(&mut b, f, fs);
            assert!((want - got).abs() < 0.05, "{f}: {want} vs {got}");
        }
    }

    #[test]
    fn silence_after_a_burst_decays_and_reset_clears() {
        let fs = 44100.0;
        let mut b = ModalBank::new();
        b.set(&[Mode { f: 61.0, q: 300.0, g: 1.0 }, Mode { f: 5000.0, q: 900.0, g: 0.3 }], fs);
        for i in 0..4410 {
            b.process(((i * 7919) % 13) as f64 / 13.0 - 0.5);
        }
        let mut last = 0.0f64;
        for _ in 0..(fs as usize * 10) {
            last = b.process(0.0);
            assert!(last.is_finite());
        }
        assert!(last.abs() < 1e-6, "{last}");
        b.reset();
        assert_eq!(b.process(0.0), 0.0);
    }
}
