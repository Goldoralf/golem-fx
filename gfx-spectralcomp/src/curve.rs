// SPDX-License-Identifier: GPL-3.0-or-later
//! The detector's band and the static curve (README §2.2, §2.3, §2.8): pure functions, shared by
//! the per-hop work, the auto make-up and the tests. New in this crate; they move to gfx-dsp when a
//! second user appears (README §2.11).

/// 0 dB = a full-scale sine in its band: L = 10 log10(E / 0.5).
pub const FULL_SCALE_POWER: f64 = 0.5;
/// Floor of a band reading (dB).
pub const FLOOR_DB: f32 = -140.0;
/// The band holds at least ±2 bins: the Hann main lobe (`res` 0 = the main lobe, never one bin).
pub const MIN_HALF_BINS: f32 = 2.0;
/// No upward gain under this frequency (Hz).
pub const UP_MIN_HZ: f32 = 20.0;
/// Auto gain's reference: pink noise at −18 dBFS RMS over 20 Hz – 20 kHz (README §2.8, open
/// question 5: a fixed constant).
pub const PINK_REF_DB: f64 = -18.0;
pub const PINK_LO_HZ: f64 = 20.0;
pub const PINK_HI_HZ: f64 = 20000.0;

/// The settings the static curve reads (one hop's values).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Curve {
    pub thresh: f32,
    pub down: f32,
    pub up: f32,
    pub slope: f32,
    pub center: f32,
    pub knee: f32,
    pub range: f32,
    pub lift_range: f32,
    pub guard: f32,
}

impl Curve {
    /// The threshold at log2(f) is `t0 + slope · log2 f`; this is t0 = thresh − slope · log2(center).
    #[inline]
    pub fn t0(&self) -> f32 {
        self.thresh - self.slope * self.center.max(1.0).log2()
    }

    /// The static gain (dB) of a band reading `l` (dB) against its threshold `t`, at `f` Hz:
    /// downward −(1 − 1 / down) · knee(o) held ≥ −range; upward (1 − 1 / up) · knee(−o), held
    /// ≤ max(0, l − guard) (nothing at or under the guard is lifted), ≤ lift_range, none under 20 Hz.
    #[inline]
    pub fn gain(&self, l: f32, t: f32, f: f32) -> f32 {
        let o = l - t;
        let w = self.knee;
        let gd = if self.down > 1.0 { (-(1.0 - 1.0 / self.down) * knee(o, w)).max(-self.range) } else { 0.0 };
        let gu = if self.up > 1.0 && f >= UP_MIN_HZ {
            ((1.0 - 1.0 / self.up) * knee(-o, w)).min((l - self.guard).max(0.0)).min(self.lift_range)
        } else {
            0.0
        };
        gd + gu
    }
}

/// [`Curve`] with its ratios turned into slopes once per hop.
#[derive(Clone, Copy, Debug)]
pub struct Prepared {
    kd: f32,
    ku: f32,
    knee: f32,
    range: f32,
    lift_range: f32,
    guard: f32,
}

impl Curve {
    pub fn prepared(&self) -> Prepared {
        Prepared {
            kd: if self.down > 1.0 { 1.0 - 1.0 / self.down } else { 0.0 },
            ku: if self.up > 1.0 { 1.0 - 1.0 / self.up } else { 0.0 },
            knee: self.knee,
            range: self.range,
            lift_range: self.lift_range,
            guard: self.guard,
        }
    }
}

impl Prepared {
    /// [`Curve::gain`], same arithmetic.
    #[inline]
    pub fn gain(&self, l: f32, t: f32, f: f32) -> f32 {
        let o = l - t;
        let gd = if self.kd > 0.0 { (-self.kd * knee(o, self.knee)).max(-self.range) } else { 0.0 };
        let gu = if self.ku > 0.0 && f >= UP_MIN_HZ { (self.ku * knee(-o, self.knee)).min((l - self.guard).max(0.0)).min(self.lift_range) } else { 0.0 };
        gd + gu
    }
}

/// Giannoulis et al.'s quadratic knee: 0 for x ≤ −W / 2, (x + W / 2)² / (2W) inside, x above.
#[inline]
pub fn knee(x: f32, w: f32) -> f32 {
    if w <= 0.0 {
        x.max(0.0)
    } else if x <= -0.5 * w {
        0.0
    } else if x >= 0.5 * w {
        x
    } else {
        let a = x + 0.5 * w;
        a * a / (2.0 * w)
    }
}

/// The band's half-widths above and below bin k, as factors of k: (2^(r/2) − 1, 1 − 2^(−r/2)) for
/// `res` in semitones (r = res / 12 octaves).
#[inline]
pub fn band_factors(res_st: f32) -> (f32, f32) {
    let r = (res_st.max(0.0) / 12.0) as f64;
    (((r / 2.0).exp2() - 1.0) as f32, (1.0 - (-r / 2.0).exp2()) as f32)
}

/// The band of bin k: [k − d_k, k + u_k] in bins, u_k = max(2, k · fu), d_k = max(2, k · fd).
#[inline]
pub fn band_edges(k: usize, fu: f32, fd: f32) -> (f32, f32) {
    let kf = k as f32;
    (kf - (kf * fd).max(MIN_HALF_BINS), kf + (kf * fu).max(MIN_HALF_BINS))
}

/// The prefix sum S (S[0] = 0, S[i + 1] = S[i] + P[i]) read at bin position y with fractional
/// edges: bin i occupies [i − ½, i + ½).
#[inline]
pub fn cum(s: &[f64], p: &[f32], nb: usize, y: f32) -> f64 {
    let x = (y + 0.5).clamp(0.0, nb as f32);
    let i = (x as usize).min(nb - 1);
    s[i] + (x - i as f32) as f64 * p[i] as f64
}

/// A band power as a level (dB re a full-scale sine), floored at −140 dB.
#[inline]
pub fn level_db(e: f64) -> f32 {
    level_of((e / FULL_SCALE_POWER) as f32)
}

/// The level (dB) of a band power already divided by 0.5, floored at −140 dB; branch-free.
#[inline]
pub fn level_of(v: f32) -> f32 {
    (10.0 * std::f32::consts::LOG10_2) * fast_log2(v.max(1e-14))
}

/// log2(x) for a normal x > 0: the exponent plus 2 / ln 2 · atanh(t), t = (m − 1) / (m + 1) for the
/// mantissa m ∈ [1, 2), to t¹¹ (|t| ≤ 1/3): error < 1e-5 (3e-5 dB on a level). No libm call.
#[inline]
pub fn fast_log2(x: f32) -> f32 {
    let bits = x.to_bits();
    let e = ((bits >> 23) & 0xff) as i32 - 127;
    let m = f32::from_bits((bits & 0x007f_ffff) | 0x3f80_0000);
    let t = (m - 1.0) / (m + 1.0);
    let t2 = t * t;
    e as f32 + t * (2.885_39 + t2 * (0.961_796_7 + t2 * (0.577_078 + t2 * (0.412_198_6 + t2 * 0.320_598_9))))
}

/// 2^y for |y| < 126 by a degree-6 polynomial on the rounded fraction: relative error < 3e-7.
#[inline]
pub fn fast_exp2(y: f32) -> f32 {
    let y = y.clamp(-126.0, 126.0);
    // nearest integer by a cast (no libm round)
    let i = (y + 0.5f32.copysign(y)) as i32 as f32;
    let f = y - i;
    let p = 1.0 + f * (std::f32::consts::LN_2 + f * (0.240_226_5 + f * (0.055_504_11 + f * (0.009_618_129 + f * (0.001_333_355_8 + f * 0.000_154_035_3)))));
    p * f32::from_bits(((i as i32 + 127) as u32) << 23)
}

/// 10^(g / 20) (a gain in dB as a linear factor), exactly 1 at 0 dB.
#[inline]
pub fn db_to_lin(g: f32) -> f32 {
    // branch-free: fast_exp2(±0) is exactly 1
    fast_exp2(g * (std::f32::consts::LOG2_10 / 20.0))
}

/// The static make-up (dB) that returns pink noise at −18 dBFS RMS (20 Hz – 20 kHz), read by the
/// detector at `res` as set (N points at `sr`), to its own power (README §2.8). Bins `k0..k1` of
/// the sum; [`makeup_db`] finishes it. Returns the (weight, weighted power gain) partial sums.
pub fn makeup_part(c: &Curve, n: usize, sr: f32, res_st: f32, k0: usize, k1: usize) -> (f64, f64) {
    let (fu, fd) = band_factors(res_st);
    let bin = sr as f64 / n as f64;
    let tot = 10f64.powf(PINK_REF_DB / 10.0);
    let span = (PINK_HI_HZ / PINK_LO_HZ).ln();
    let t0 = c.t0();
    let (mut sw, mut sg) = (0.0f64, 0.0f64);
    for k in k0.max(1)..k1 {
        let f = k as f64 * bin;
        if !(PINK_LO_HZ..=PINK_HI_HZ).contains(&f) {
            continue;
        }
        let (lo, hi) = band_edges(k, fu, fd);
        let flo = (lo as f64 * bin).clamp(PINK_LO_HZ, PINK_HI_HZ);
        let fhi = (hi as f64 * bin).clamp(PINK_LO_HZ, PINK_HI_HZ);
        let e = tot * (fhi / flo).max(1.0).ln() / span;
        let l = level_db(e);
        let ff = f as f32;
        let g = c.gain(l, t0 + c.slope * ff.log2(), ff) as f64;
        let w = 1.0 / f;
        sw += w;
        sg += w * (g * std::f64::consts::LN_10 / 10.0).exp();
    }
    (sw, sg)
}

/// The make-up (dB) from [`makeup_part`]'s sums over every bin.
pub fn makeup_db(sw: f64, sg: f64) -> f32 {
    if sw <= 0.0 || sg <= 0.0 {
        0.0
    } else {
        (-10.0 * (sg / sw).log10()) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c() -> Curve {
        Curve { thresh: -36.0, down: 4.0, up: 3.0, slope: 0.0, center: 420.0, knee: 0.0, range: 60.0, lift_range: 24.0, guard: -96.0 }
    }

    #[test]
    fn fast_log_and_exp_are_accurate() {
        let mut x = 1e-13f32;
        while x < 1e6 {
            assert!((fast_log2(x) as f64 - (x as f64).log2()).abs() < 1e-5, "{x}");
            x *= 1.37;
        }
        let mut y = -60.0f32;
        while y < 40.0 {
            let a = db_to_lin(y) as f64;
            let b = 10f64.powf(y as f64 / 20.0);
            assert!((a / b - 1.0).abs() < 1e-6, "{y}");
            y += 0.173;
        }
        assert_eq!(db_to_lin(0.0), 1.0);
    }

    #[test]
    fn knee_is_continuous_and_quadratic() {
        assert_eq!(knee(-3.0, 6.0), 0.0);
        assert_eq!(knee(3.0, 6.0), 3.0);
        assert!((knee(0.0, 6.0) - 0.75).abs() < 1e-6);
        assert!((knee(-2.999, 6.0)).abs() < 1e-5 && (knee(2.999, 6.0) - 2.999).abs() < 1e-3);
        assert_eq!(knee(-1.0, 0.0), 0.0);
    }

    #[test]
    fn curve_down_up_guard_ranges() {
        let mut k = c();
        // 12 dB over: down 4 → −9 dB; 12 dB under: up 3 → +8 dB
        assert!((k.gain(-24.0, -36.0, 1000.0) + 9.0).abs() < 1e-5);
        assert!((k.gain(-48.0, -36.0, 1000.0) - 8.0).abs() < 1e-5);
        // the guard: at −90 with guard −96 at most +6
        assert!((k.gain(-90.0, -36.0, 1000.0) - 6.0).abs() < 1e-5);
        k.guard = -72.0;
        assert_eq!(k.gain(-80.0, -36.0, 1000.0), 0.0);
        // lift range, range, 20 Hz
        k.guard = -96.0;
        k.lift_range = 3.0;
        assert!((k.gain(-60.0, -36.0, 1000.0) - 3.0).abs() < 1e-6);
        assert_eq!(k.gain(-60.0, -36.0, 15.0), 0.0);
        k.range = 6.0;
        assert!((k.gain(0.0, -36.0, 1000.0) + 6.0).abs() < 1e-6);
        // ratios 1: exactly 0
        k.down = 1.0;
        k.up = 1.0;
        assert_eq!(k.gain(0.0, -36.0, 1000.0), 0.0);
        assert_eq!(k.gain(-60.0, -36.0, 1000.0), 0.0);
    }

    #[test]
    fn slope_is_db_per_octave_about_center() {
        let k = Curve { slope: -6.0, center: 1000.0, ..c() };
        let t = |f: f32| k.t0() + k.slope * f.log2();
        assert!((t(1000.0) + 36.0).abs() < 1e-4);
        assert!((t(2000.0) + 42.0).abs() < 1e-4);
        assert!((t(250.0) + 24.0).abs() < 1e-4);
    }

    #[test]
    fn band_is_constant_q_with_two_bins_minimum() {
        let (fu, fd) = band_factors(2.0);
        assert!((fu - (2f32.powf(1.0 / 12.0) - 1.0)).abs() < 1e-6);
        let (lo, hi) = band_edges(10, fu, fd);
        assert_eq!((lo, hi), (8.0, 12.0));
        let (lo, hi) = band_edges(1000, fu, fd);
        assert!((hi - 1000.0 * 2f32.powf(1.0 / 12.0)).abs() < 1e-2 && (lo - 1000.0 * 2f32.powf(-1.0 / 12.0)).abs() < 1e-2);
        let (fu0, fd0) = band_factors(0.0);
        assert_eq!(band_edges(5000, fu0, fd0), (4998.0, 5002.0));
    }

    /// Auto make-up: ratios 1 → 0 dB; a heavy downward setting → positive; deterministic.
    #[test]
    fn makeup_follows_the_curve() {
        let flat = Curve { down: 1.0, up: 1.0, ..c() };
        let (w, g) = makeup_part(&flat, 1024, 48000.0, 2.0, 0, 513);
        assert!(makeup_db(w, g).abs() < 1e-9);
        let heavy = Curve { thresh: -60.0, down: 8.0, up: 1.0, ..c() };
        let (w, g) = makeup_part(&heavy, 1024, 48000.0, 2.0, 0, 513);
        assert!(makeup_db(w, g) > 10.0);
        // split sums add up
        let (w1, g1) = makeup_part(&heavy, 1024, 48000.0, 2.0, 0, 200);
        let (w2, g2) = makeup_part(&heavy, 1024, 48000.0, 2.0, 200, 513);
        assert!((makeup_db(w1 + w2, g1 + g2) - makeup_db(w, g)).abs() < 1e-5);
    }
}
