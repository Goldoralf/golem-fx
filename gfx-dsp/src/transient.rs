// SPDX-License-Identifier: GPL-3.0-or-later
//! The differential transient detector (gfx-transient §2.2 – 2.4, SPL's principle computed as dB
//! differences), and the lookahead ramp its shares go through (§2.6).
//!
//! Input: the peak level p, a sliding maximum over one period of the lowest note (the caller's
//! `SlidingMax`, window from [`crate::sidechain::window_secs`]). Then, all linear and in f64:
//! - floor F = max(`thresh`, P − 48 dB), P the phrase level of p; every envelope state is clamped to F;
//! - attack pair: E_f = max(p, F); E_s a one-pole **attack** toward E_f (`att_time`), instant release;
//!   D_a = 20 log10(E_f / E_s) ≥ 0;
//! - sustain pair: E_rf (instant attack, 10 ms release) and E_rs (instant attack, `sus_time` release)
//!   on p; D_s = 20 log10(E_rs / E_rf) ≥ 0;
//! - one law: share(D) = 1 − exp(−max(0, D − 1.5 dB) / D_ref), D_ref = 22 − 20 · `sens` dB.
//!
//! Both D are ratios: scaling the input scales every envelope and P alike, so the shares do not depend
//! on the level as long as it stays above `thresh`. The shares are 0 … 1; the caller multiplies them
//! by its gains in dB, after smoothing (the engine never handles a sign). The floor bound of a sustain
//! boost (§2.4) is the caller's, from [`Reading::height`] = E_rf / F.

use crate::envelope::SlidingMax;
use crate::sidechain::Phrase;
use crate::util::one_pole_coef;

/// The dead zone of the law (dB): a rise or fall of less is not shaped.
pub const DEAD_ZONE_DB: f64 = 1.5;
/// The floor sits this far under the phrase level (dB).
pub const FLOOR_WINDOW_DB: f64 = 48.0;
/// Release of the fast sustain envelope E_rf (ms).
pub const RF_RELEASE_MS: f64 = 10.0;

const DB_PER_NEPER: f64 = 8.685_889_638_065_037;

/// D_ref (dB) for a sensitivity of 0 … 1: 22 dB at 0, 12 dB at 0.5, 2 dB at 1.
#[inline]
pub fn d_ref(sens: f32) -> f64 {
    22.0 - 20.0 * sens.clamp(0.0, 1.0) as f64
}

/// The law: the share (0 … 1) of the gain given to a difference of `d_db` dB.
#[inline]
pub fn share(d_db: f64, d_ref: f64) -> f64 {
    let d = d_db - DEAD_ZONE_DB;
    if d <= 0.0 {
        0.0
    } else {
        1.0 - (-d / d_ref).exp()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub att_time_ms: f32,
    pub sus_time_ms: f32,
    /// 0 … 1
    pub sens: f32,
    pub thresh_db: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { att_time_ms: 15.0, sus_time_ms: 300.0, sens: 0.5, thresh_db: -66.0 }
    }
}

/// One sample of the detector.
#[derive(Clone, Copy, Debug, Default)]
pub struct Reading {
    /// E_f / E_s ≥ 1 (D_a = 20 log10 of it)
    pub ratio_a: f64,
    /// E_rs / E_rf ≥ 1 (D_s = 20 log10 of it)
    pub ratio_s: f64,
    pub share_att: f64,
    pub share_sus: f64,
    /// E_rf / F ≥ 1: the tail's height over the floor (a sustain boost never exceeds it, in dB)
    pub height: f64,
    /// the floor F (linear)
    pub floor: f64,
}

impl Reading {
    pub fn d_a(&self) -> f64 {
        20.0 * self.ratio_a.log10()
    }
    pub fn d_s(&self) -> f64 {
        20.0 * self.ratio_s.log10()
    }
    pub fn height_db(&self) -> f64 {
        20.0 * self.height.log10()
    }
}

#[derive(Clone, Debug)]
pub struct Detector {
    sr: f64,
    phrase: Phrase,
    es: f64,
    erf: f64,
    ers: f64,
    ka: f64,
    krf: f64,
    krs: f64,
    d_ref: f64,
    /// share = 1 − exp(DEAD_ZONE / d_ref) · ratio^(−k), k = 20 / (ln 10 · d_ref)
    dz_ratio: f64,
    th: f64,
    fw: f64,
}

impl Default for Detector {
    fn default() -> Self {
        let mut d = Self {
            sr: 48000.0,
            phrase: Phrase::default(),
            es: 0.0,
            erf: 0.0,
            ers: 0.0,
            ka: 0.0,
            krf: 0.0,
            krs: 0.0,
            d_ref: 12.0,
            dz_ratio: 10f64.powf(DEAD_ZONE_DB / 20.0),
            th: 0.0,
            fw: 10f64.powf(-FLOOR_WINDOW_DB / 20.0),
        };
        d.set(&Settings::default());
        d
    }
}

impl Detector {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.sr = sample_rate as f64;
        self.phrase.prepare(sample_rate);
        self.krf = one_pole_coef(RF_RELEASE_MS * 0.001 * self.sr);
    }

    pub fn reset(&mut self) {
        self.phrase.reset();
        self.es = 0.0;
        self.erf = 0.0;
        self.ers = 0.0;
    }

    /// Control rate: times, sensitivity, floor.
    pub fn set(&mut self, s: &Settings) {
        self.ka = one_pole_coef(s.att_time_ms.max(0.01) as f64 * 0.001 * self.sr);
        self.krs = one_pole_coef(s.sus_time_ms.max(0.01) as f64 * 0.001 * self.sr);
        self.d_ref = d_ref(s.sens);
        self.th = 10f64.powf(s.thresh_db as f64 / 20.0);
    }

    #[inline]
    fn law(&self, ratio: f64) -> f64 {
        if ratio <= self.dz_ratio {
            0.0
        } else {
            share(ratio.ln() * DB_PER_NEPER, self.d_ref)
        }
    }

    /// One sample of the peak level `p` (linear, ≥ 0).
    #[inline]
    pub fn next(&mut self, p: f32) -> Reading {
        let p = p as f64;
        let big_p = self.phrase.next(p, self.th);
        let f = (big_p * self.fw).max(self.th);
        // attack pair
        let ef = p.max(f);
        self.es = self.es.max(f);
        self.es = if ef < self.es { ef } else { ef + (self.es - ef) * self.ka };
        // sustain pair
        self.erf = if p > self.erf { p } else { p + (self.erf - p) * self.krf };
        self.erf = self.erf.max(f);
        self.ers = if p > self.ers { p } else { p + (self.ers - p) * self.krs };
        self.ers = self.ers.max(f);
        let ratio_a = ef / self.es;
        let ratio_s = self.ers / self.erf;
        Reading {
            ratio_a,
            ratio_s,
            share_att: self.law(ratio_a),
            share_sus: self.law(ratio_s),
            height: self.erf / f,
            floor: f,
        }
    }
}

/// Moving average with an exact zero when its window holds only zeros (gfx-limiter's box filter, on a
/// share), capacity fixed in `prepare`.
#[derive(Clone, Debug, Default)]
struct BoxAvg {
    buf: Vec<f64>,
    len: usize,
    pos: usize,
    sum: f64,
    nonzero: usize,
}

impl BoxAvg {
    fn prepare(&mut self, cap: usize) {
        self.buf = vec![0.0; cap.max(1)];
        self.set_len(1);
    }
    fn set_len(&mut self, len: usize) {
        // (before `prepare` the buffer is empty: length 1, never read)
        self.len = len.min(self.buf.len()).max(1);
        self.reset();
    }
    fn reset(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
        self.pos = 0;
        self.sum = 0.0;
        self.nonzero = 0;
    }
    #[inline]
    fn next(&mut self, v: f64) -> f64 {
        let old = self.buf[self.pos];
        if old != 0.0 {
            self.nonzero -= 1;
        }
        if v != 0.0 {
            self.nonzero += 1;
        }
        self.buf[self.pos] = v;
        self.pos += 1;
        if self.pos == self.len {
            self.pos = 0;
        }
        self.sum += v - old;
        if self.nonzero == 0 {
            self.sum = 0.0;
            0.0
        } else {
            (self.sum / self.len as f64).clamp(0.0, 1.0)
        }
    }
}

/// The lookahead ramp of a share (gfx-limiter's Signalsmith engine, run on a share instead of a
/// reduction): an optional moving maximum over the ramp, two cascaded boxes of half the ramp each,
/// then a delay. Configured for a lookahead L and a ramp R (samples), a share that steps up at
/// detector time n0 leaves the engine starting at n0 + L − R + 1 and reaches its full value at
/// exactly n0 + L: the audio, delayed by L, meets the full gain at its onset.
#[derive(Clone, Debug, Default)]
pub struct Ramp {
    with_max: bool,
    held: SlidingMax,
    b1: BoxAvg,
    b2: BoxAvg,
    delay: Vec<f64>,
    pos: usize,
    d: usize,
}

impl Ramp {
    /// `with_max`: hold the peaks over the ramp (the attack share); without, a plain smoothing (the
    /// sustain share).
    pub fn new(with_max: bool) -> Self {
        Self { with_max, ..Self::default() }
    }

    /// Allocates for lookaheads up to `max_samples`.
    pub fn prepare(&mut self, max_samples: usize) {
        let cap = max_samples + 4;
        self.held.prepare(cap);
        self.b1.prepare(cap);
        self.b2.prepare(cap);
        self.delay = vec![0.0; cap + 1];
        self.configure(0, 0);
    }

    /// Lookahead `la` and ramp `ramp` samples (ramp clamped to 2 … la); state cleared.
    pub fn configure(&mut self, la: usize, ramp: usize) {
        let ramp = ramp.clamp(2, la.max(2));
        let h = (ramp / 2).max(1);
        self.b1.set_len(h);
        self.b2.set_len(h);
        self.held.set_window(2 * h);
        self.held.reset();
        // two boxes of h reach the full value 2h − 2 samples after a step
        self.d = la.saturating_sub(2 * h - 2).min(self.delay.len().saturating_sub(1));
        self.reset();
    }

    pub fn reset(&mut self) {
        self.held.reset();
        self.b1.reset();
        self.b2.reset();
        self.delay.iter_mut().for_each(|v| *v = 0.0);
        self.pos = 0;
    }

    /// The share delay (samples) after the boxes.
    pub fn delay(&self) -> usize {
        self.d
    }

    #[inline]
    pub fn next(&mut self, share: f64) -> f64 {
        let v = if self.with_max { self.held.next(share as f32) as f64 } else { share };
        let b = self.b2.next(self.b1.next(v));
        if self.d == 0 {
            return b;
        }
        let n = self.delay.len();
        self.delay[self.pos] = b;
        let out = self.delay[(self.pos + n - self.d) % n];
        self.pos += 1;
        if self.pos == n {
            self.pos = 0;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidechain::window_secs;

    const SR: f32 = 48000.0;

    /// Runs the detector on a signal through the sliding maximum (`sc_hpf` 0).
    fn run(x: &[f32], s: Settings) -> Vec<Reading> {
        let mut m = SlidingMax::new();
        m.prepare(4096);
        m.set_window((window_secs(0.0, 2.0) * SR) as usize);
        let mut d = Detector::default();
        d.prepare(SR);
        d.set(&s);
        d.reset();
        x.iter().map(|&v| d.next(m.next(v))).collect()
    }

    fn sine(f: f32, amp: f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| amp * (2.0 * std::f32::consts::PI * f * i as f32 / SR).sin()).collect()
    }

    #[test]
    fn law_and_sensitivity() {
        assert_eq!(share(1.5, 12.0), 0.0);
        assert!((d_ref(0.0) - 22.0).abs() < 1e-12 && (d_ref(0.5) - 12.0).abs() < 1e-12 && (d_ref(1.0) - 2.0).abs() < 1e-12);
        // rises of 3 / 6 / 12 / 24 dB at the default sens: 12 / 31 / 58 / 85 % of the gain (the
        // README's 11 / 30 / 57 / 82 % are measured through the detector and the smoothing)
        for (rise, want) in [(3.0, 0.1175), (6.0, 0.3127), (12.0, 0.5831), (24.0, 0.8466)] {
            let s = share(rise, 12.0);
            assert!((s - want).abs() < 1e-3, "{rise}: {s}");
        }
        // sens 100 %: 53 / 89 / 99.5 %
        for (rise, want) in [(3.0, 0.53), (6.0, 0.89), (12.0, 0.995)] {
            let s = share(rise, 2.0);
            assert!((s - want).abs() < 0.01, "{rise}: {s}");
        }
    }

    #[test]
    fn steady_note_gives_nothing() {
        // after the first period the sliding maximum is constant: no share at all
        for f in [31.0, 41.0, 82.0, 1000.0] {
            let r = run(&sine(f, 0.5, (0.5 * SR) as usize), Settings::default());
            let late = &r[(0.3 * SR) as usize..];
            assert!(late.iter().all(|r| r.share_att == 0.0 && r.share_sus == 0.0), "{f}");
        }
    }

    #[test]
    fn attack_length_table() {
        // a burst out of silence, att_time 20 ms: D_a at 0.5 / 1 / 1.5 / 2 att_time (§2.3)
        let n = (0.3 * SR) as usize;
        let i0 = (0.1 * SR) as usize;
        let mut x = vec![0.0f32; n];
        for i in i0..n {
            x[i] = if (i - i0).is_multiple_of(2) { 0.5 } else { -0.5 };
        }
        let r = run(&x, Settings { att_time_ms: 20.0, ..Settings::default() });
        for (t, want) in [(0.5, 8.1), (1.0, 4.0), (1.5, 2.2), (2.0, 1.3)] {
            let d = r[i0 + (t * 0.02 * SR) as usize].d_a();
            assert!((d - want).abs() < 0.15, "{t}: {d}");
        }
        assert_eq!(r[i0 + (2.1 * 0.02 * SR) as usize].share_att, 0.0);
    }

    #[test]
    fn level_independent() {
        let n = (1.0 * SR) as usize;
        let mut x = vec![0.0f32; n];
        // a decaying 220 Hz note at 0.1 s, a louder one at 0.5 s
        for (t0, a) in [(0.1, 0.3f32), (0.5, 0.6)] {
            let i0 = (t0 * SR) as usize;
            for i in i0..n {
                let t = (i - i0) as f32 / SR;
                x[i] += a * (-t * 20.0 / 8.686).exp() * (2.0 * std::f32::consts::PI * 220.0 * t).sin();
            }
        }
        let s = Settings { thresh_db: -96.0, ..Settings::default() };
        let r0 = run(&x, s);
        for lv in [-12.0f32, -24.0, -36.0] {
            let g = 10f32.powf(lv / 20.0);
            let xs: Vec<f32> = x.iter().map(|v| v * g).collect();
            let r = run(&xs, s);
            let (mut worst, mut at) = (0.0f64, 0);
            for (i, (a, b)) in r0.iter().zip(&r).enumerate() {
                // the first 2 ms of an onset out of silence: there the sine's first samples meet the
                // absolute floor (−96 dB) at one level and not at the other
                if (4800..4896).contains(&i) {
                    continue;
                }
                let e = (12.0 * (a.share_att - b.share_att)).abs().max((12.0 * (a.share_sus - b.share_sus)).abs());
                if e > worst {
                    worst = e;
                    at = i;
                }
            }
            assert!(worst < 0.06, "{lv}: {worst} dB at {at}");
        }
    }

    #[test]
    fn sustain_settles_under_the_reference() {
        // a note decaying at r × the reference rate settles at D_s = −20 log10(1 − r)
        // (settled after 9 sus_time, still inside the 48 dB floor window)
        let sus = 100.0;
        let reference = 8.686 / (sus * 0.001);
        for (r, want) in [(0.25, 2.50), (0.5, 6.02)] {
            let n = (1.2 * SR) as usize;
            let i0 = (0.1 * SR) as usize;
            let mut x = vec![0.0f32; n];
            for i in i0..n {
                let t = (i - i0) as f32 / SR;
                x[i] = 0.9 * (-t * r * reference / 8.686).exp() * if i.is_multiple_of(2) { 1.0 } else { -1.0 };
            }
            let rd = run(&x, Settings { sus_time_ms: sus, thresh_db: -120.0, ..Settings::default() });
            let d = rd[i0 + (0.9 * SR) as usize].d_s();
            // E_rf's own 10 ms release lags the note too: 20 log10(1 − r · 10 ms / sus) less
            let want = want + 20.0 * (1.0 - r * RF_RELEASE_MS as f32 / sus).log10();
            assert!((d - want as f64).abs() < 0.1, "{r}: {d} vs {want}");
        }
    }

    #[test]
    fn floor_clamps_everything() {
        // noise at −80 dBFS, `thresh` −66: nothing above the floor, no share, height 1
        let x: Vec<f32> = (0..48000).map(|i| 1e-4 * ((i * 7919 % 101) as f32 / 50.0 - 1.0)).collect();
        let r = run(&x, Settings::default());
        assert!(r.iter().all(|r| r.share_att == 0.0 && r.share_sus == 0.0 && r.height == 1.0));
    }

    #[test]
    fn ramp_reaches_full_at_the_lookahead() {
        for (la, ramp) in [(48usize, 24usize), (96, 24), (240, 96), (48, 48)] {
            let mut r = Ramp::new(true);
            r.prepare(256);
            r.configure(la, ramp);
            let mut out = Vec::new();
            for i in 0..600 {
                // a one-sample spike of share 1 at n0 = 100 (the attack's peak), then its decay
                let s = if i == 100 { 1.0 } else if i > 100 { 0.5 } else { 0.0 };
                out.push(r.next(s));
            }
            let first = out.iter().position(|&v| v > 0.0).unwrap();
            assert!(out[100 + la] > 1.0 - 1e-12, "{la} {ramp}");
            assert!(out[100 + la - 1] < 1.0);
            assert!(first > 100 + la - ramp, "{la} {ramp} {first}");
        }
    }
}
