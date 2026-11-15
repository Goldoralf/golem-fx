// SPDX-License-Identifier: GPL-3.0-or-later
//! Single-sideband building blocks (gfx-freqshift README §2.2): a Hilbert pair designed per sample
//! rate, the complex shifter, and the same pair used a second time to remove negative frequencies.
//!
//! - [`HilbertDesign`]: the closed-form elliptic polyphase half-band of Valenzuela–Constantinides
//!   (as in de Soras's HIIR), rotated by fs / 4: its two all-pass paths become a Hilbert pair and the
//!   half-band's stopband becomes the image rejection. The band is fixed **in Hz**: 15 Hz to
//!   fs / 2 − 15 Hz, 8 + 8 sections up to 50 kHz, 10 + 10 above. The elliptic nome is computed exactly
//!   (q = exp(−π K′ / K), K by the arithmetic-geometric mean): rejection
//!   6.02 + 5 · (2N + 1) · log₁₀ q dB, −86.8 dB at 48 kHz, −99.7 dB at 96 kHz, −91.6 dB at 192 kHz.
//!   HIIR's short series for q loses 4 – 12 dB at these narrow transitions.
//! - [`HilbertPair`]: path A (even-ranked coefficients, the in-phase output) and path B (odd-ranked,
//!   one sample later, the quadrature). For a positive frequency B·z⁻¹ lags A by 90°, so A x + j·B z⁻¹ x
//!   is the analytic signal of x. Zero latency; both paths share the same dispersion.
//! - [`Allpass`]: path A alone (a dry path matched to what the wet crosses at shift 0).
//! - [`Phasor`]: an f64 phase accumulator with a signed frequency (through zero without a step),
//!   parking at 0 and locking onto another phasor.
//! - [`SsbShifter`]: w = (I + jQ) · (cos φ + j · b · sin φ). `b` = +1 gives the signed shift alone,
//!   −1 its mirror, 0 both sidebands (a carrier-free ring modulator). [`SsbShifter::zero_cut`] passes w
//!   through a second pair as the filter ½ (A + j B z⁻¹): positive frequencies pass, negative ones
//!   (partials shifted below 0 Hz, or wrapped past Nyquist) are stopped at the pair's own rejection.
//!
//! Users: gfx-freqshift; gfx-ringmod's Upper / Lower (an external carrier phase into
//! [`SsbShifter::process`]). Each section is the one-multiply z² all-pass of `pitch.rs`; states in f64.

use std::f64::consts::PI;

/// Largest number of sections per path.
pub const MAX_SECTIONS: usize = 10;
/// The band's edges, in Hz from 0 and from Nyquist.
pub const EDGE_HZ: f64 = 15.0;
/// Up to this rate a path has 8 sections, above it 10.
pub const RATE_8: f64 = 50_000.0;

/// Arithmetic-geometric mean.
fn agm(mut a: f64, mut b: f64) -> f64 {
    for _ in 0..40 {
        let (an, bn) = (0.5 * (a + b), (a * b).sqrt());
        a = an;
        b = bn;
        if (a - b).abs() <= 1e-16 * a {
            break;
        }
    }
    0.5 * (a + b)
}

/// The coefficients of a Hilbert pair: `n` per path, the ascending list split even / odd.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HilbertDesign {
    /// path A (in phase): the 1st, 3rd, 5th… coefficients of the ascending list
    pub a: [f64; MAX_SECTIONS],
    /// path B (quadrature, delayed one sample): the 2nd, 4th…
    pub b: [f64; MAX_SECTIONS],
    /// sections per path
    pub n: usize,
    /// the elliptic nome
    pub q: f64,
}

impl HilbertDesign {
    /// The design for a sample rate: band 15 Hz – fs / 2 − 15 Hz, 8 + 8 sections up to 50 kHz, 10 + 10
    /// above.
    pub fn for_rate(sample_rate: f64) -> Self {
        let n = if sample_rate <= RATE_8 { 8 } else { 10 };
        Self::new(sample_rate, EDGE_HZ, n)
    }

    /// Any band edge (Hz) and section count per path (1 – 10). Closed form, no allocation.
    pub fn new(sample_rate: f64, edge_hz: f64, sections: usize) -> Self {
        let n = sections.clamp(1, MAX_SECTIONS);
        let ncoef = 2 * n;
        // transition width (normalized to fs) of the half-band
        let tbw = (2.0 * edge_hz / sample_rate).clamp(1e-7, 0.45);
        let k = ((1.0 - 2.0 * tbw) * PI / 4.0).tan().powi(2);
        let kp = (1.0 - k * k).sqrt();
        // q = exp(−π K(k′) / K(k)), K(k) = π / (2 agm(1, k′))
        let q = (-PI * agm(1.0, kp) / agm(1.0, k)).exp();
        let order = (2 * ncoef + 1) as f64;
        let mut c = [0.0f64; 2 * MAX_SECTIONS];
        for (idx, out) in c.iter_mut().enumerate().take(ncoef) {
            let m = (idx + 1) as f64;
            let mut num = 0.0;
            let mut sign = 1.0;
            for i in 0..400 {
                let fi = i as f64;
                let p = q.powf(fi * (fi + 1.0));
                num += sign * p * ((2.0 * fi + 1.0) * m * PI / order).sin();
                sign = -sign;
                if p < 1e-300 {
                    break;
                }
            }
            num *= q.powf(0.25);
            let mut den = 0.5;
            let mut sign = -1.0;
            for i in 1..400 {
                let fi = i as f64;
                let p = q.powf(fi * fi);
                den += sign * p * (2.0 * fi * m * PI / order).cos();
                sign = -sign;
                if p < 1e-300 {
                    break;
                }
            }
            let ww = (num / den).powi(2);
            let x = ((1.0 - ww * k) * (1.0 - ww / k)).sqrt() / (1.0 + ww);
            *out = (1.0 - x) / (1.0 + x);
        }
        let mut a = [0.0; MAX_SECTIONS];
        let mut b = [0.0; MAX_SECTIONS];
        for i in 0..n {
            a[i] = c[2 * i];
            b[i] = c[2 * i + 1];
        }
        Self { a, b, n, q }
    }

    /// The image rejection the design guarantees over its band (dB, negative):
    /// 6.02 + 5 · (2N + 1) · log₁₀ q, N = coefficients of both paths.
    pub fn rejection_db(&self) -> f64 {
        let big_n = (2 * self.n) as f64;
        10.0 * 4f64.log10() + 5.0 * (2.0 * big_n + 1.0) * self.q.log10()
    }

    /// Complex responses (A, B·z⁻¹) at `freq` (Hz) as (re, im) pairs. Test and design helper.
    pub fn response(&self, freq: f64, sample_rate: f64) -> ((f64, f64), (f64, f64)) {
        let w = 2.0 * PI * freq / sample_rate;
        let z2 = (( -2.0 * w).cos(), (-2.0 * w).sin());
        let mul = |a: (f64, f64), b: (f64, f64)| (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0);
        let div = |a: (f64, f64), b: (f64, f64)| {
            let d = b.0 * b.0 + b.1 * b.1;
            ((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d)
        };
        let path = |cs: &[f64]| {
            let mut h = (1.0, 0.0);
            for &c in cs {
                h = mul(h, div((c - z2.0, -z2.1), (1.0 - c * z2.0, -c * z2.1)));
            }
            h
        };
        let ha = path(&self.a[..self.n]);
        let hb = mul(path(&self.b[..self.n]), (w.cos(), -w.sin()));
        (ha, hb)
    }

    /// Image level at `freq` (dB re the wanted sideband): min(|A + jB z⁻¹|, |A − jB z⁻¹|) / 2.
    pub fn image_db(&self, freq: f64, sample_rate: f64) -> f64 {
        let (a, b) = self.response(freq, sample_rate);
        // j·b = (−b.im, b.re)
        let p = ((a.0 - b.1).hypot(a.1 + b.0)) * 0.5;
        let m = ((a.0 + b.1).hypot(a.1 - b.0)) * 0.5;
        20.0 * p.min(m).max(1e-300).log10()
    }
}

/// One all-pass cascade of z² sections, y = c (x + y[n−2]) − x[n−2].
#[derive(Clone, Copy, Debug)]
pub struct Allpass {
    c: [f64; MAX_SECTIONS],
    /// per section: x[n−1], x[n−2], y[n−1], y[n−2]
    s: [[f64; 4]; MAX_SECTIONS],
    n: usize,
}

impl Default for Allpass {
    fn default() -> Self {
        Self { c: [0.0; MAX_SECTIONS], s: [[0.0; 4]; MAX_SECTIONS], n: 0 }
    }
}

impl Allpass {
    /// Path A of a design (what the in-phase output crosses).
    pub fn path_a(d: &HilbertDesign) -> Self {
        Self { c: d.a, s: [[0.0; 4]; MAX_SECTIONS], n: d.n }
    }

    /// Path B of a design, without its one-sample delay.
    pub fn path_b(d: &HilbertDesign) -> Self {
        Self { c: d.b, s: [[0.0; 4]; MAX_SECTIONS], n: d.n }
    }

    pub fn reset(&mut self) {
        self.s = [[0.0; 4]; MAX_SECTIONS];
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let mut v = x;
        for i in 0..self.n {
            let [x1, x2, y1, y2] = self.s[i];
            // c · y[n−2] − x[n−2] does not wait for this sample: one multiply-add on the critical path
            let y = self.c[i] * v + (self.c[i] * y2 - x2);
            self.s[i] = [v, x1, y, y1];
            v = y;
        }
        v
    }
}

/// The Hilbert pair: path A, and path B one sample later.
#[derive(Clone, Copy, Debug, Default)]
pub struct HilbertPair {
    a: Allpass,
    b: Allpass,
    d: f64,
}

impl HilbertPair {
    pub fn new(d: &HilbertDesign) -> Self {
        Self { a: Allpass::path_a(d), b: Allpass::path_b(d), d: 0.0 }
    }

    pub fn reset(&mut self) {
        self.a.reset();
        self.b.reset();
        self.d = 0.0;
    }

    /// The two paths on two inputs: (A · u, B z⁻¹ · v).
    #[inline]
    pub fn split(&mut self, u: f64, v: f64) -> (f64, f64) {
        let ya = self.a.process(u);
        let yb = self.b.process(v);
        let q = self.d;
        self.d = yb;
        (ya, q)
    }

    /// The analytic signal of `x`: (in phase, quadrature).
    #[inline]
    pub fn analytic(&mut self, x: f64) -> (f64, f64) {
        self.split(x, x)
    }

    /// The real part of ½ (A + j B z⁻¹) · (u + j v): the positive frequencies of the complex signal
    /// u + j v, its negative ones stopped.
    #[inline]
    pub fn positive(&mut self, u: f64, v: f64) -> f64 {
        let (a, b) = self.split(u, v);
        0.5 * (a - b)
    }
}

/// A phase accumulator in cycles (0 – 1), f64.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Phasor {
    pub phase: f64,
}

/// The shortest signed distance from `from` to `to` on the circle (cycles, −0.5 … 0.5).
#[inline]
pub fn phase_dist(from: f64, to: f64) -> f64 {
    let d = (to - from).rem_euclid(1.0);
    if d > 0.5 {
        d - 1.0
    } else {
        d
    }
}

impl Phasor {
    pub fn reset(&mut self) {
        self.phase = 0.0;
    }

    /// Advances by `hz` (signed) for one sample.
    #[inline]
    pub fn advance(&mut self, hz: f64, sample_rate: f64) {
        self.phase = (self.phase + hz / sample_rate).rem_euclid(1.0);
    }

    /// Moves towards `target` by the shortest way, at most `max_hz` cycles per second; true once on it.
    #[inline]
    pub fn slew_to(&mut self, target: f64, max_hz: f64, sample_rate: f64) -> bool {
        let d = phase_dist(self.phase, target);
        let step = max_hz / sample_rate;
        if d.abs() <= step {
            self.phase = target;
            true
        } else {
            self.phase = (self.phase + step.copysign(d)).rem_euclid(1.0);
            false
        }
    }

    /// (cos φ, sin φ).
    #[inline]
    pub fn cos_sin(&self) -> (f64, f64) {
        let (s, c) = (2.0 * PI * self.phase).sin_cos();
        (c, s)
    }
}

/// The single-sideband shifter: a Hilbert pair, the complex product, and a second pair that removes
/// negative frequencies.
#[derive(Clone, Copy, Debug, Default)]
pub struct SsbShifter {
    pair: HilbertPair,
    cut: HilbertPair,
}

impl SsbShifter {
    pub fn new(d: &HilbertDesign) -> Self {
        Self { pair: HilbertPair::new(d), cut: HilbertPair::new(d) }
    }

    pub fn reset(&mut self) {
        self.pair.reset();
        self.cut.reset();
    }

    /// Clears only the second pass (when it is switched out).
    pub fn reset_cut(&mut self) {
        self.cut.reset();
    }

    /// The complex shifted signal w = (I + jQ) · (cos φ + j · b · sin φ), as (Re w, Im w).
    /// `balance` b: +1 = the signed shift alone, 0 = both sidebands at −6 dB each, −1 = the mirror.
    #[inline]
    pub fn shift(&mut self, x: f64, cos: f64, sin: f64, balance: f64) -> (f64, f64) {
        let (i, q) = self.pair.analytic(x);
        let s = balance * sin;
        (i * cos - q * s, q * cos + i * s)
    }

    /// The second pass: Re ½ (A + j B z⁻¹) w, the negative frequencies of w stopped.
    #[inline]
    pub fn zero_cut(&mut self, w: (f64, f64)) -> f64 {
        self.cut.positive(w.0, w.1)
    }

    /// Shift and take the real part, through the second pass when `zero_cut`.
    #[inline]
    pub fn process(&mut self, x: f64, cos: f64, sin: f64, balance: f64, zero_cut: bool) -> f64 {
        let w = self.shift(x, cos, sin, balance);
        if zero_cut {
            self.zero_cut(w)
        } else {
            w.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATES: [f64; 7] = [44100.0, 48000.0, 50000.0, 88200.0, 96000.0, 176400.0, 192000.0];

    /// The worst image over 15 Hz – fs / 2 − 15 Hz on a dense grid (log near the edges, linear in the
    /// middle), as gfx-freqshift/proto/design.py.
    fn worst_image(d: &HilbertDesign, fs: f64) -> (f64, f64) {
        let lo = EDGE_HZ;
        let mut worst = (-400.0, 0.0);
        let m = 6000;
        let mut test = |f: f64| {
            let v = d.image_db(f, fs);
            if v > worst.0 {
                worst = (v, f);
            }
        };
        for i in 0..m {
            let t = i as f64 / (m - 1) as f64;
            let f = lo * (1000.0 / lo).powf(t);
            test(f);
            test(fs / 2.0 - f);
            test(1000.0 + (fs / 2.0 - 2000.0) * t);
        }
        worst
    }

    /// The proto's figure: 8 + 8 at 48 kHz, exact nome, image −86.8 dB over the band.
    #[test]
    fn image_48k_is_minus_87_db() {
        let d = HilbertDesign::for_rate(48000.0);
        assert_eq!(d.n, 8);
        assert!((d.q - 0.273_805_097_5).abs() < 1e-9, "q {}", d.q);
        let (w, f) = worst_image(&d, 48000.0);
        assert!((w - -86.8).abs() < 0.1, "worst image {w:.2} dB at {f:.0} Hz");
        assert!((d.rejection_db() - -86.8).abs() < 0.05, "formula {:.2}", d.rejection_db());
    }

    /// Every rate (and an odd one): the simulated pair within 0.5 dB of the formula; 8 + 8 up to 50 kHz,
    /// 10 + 10 above; the figures of the design.
    #[test]
    fn image_matches_formula_at_every_rate() {
        let expect = [(44100.0, -87.8), (48000.0, -86.8), (88200.0, -100.8), (96000.0, -99.7), (176400.0, -92.5), (192000.0, -91.6)];
        for fs in RATES {
            let d = HilbertDesign::for_rate(fs);
            assert_eq!(d.n, if fs <= 50000.0 { 8 } else { 10 });
            let (w, f) = worst_image(&d, fs);
            let r = d.rejection_db();
            assert!((w - r).abs() < 0.5, "{fs}: simulated {w:.2} dB at {f:.0} Hz, formula {r:.2}");
            if let Some(&(_, e)) = expect.iter().find(|(r, _)| *r == fs) {
                assert!((r - e).abs() < 0.1, "{fs}: {r:.2} dB, design {e}");
            }
        }
    }

    /// The 48 kHz coefficients, pinned (gfx-freqshift/proto/design.py, mpmath at 40 digits).
    #[test]
    fn coefficients_48k_pinned() {
        let d = HilbertDesign::for_rate(48000.0);
        let a = [0.05147072579185804, 0.3596317083002062, 0.6711289202054751, 0.8539335357073268, 0.9392961949397727, 0.9755671419081565, 0.9905506653324277, 0.9970614444015199];
        let b = [0.18621998827056482, 0.5293680110722544, 0.7781821634338663, 0.9053357860627815, 0.961363415883784, 0.9846789973698934, 0.9944059492278476, 0.9990856432094816];
        for i in 0..8 {
            assert!((d.a[i] - a[i]).abs() < 1e-12, "a[{i}] {} vs {}", d.a[i], a[i]);
            assert!((d.b[i] - b[i]).abs() < 1e-12, "b[{i}] {} vs {}", d.b[i], b[i]);
        }
        // the delayed path is the one whose first coefficient is the larger
        assert!(d.b[0] > d.a[0]);
    }

    /// Path A's group delay at 48 kHz (README §2.4: 10.8 ms at 41 Hz, 0.54 ms at 1 kHz).
    #[test]
    fn group_delay_48k() {
        let d = HilbertDesign::for_rate(48000.0);
        let gd = |f: f64| {
            let df = 0.01;
            let (a1, _) = d.response(f - df, 48000.0);
            let (a2, _) = d.response(f + df, 48000.0);
            let dphi = (a2.1 * a1.0 - a2.0 * a1.1).atan2(a2.0 * a1.0 + a2.1 * a1.1);
            -dphi / (2.0 * PI * 2.0 * df) * 1000.0
        };
        assert!((gd(41.0) - 10.83).abs() < 0.05, "{}", gd(41.0));
        assert!((gd(1000.0) - 0.538).abs() < 0.01, "{}", gd(1000.0));
    }

    fn tone_levels(y: &[f64], fs: f64, freqs: &[f64]) -> Vec<f64> {
        // single-bin DFT with a Blackman-Harris window
        let n = y.len();
        let w = |i: usize| {
            let t = 2.0 * PI * i as f64 / n as f64;
            0.35875 - 0.48829 * t.cos() + 0.14128 * (2.0 * t).cos() - 0.01168 * (3.0 * t).cos()
        };
        freqs
            .iter()
            .map(|&f| {
                let (mut re, mut im) = (0.0, 0.0);
                for (i, &v) in y.iter().enumerate() {
                    let ph = 2.0 * PI * f * i as f64 / fs;
                    re += v * w(i) * ph.cos();
                    im += v * w(i) * ph.sin();
                }
                (re.hypot(im) / (0.35875 * n as f64 * 0.5)).max(1e-300)
            })
            .collect()
    }

    fn run_shift(fs: f64, f: f64, hz: f64, balance: f64, cut: bool) -> Vec<f64> {
        let d = HilbertDesign::for_rate(fs);
        let mut s = SsbShifter::new(&d);
        let mut ph = Phasor::default();
        let n = (1.5 * fs) as usize;
        let mut y = Vec::with_capacity(n);
        for i in 0..n {
            let x = (2.0 * PI * f * i as f64 / fs).sin();
            let (c, sn) = ph.cos_sin();
            y.push(s.process(x, c, sn, balance, cut));
            ph.advance(hz, fs);
        }
        y.split_off(n / 3)
    }

    /// Time domain: +100 Hz on 1 kHz lands at 1,100 Hz, the image at 900 Hz under the design's rejection;
    /// −100 Hz the other way round.
    #[test]
    fn shifter_direction_and_image() {
        for fs in [48000.0, 96000.0] {
            let rej = HilbertDesign::for_rate(fs).rejection_db();
            for (hz, want, img) in [(100.0, 1100.0, 900.0), (-100.0, 900.0, 1100.0)] {
                let y = run_shift(fs, 1000.0, hz, 1.0, false);
                let l = tone_levels(&y, fs, &[want, img]);
                assert!((20.0 * l[0].log10()).abs() < 0.1, "{fs} {hz}: wanted {:.2} dB", 20.0 * l[0].log10());
                let im = 20.0 * (l[1] / l[0]).log10();
                assert!(im < rej + 1.0, "{fs} {hz}: image {im:.1} dB");
            }
        }
    }

    /// Balance 0 = both sidebands at −6 dB; −1 = the mirror alone.
    #[test]
    fn balance_mixes_the_sidebands() {
        let fs = 48000.0;
        let y = run_shift(fs, 1000.0, 200.0, 0.0, false);
        let l = tone_levels(&y, fs, &[1200.0, 800.0]);
        assert!((20.0 * l[0].log10() + 6.02).abs() < 0.1 && (20.0 * (l[0] / l[1]).log10()).abs() < 0.1);
        let y = run_shift(fs, 1000.0, 200.0, -1.0, false);
        let l = tone_levels(&y, fs, &[800.0, 1200.0]);
        assert!(20.0 * (l[1] / l[0]).log10() < -84.0);
    }

    /// The second pass: 100 Hz shifted −150 Hz leaves its 50 Hz reflection at the pair's rejection;
    /// 170 Hz shifted −150 Hz passes at 0 dB; a wrap past Nyquist is removed.
    #[test]
    fn zero_cut_removes_negative_frequencies() {
        let fs = 48000.0;
        let y = run_shift(fs, 100.0, -150.0, 1.0, false);
        let open = tone_levels(&y, fs, &[50.0])[0];
        assert!((20.0 * open.log10()).abs() < 0.2, "without the cut the reflection is the signal");
        let y = run_shift(fs, 100.0, -150.0, 1.0, true);
        let cut = 20.0 * tone_levels(&y, fs, &[50.0])[0].log10();
        assert!(cut < -84.0, "reflection {cut:.1} dB");
        let y = run_shift(fs, 170.0, -150.0, 1.0, true);
        let pass = 20.0 * tone_levels(&y, fs, &[20.0])[0].log10();
        assert!(pass.abs() < 0.5, "20 Hz result {pass:.2} dB");
        // 23 kHz + 2 kHz wraps to 23 kHz, a negative frequency in w
        let y = run_shift(fs, 23000.0, 2000.0, 1.0, true);
        let wrap = 20.0 * tone_levels(&y, fs, &[23000.0])[0].log10();
        assert!(wrap < -84.0, "wrap {wrap:.1} dB");
    }

    /// Phasor: shortest-way slew and the distance.
    #[test]
    fn phasor_slews_the_shortest_way() {
        assert!((phase_dist(0.9, 0.1) - 0.2).abs() < 1e-12);
        assert!((phase_dist(0.1, 0.9) + 0.2).abs() < 1e-12);
        let mut p = Phasor { phase: 0.95 };
        let mut n = 0;
        while !p.slew_to(0.0, 1.0, 1000.0) {
            n += 1;
            assert!(p.phase > 0.9 || p.phase == 0.0);
        }
        assert!((49..=50).contains(&n), "{n} steps");
    }
}
