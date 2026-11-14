// SPDX-License-Identifier: GPL-3.0-or-later
//! Accurate graphic equalizer (gfx-geq README §3.1 – §3.3): ACGE (Välimäki & Liski, IEEE SPL 2017)
//! with **fill-in sections** between the faders, solved to convergence on the exact digital response
//! of matched bells.
//!
//! - **Sections** ([`sections`]): one bell per fader, plus fill-in bells cutting each gap of g octaves
//!   into ⌈g / 0.7⌉ equal log-frequency parts (at least two from half an octave up).
//! - **Band filter** ([`bell_q`]): a symmetric matched bell whose edges are its neighbouring sections,
//!   where its dB gain is c = 0.4 of its peak's; Q follows from the gain in closed form.
//! - **Target** ([`Pchip`]): a monotone cubic in log-frequency through the faders, defined between the
//!   first and the last fader only.
//! - **Solver** ([`Plan`]): the interaction matrix at a 15 dB prototype gain, its weighted, Tikhonov-
//!   regularised pseudo-inverse P, then chord iterations g ← clamp(g + P · (T − R(g)), ±30 dB) on the
//!   exact response R at the design points (the sections' centres and the midpoints between them).
//!   Cold start: to convergence (0.005 dB, at most 8); running: one correction per control tick,
//!   warm-started from the previous gains.
//!
//! Everything is fixed-size: a [`Plan`] is built once per layout and sample rate (`prepare`), and the
//! per-tick work allocates nothing. The sections' centres never move, so each one caches the fitting
//! grid of the matched design ([`FixedBell`]).

use crate::matched::{Analog, Biquad};
use std::f64::consts::PI;

/// Most faders of a layout.
pub const MAX_BANDS: usize = 11;
/// Most sections (faders + fill-ins): Octave 11 has 20.
pub const MAX_SECTIONS: usize = 20;
/// Most design points: the sections and the midpoints between them.
pub const MAX_POINTS: usize = 2 * MAX_SECTIONS - 1;

/// A section's dB gain at its edges, as a fraction of its peak gain (ACGE's c; 0.3 with one bell per band).
pub const EDGE_GAIN: f64 = 0.4;
/// Gain of the prototype sections the interaction matrix is measured on (dB).
pub const PROTO_DB: f64 = 15.0;
/// Tikhonov regularisation of the pseudo-inverse.
pub const LAMBDA: f64 = 1e-3;
/// Largest filter gain the solver may ask for (dB).
pub const GAIN_CLAMP_DB: f64 = 30.0;
/// A correction below this (dB, every section) is converged.
pub const TOLERANCE_DB: f64 = 0.005;
/// Corrections of a cold start, at most.
pub const MAX_COLD_CORRECTIONS: usize = 8;
/// Design-point weights: at a fader, at a fill-in section, at a midpoint between two sections.
pub const WEIGHT_FADER: f64 = 1.0;
pub const WEIGHT_FILL: f64 = 0.15;
pub const WEIGHT_MID: f64 = 0.05;
/// Fill-in rule: parts of at most this many octaves, and at least two parts from `FILL_MIN_OCT` up.
const FILL_GAP_OCT: f64 = 0.7;
const FILL_MIN_OCT: f64 = 0.5;
/// Sections above this fraction of the sample rate are dropped (never at 44.1 kHz or above).
const TOP: f64 = 0.45;

/// The section grid of a fader layout: the faders and the fill-ins between them, ascending. Returns
/// the section count; `is_fader[k]` tells a fader's section from a fill-in. Sections above 0.45 fs
/// are dropped.
pub fn sections(faders: &[f64], sample_rate: f64, freq: &mut [f64; MAX_SECTIONS], is_fader: &mut [bool; MAX_SECTIONS]) -> usize {
    assert!(!faders.is_empty() && faders.len() <= MAX_BANDS, "1 – {MAX_BANDS} faders");
    let mut n = 0;
    let mut push = |f: f64, fader: bool| {
        if f <= TOP * sample_rate {
            assert!(n < MAX_SECTIONS, "more than {MAX_SECTIONS} sections");
            freq[n] = f;
            is_fader[n] = fader;
            n += 1;
        }
    };
    push(faders[0], true);
    for w in faders.windows(2) {
        let gap = (w[1] / w[0]).log2();
        let mut k = (gap / FILL_GAP_OCT).ceil().max(1.0) as usize;
        if gap >= FILL_MIN_OCT {
            k = k.max(2);
        }
        for j in 1..k {
            push(w[0] * (gap * j as f64 / k as f64).exp2(), false);
        }
        push(w[1], true);
    }
    n
}

/// Q of a symmetric bell of `gain_db` whose dB gain at Ω = `edge_ratio` (and 1 / Ω) is `c` × its
/// peak's: with x = (1 − Ω²)² / Ω², α = 10^(|G| / 20), K = α^(2c), 1 / Q² = x (K − 1) / (α − K / α),
/// whose limit at G → 0 is x c / (1 − c).
pub fn bell_q(gain_db: f64, edge_ratio: f64, c: f64) -> f64 {
    let e2 = edge_ratio * edge_ratio;
    let x = (1.0 - e2).powi(2) / e2;
    let iq2 = if gain_db.abs() < 1e-6 {
        x * c / (1.0 - c)
    } else {
        let a = 10f64.powf(gain_db.abs() / 20.0);
        let k = a.powf(2.0 * c);
        x * (k - 1.0) / (a - k / a)
    };
    1.0 / iq2.sqrt()
}

/// Monotone piecewise cubic Hermite interpolation (Fritsch – Carlson, with SciPy's `PchipInterpolator`
/// end conditions): no overshoot, flat where two neighbouring values are equal. Clamped outside.
#[derive(Clone, Copy, Debug)]
pub struct Pchip {
    n: usize,
    x: [f64; MAX_BANDS],
    y: [f64; MAX_BANDS],
    d: [f64; MAX_BANDS],
}

impl Pchip {
    /// `x` strictly ascending, 2 – 11 points (one point: a constant).
    pub fn new(x: &[f64], y: &[f64]) -> Self {
        let n = x.len();
        assert!(n == y.len() && (1..=MAX_BANDS).contains(&n));
        let mut s = Self { n, x: [0.0; MAX_BANDS], y: [0.0; MAX_BANDS], d: [0.0; MAX_BANDS] };
        s.x[..n].copy_from_slice(x);
        s.y[..n].copy_from_slice(y);
        if n < 2 {
            return s;
        }
        let mut h = [0.0; MAX_BANDS];
        let mut m = [0.0; MAX_BANDS];
        for k in 0..n - 1 {
            h[k] = x[k + 1] - x[k];
            m[k] = (y[k + 1] - y[k]) / h[k];
        }
        if n == 2 {
            s.d[0] = m[0];
            s.d[1] = m[0];
            return s;
        }
        for k in 1..n - 1 {
            let (m0, m1) = (m[k - 1], m[k]);
            s.d[k] = if m0 * m1 <= 0.0 {
                0.0
            } else {
                let w1 = 2.0 * h[k] + h[k - 1];
                let w2 = h[k] + 2.0 * h[k - 1];
                (w1 + w2) / (w1 / m0 + w2 / m1)
            };
        }
        // sign with sign(0) = 0, as numpy's
        let sg = |v: f64| if v > 0.0 { 1 } else if v < 0.0 { -1 } else { 0 };
        let end = |h0: f64, h1: f64, m0: f64, m1: f64| {
            let d = ((2.0 * h0 + h1) * m0 - h0 * m1) / (h0 + h1);
            if sg(d) != sg(m0) {
                0.0
            } else if sg(m0) != sg(m1) && d.abs() > 3.0 * m0.abs() {
                3.0 * m0
            } else {
                d
            }
        };
        s.d[0] = end(h[0], h[1], m[0], m[1]);
        s.d[n - 1] = end(h[n - 2], h[n - 3], m[n - 2], m[n - 3]);
        s
    }

    pub fn eval(&self, x: f64) -> f64 {
        let n = self.n;
        if n < 2 || x <= self.x[0] {
            return self.y[0];
        }
        if x >= self.x[n - 1] {
            return self.y[n - 1];
        }
        let mut k = 0;
        while x > self.x[k + 1] {
            k += 1;
        }
        let h = self.x[k + 1] - self.x[k];
        let t = (x - self.x[k]) / h;
        let (t2, t3) = (t * t, t * t * t);
        (2.0 * t3 - 3.0 * t2 + 1.0) * self.y[k]
            + (t3 - 2.0 * t2 + t) * h * self.d[k]
            + (-2.0 * t3 + 3.0 * t2) * self.y[k + 1]
            + (t3 - t2) * h * self.d[k + 1]
    }
}

/// The φ basis of a digital magnitude at `freq`: |H|² = (Σ b)² p0 + (b0 − b1 + b2)² p1 − 4 b0 b2 p2
/// over the same for a (`matched::Biquad::mag2`, with the sine computed once).
#[derive(Clone, Copy, Debug, Default)]
pub struct Phi {
    p0: f64,
    p1: f64,
    p2: f64,
}

impl Phi {
    pub fn new(freq: f64, sample_rate: f64) -> Self {
        let s = (PI * freq / sample_rate).sin().powi(2);
        Self { p0: 1.0 - s, p1: s, p2: 4.0 * (1.0 - s) * s }
    }

    #[inline]
    pub fn mag2(&self, b: &Biquad) -> f64 {
        let n = (b.b0 + b.b1 + b.b2).powi(2) * self.p0 + (b.b0 - b.b1 + b.b2).powi(2) * self.p1 - 4.0 * b.b0 * b.b2 * self.p2;
        let d = (1.0 + b.a1 + b.a2).powi(2) * self.p0 + (1.0 - b.a1 + b.a2).powi(2) * self.p1 - 4.0 * b.a2 * self.p2;
        n / d.max(1e-300)
    }
}

const GRID: usize = 25;

/// `matched::design` of a symmetric bell whose centre never moves: the 25-point fitting grid (its
/// frequencies and φ terms) is computed once. Same arithmetic, same result.
#[derive(Clone, Copy, Debug)]
pub struct FixedBell {
    w0: f64,
    omega: [f64; GRID],
    phi: [(f64, f64, f64); GRID],
}

impl Default for FixedBell {
    fn default() -> Self {
        Self::new(1000.0, 48000.0)
    }
}

impl FixedBell {
    pub fn new(f0: f64, sample_rate: f64) -> Self {
        let w0 = 2.0 * PI * (f0 / sample_rate).clamp(1e-6, 0.4999);
        let lo = (w0 / 16.0).max(1e-4);
        let hi = (w0 * 16.0).min(0.95 * PI);
        let mut s = Self { w0, omega: [0.0; GRID], phi: [(0.0, 0.0, 0.0); GRID] };
        for k in 0..GRID {
            let w = lo * (hi / lo).powf(k as f64 / (GRID - 1) as f64);
            let sn = (w / 2.0).sin().powi(2);
            s.omega[k] = w / w0;
            s.phi[k] = (1.0 - sn, sn, 4.0 * (1.0 - sn) * sn);
        }
        s
    }

    /// The matched bell of `gain_db` and `q` at this centre (= `matched::design(Analog::bell(q, gain_db), …)`).
    pub fn design(&self, q: f64, gain_db: f64) -> Biquad {
        let proto = Analog::bell(q, gain_db);
        if gain_db < 0.0 {
            // a cut: the mirror boost, inverted (as matched::design does)
            let boost = self.direct(Analog { n: proto.d, d: proto.n });
            return Biquad { b0: 1.0 / boost.b0, b1: boost.a1 / boost.b0, b2: boost.a2 / boost.b0, a1: boost.b1 / boost.b0, a2: boost.b2 / boost.b0 };
        }
        self.direct(proto)
    }

    fn direct(&self, proto: Analog) -> Biquad {
        let w0 = self.w0;
        let (d0, d1, d2) = (proto.d[0], proto.d[1], proto.d[2]);
        let wp = (d0 / d2).sqrt() * w0;
        let zeta = d1 / (2.0 * (d0 * d2).sqrt());
        let (a1, a2) = if zeta <= 1.0 {
            let e = (-zeta * wp).exp();
            (-2.0 * e * ((1.0 - zeta * zeta).sqrt() * wp).cos(), e * e)
        } else {
            let e = (-zeta * wp).exp();
            (-2.0 * e * ((zeta * zeta - 1.0).sqrt() * wp).cosh(), e * e)
        };
        let a0p = (1.0 + a1 + a2).powi(2);
        let a1p = (1.0 - a1 + a2).powi(2);
        let a2p = -4.0 * a2;
        let t0 = proto.mag2(0.0);
        let tn = proto.mag2(PI / w0);
        let b0p = t0 * a0p;
        let b1p = tn * a1p;
        let (mut num, mut den) = (0.0f64, 0.0f64);
        for k in 0..GRID {
            let (p0, p1, p2) = self.phi[k];
            let target = proto.mag2(self.omega[k]) * (a0p * p0 + a1p * p1 + a2p * p2);
            if target <= 1e-12 || p2 <= 1e-12 {
                continue;
            }
            let r = target - b0p * p0 - b1p * p1;
            let wgt = 1.0 / (target * target);
            num += wgt * p2 * r;
            den += wgt * p2 * p2;
        }
        let b2p = if den > 0.0 { num / den } else { 0.0 };
        let sb0 = b0p.max(0.0).sqrt();
        let sb1 = b1p.max(0.0).sqrt();
        let w = 0.5 * (sb0 + sb1);
        let b0 = 0.5 * (w + (w * w + b2p).max(0.0).sqrt());
        let b1 = 0.5 * (sb0 - sb1);
        let b2 = w - b0;
        Biquad { b0, b1, b2, a1, a2 }
    }
}

/// Everything the solver needs for one fader layout at one sample rate, built once (`prepare`).
#[derive(Clone, Debug)]
pub struct Plan {
    sample_rate: f64,
    n_bands: usize,
    bands: [f64; MAX_BANDS],
    log_bands: [f64; MAX_BANDS],
    n: usize,
    freq: [f64; MAX_SECTIONS],
    is_fader: [bool; MAX_SECTIONS],
    edge: [f64; MAX_SECTIONS],
    bells: [FixedBell; MAX_SECTIONS],
    np: usize,
    points: [f64; MAX_POINTS],
    log_points: [f64; MAX_POINTS],
    phi: [Phi; MAX_POINTS],
    /// the regularised pseudo-inverse, P[section][point]
    p: [[f64; MAX_POINTS]; MAX_SECTIONS],
}

impl Plan {
    /// The plan of `faders` (Hz, ascending) at `sample_rate`. Allocation-free, a few tens of µs.
    pub fn new(faders: &[f64], sample_rate: f64) -> Self {
        let mut freq = [0.0; MAX_SECTIONS];
        let mut is_fader = [false; MAX_SECTIONS];
        let n = sections(faders, sample_rate, &mut freq, &mut is_fader);
        let mut s = Self {
            sample_rate,
            n_bands: faders.len(),
            bands: [0.0; MAX_BANDS],
            log_bands: [0.0; MAX_BANDS],
            n,
            freq,
            is_fader,
            edge: [1.0; MAX_SECTIONS],
            bells: [FixedBell::default(); MAX_SECTIONS],
            np: 0,
            points: [0.0; MAX_POINTS],
            log_points: [0.0; MAX_POINTS],
            phi: [Phi::default(); MAX_POINTS],
            p: [[0.0; MAX_POINTS]; MAX_SECTIONS],
        };
        for (k, &f) in faders.iter().enumerate() {
            s.bands[k] = f;
            s.log_bands[k] = f.log2();
        }
        // band edges at the neighbouring sections: the mean log-distance to them
        for m in 0..n {
            let mut d = 0.0;
            let mut c = 0;
            if m > 0 {
                d += (freq[m] / freq[m - 1]).log2();
                c += 1;
            }
            if m + 1 < n {
                d += (freq[m + 1] / freq[m]).log2();
                c += 1;
            }
            s.edge[m] = if c > 0 { (d / c as f64).exp2() } else { 2.0 };
            s.bells[m] = FixedBell::new(freq[m], sample_rate);
        }
        // design points: the sections and the midpoints between them, with their weights
        let mut w = [0.0; MAX_POINTS];
        for m in 0..n {
            if m > 0 {
                s.points[s.np] = (freq[m - 1] * freq[m]).sqrt();
                w[s.np] = WEIGHT_MID;
                s.np += 1;
            }
            s.points[s.np] = freq[m];
            w[s.np] = if is_fader[m] { WEIGHT_FADER } else { WEIGHT_FILL };
            s.np += 1;
        }
        for k in 0..s.np {
            s.log_points[k] = s.points[k].log2();
            s.phi[k] = Phi::new(s.points[k], sample_rate);
        }
        // interaction matrix: each section's normalised dB response at the prototype gain
        let np = s.np;
        let mut b = [[0.0; MAX_SECTIONS]; MAX_POINTS];
        for m in 0..n {
            let d = s.bells[m].design(bell_q(PROTO_DB, s.edge[m], EDGE_GAIN), PROTO_DB);
            for k in 0..np {
                b[k][m] = 10.0 * s.phi[k].mag2(&d).log10() / PROTO_DB;
            }
        }
        // P = (Bᵀ W B + λ I)⁻¹ Bᵀ W, by Cholesky
        let mut a = [[0.0; MAX_SECTIONS]; MAX_SECTIONS];
        for i in 0..n {
            for j in 0..n {
                let mut acc = if i == j { LAMBDA } else { 0.0 };
                for k in 0..np {
                    acc += b[k][i] * w[k] * b[k][j];
                }
                a[i][j] = acc;
            }
        }
        let mut l = [[0.0; MAX_SECTIONS]; MAX_SECTIONS];
        for i in 0..n {
            for j in 0..=i {
                let mut acc = a[i][j];
                for k in 0..j {
                    acc -= l[i][k] * l[j][k];
                }
                l[i][j] = if i == j { acc.max(1e-300).sqrt() } else { acc / l[j][j] };
            }
        }
        for k in 0..np {
            // solve L Lᵀ x = column k of Bᵀ W
            let mut y = [0.0; MAX_SECTIONS];
            for i in 0..n {
                let mut acc = b[k][i] * w[k];
                for j in 0..i {
                    acc -= l[i][j] * y[j];
                }
                y[i] = acc / l[i][i];
            }
            for i in (0..n).rev() {
                let mut acc = y[i];
                for j in i + 1..n {
                    acc -= l[j][i] * s.p[j][k];
                }
                s.p[i][k] = acc / l[i][i];
            }
        }
        s
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }
    /// Fader count.
    pub fn bands(&self) -> &[f64] {
        &self.bands[..self.n_bands]
    }
    /// Section count (faders + fill-ins).
    pub fn len(&self) -> usize {
        self.n
    }
    pub fn is_empty(&self) -> bool {
        self.n == 0
    }
    /// Section centres (Hz).
    pub fn section_freqs(&self) -> &[f64] {
        &self.freq[..self.n]
    }
    pub fn is_fader(&self, m: usize) -> bool {
        self.is_fader[m]
    }
    /// Design points (Hz).
    pub fn points(&self) -> &[f64] {
        &self.points[..self.np]
    }

    /// The fader curve: a monotone cubic in log-frequency through `fader_db`.
    pub fn curve(&self, fader_db: &[f64]) -> Pchip {
        Pchip::new(&self.log_bands[..self.n_bands], &fader_db[..self.n_bands])
    }

    /// The target at the design points.
    pub fn targets(&self, fader_db: &[f64], t: &mut [f64; MAX_POINTS]) {
        let c = self.curve(fader_db);
        for k in 0..self.np {
            t[k] = c.eval(self.log_points[k]);
        }
    }

    /// The matched design of section `m` at `gain_db`.
    #[inline]
    pub fn design(&self, m: usize, gain_db: f64) -> Biquad {
        self.bells[m].design(bell_q(gain_db, self.edge[m], EDGE_GAIN), gain_db)
    }

    /// Designs every section from its gain.
    pub fn designs(&self, g: &[f64; MAX_SECTIONS], out: &mut [Biquad; MAX_SECTIONS]) {
        for m in 0..self.n {
            out[m] = self.design(m, g[m]);
        }
    }

    /// One chord correction from `designs` (the designs of `g`): g ← clamp(g + P (T − R(g))), then
    /// `designs` ← the designs of the new g. Returns the largest |Δg| (dB).
    pub fn correct(&self, t: &[f64; MAX_POINTS], g: &mut [f64; MAX_SECTIONS], designs: &mut [Biquad; MAX_SECTIONS]) -> f64 {
        let mut e = [0.0; MAX_POINTS];
        for k in 0..self.np {
            let mut m2 = 1.0;
            for d in &designs[..self.n] {
                m2 *= self.phi[k].mag2(d);
            }
            e[k] = t[k] - 10.0 * m2.max(1e-300).log10();
        }
        let mut worst = 0.0f64;
        for m in 0..self.n {
            let mut dg = 0.0;
            for k in 0..self.np {
                dg += self.p[m][k] * e[k];
            }
            let new = (g[m] + dg).clamp(-GAIN_CLAMP_DB, GAIN_CLAMP_DB);
            worst = worst.max((new - g[m]).abs());
            g[m] = new;
        }
        self.designs(g, designs);
        worst
    }

    /// Cold start: g₀ = P T, then corrections until the change is below 0.005 dB (at most 8).
    /// Returns the corrections made.
    pub fn solve(&self, t: &[f64; MAX_POINTS], g: &mut [f64; MAX_SECTIONS], designs: &mut [Biquad; MAX_SECTIONS]) -> usize {
        for m in 0..MAX_SECTIONS {
            g[m] = 0.0;
            if m < self.n {
                let mut acc = 0.0;
                for k in 0..self.np {
                    acc += self.p[m][k] * t[k];
                }
                g[m] = acc.clamp(-GAIN_CLAMP_DB, GAIN_CLAMP_DB);
            }
        }
        self.designs(g, designs);
        if t[..self.np].iter().all(|&v| v == 0.0) {
            // flat is flat: g ≡ 0 exactly (the 0 dB matched bell is only nearly the identity)
            return 0;
        }
        for it in 1..=MAX_COLD_CORRECTIONS {
            if self.correct(t, g, designs) < TOLERANCE_DB {
                return it;
            }
        }
        MAX_COLD_CORRECTIONS
    }

    /// Response of the cascade `designs` at `freq` (dB).
    pub fn response_db(&self, designs: &[Biquad; MAX_SECTIONS], freq: f64) -> f64 {
        let phi = Phi::new(freq, self.sample_rate);
        let mut m2 = 1.0;
        for d in &designs[..self.n] {
            m2 *= phi.mag2(d);
        }
        10.0 * m2.max(1e-300).log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matched::design;

    const OCT: [f64; 11] = [31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 12000.0, 16000.0];
    const GB: [f64; 7] = [50.0, 120.0, 400.0, 500.0, 800.0, 4500.0, 10000.0];

    #[test]
    fn fill_in_rule() {
        let mut f = [0.0; MAX_SECTIONS];
        let mut is = [false; MAX_SECTIONS];
        assert_eq!(sections(&OCT, 48000.0, &mut f, &mut is), 20);
        assert_eq!(sections(&[100.0, 200.0, 400.0, 800.0, 1600.0, 3200.0, 6400.0], 48000.0, &mut f, &mut is), 13);
        let n = sections(&GB, 48000.0, &mut f, &mut is);
        assert_eq!(n, 15);
        // 400 – 500 Hz: no fill-in; 800 Hz – 4.5 kHz: three
        let i400 = f.iter().position(|&x| x == 400.0).unwrap();
        assert_eq!(f[i400 + 1], 500.0);
        let i800 = f.iter().position(|&x| x == 800.0).unwrap();
        assert_eq!(f[i800 + 4], 4500.0);
        assert!(is[i800] && !is[i800 + 1] && !is[i800 + 3]);
        // 16 kHz is dropped at 32 kHz (above 0.45 fs)
        assert_eq!(sections(&OCT, 32000.0, &mut f, &mut is), 19);
    }

    #[test]
    fn q_law_gives_the_edge_gain() {
        for &g in &[-15.0, -3.0, 0.5, 6.0, 24.0] {
            for &e in &[1.25, 2.0, 2.4] {
                let q = bell_q(g, e, EDGE_GAIN);
                let at = 10.0 * Analog::bell(q, g).mag2(e).log10();
                assert!((at - EDGE_GAIN * g).abs() < 1e-9, "G {g} edge {e}: {at} vs {}", EDGE_GAIN * g);
            }
        }
        // continuous at 0 dB
        assert!((bell_q(1e-7, 2.0, EDGE_GAIN) - bell_q(1e-3, 2.0, EDGE_GAIN)).abs() < 1e-3);
    }

    #[test]
    fn fixed_bell_is_matched_design() {
        for &(f0, sr) in &[(31.25, 192000.0), (1000.0, 48000.0), (16000.0, 44100.0)] {
            let fb = FixedBell::new(f0, sr);
            for &(q, g) in &[(0.3, 15.0), (1.2, -9.0), (2.0, 0.0), (0.7, 29.0)] {
                let a = fb.design(q, g);
                let b = design(Analog::bell(q, g), f0, sr);
                for (x, y) in [(a.b0, b.b0), (a.b1, b.b1), (a.b2, b.b2), (a.a1, b.a1), (a.a2, b.a2)] {
                    assert!((x - y).abs() <= 1e-12 * y.abs().max(1.0), "{f0} Hz {g} dB: {a:?} vs {b:?}");
                }
            }
        }
    }

    #[test]
    fn pchip_is_monotone_and_honours_the_points() {
        let x = [0.0, 1.0, 2.0, 3.0, 4.0];
        let y = [0.0, 6.0, 6.0, -3.0, 12.0];
        let c = Pchip::new(&x, &y);
        for k in 0..5 {
            assert!((c.eval(x[k]) - y[k]).abs() < 1e-12);
        }
        // the plateau stays flat, nothing overshoots
        for i in 0..=400 {
            let v = c.eval(i as f64 / 100.0);
            assert!((-3.0 - 1e-9..=12.0 + 1e-9).contains(&v));
            if (1.0..=2.0).contains(&(i as f64 / 100.0)) {
                assert!((v - 6.0).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn flat_target_solves_to_zero_and_a_curve_converges() {
        let plan = Plan::new(&OCT, 48000.0);
        let mut t = [0.0; MAX_POINTS];
        let mut g = [1.0; MAX_SECTIONS];
        let mut d = [Biquad { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 }; MAX_SECTIONS];
        plan.targets(&[0.0; 11], &mut t);
        plan.solve(&t, &mut g, &mut d);
        assert!(g.iter().all(|&x| x == 0.0));
        let fad = [3.0, -6.0, 8.0, -4.0, 6.0, 0.0, -9.0, 5.0, -3.0, 7.0, -5.0];
        plan.targets(&fad, &mut t);
        let it = plan.solve(&t, &mut g, &mut d);
        assert!(it <= MAX_COLD_CORRECTIONS);
        for (k, &f) in OCT.iter().enumerate() {
            let r = plan.response_db(&d, f);
            assert!((r - fad[k]).abs() < 0.25, "{f} Hz: {r:.2} vs {}", fad[k]);
        }
    }
}
