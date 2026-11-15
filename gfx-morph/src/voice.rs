// SPDX-License-Identifier: GPL-3.0-or-later
//! The reference signals of the build-time tables and of the tests (README §2.5, `proto/common.py`'s
//! source): a synthetic sung vowel (a band-limited glottal-derivative source, cosine-phase harmonics at
//! 1 / k, through four formant resonators, then a 70 Hz Butterworth high-pass), sibilants, pink noise.
//! Plain `std`: build.rs includes this file.

#![allow(dead_code, clippy::needless_range_loop)]

use std::f64::consts::PI;

/// Formants (Hz) and bandwidths (Hz) of the reference vowels (textbook values, Peterson & Barney style).
pub const VOWEL_A: ([f64; 4], [f64; 4]) = ([730.0, 1090.0, 2440.0, 3400.0], [90.0, 110.0, 160.0, 250.0]);
pub const VOWEL_O: ([f64; 4], [f64; 4]) = ([570.0, 840.0, 2410.0, 3400.0], [80.0, 90.0, 160.0, 250.0]);
pub const VOWEL_E: ([f64; 4], [f64; 4]) = ([530.0, 1840.0, 2480.0, 3500.0], [80.0, 100.0, 160.0, 250.0]);

/// The reference vowel's pitch (Hz) and level (dBFS RMS).
pub const REF_F0: f64 = 187.3;
pub const REF_DB: f64 = -18.0;

/// A direct-form biquad (b, a with a0 = 1), f64.
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    pub b: [f64; 3],
    pub a: [f64; 3],
    z: [f64; 2],
}

impl Biquad {
    pub fn new(b: [f64; 3], a: [f64; 3]) -> Self {
        Self { b: [b[0] / a[0], b[1] / a[0], b[2] / a[0]], a: [1.0, a[1] / a[0], a[2] / a[0]], z: [0.0; 2] }
    }

    /// A 2nd-order Butterworth high-pass (bilinear, prewarped: scipy's `butter(2, f, 'high')`).
    pub fn butter_hp(f: f64, fs: f64) -> Self {
        let k = (PI * f / fs).tan();
        let n = 1.0 / (1.0 + std::f64::consts::SQRT_2 * k + k * k);
        Self::new([n, -2.0 * n, n], [1.0, 2.0 * (k * k - 1.0) * n, (1.0 - std::f64::consts::SQRT_2 * k + k * k) * n])
    }

    /// A formant resonator: two poles at radius e^(−π B / fs), unity at DC.
    pub fn formant(f: f64, bw: f64, fs: f64) -> Self {
        let r = (-PI * bw / fs).exp();
        let a = [1.0, -2.0 * r * (2.0 * PI * f / fs).cos(), r * r];
        Self::new([a[0] + a[1] + a[2], 0.0, 0.0], a)
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[1] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[2] * y;
        y
    }

    /// |H(e^jω)|² at `f`.
    pub fn mag2(&self, f: f64, fs: f64) -> f64 {
        let w = 2.0 * PI * f / fs;
        let (c1, s1, c2, s2) = (w.cos(), -w.sin(), (2.0 * w).cos(), -(2.0 * w).sin());
        let nr = self.b[0] + self.b[1] * c1 + self.b[2] * c2;
        let ni = self.b[1] * s1 + self.b[2] * s2;
        let dr = 1.0 + self.a[1] * c1 + self.a[2] * c2;
        let di = self.a[1] * s1 + self.a[2] * s2;
        (nr * nr + ni * ni) / (dr * dr + di * di)
    }
}

/// The highest harmonic of the source: every partial below 0.45 fs.
fn kmax(f0: f64, fs: f64) -> usize {
    (0.45 * fs / f0) as usize
}

/// A sung vowel: `n` samples at `fs`, pitch `f0`, normalised to `level_db` dBFS RMS.
pub fn vowel(f0: f64, n: usize, formants: ([f64; 4], [f64; 4]), level_db: f64, fs: f64) -> Vec<f64> {
    let km = kmax(f0, fs);
    let w = 2.0 * PI * f0 / fs;
    let mut res: Vec<Biquad> = (0..4).map(|i| Biquad::formant(formants.0[i], formants.1[i], fs)).collect();
    let mut hp = Biquad::butter_hp(70.0, fs);
    let mut y = Vec::with_capacity(n);
    for i in 0..n {
        // Σ cos(k θ) / k by the Chebyshev recurrence
        let c1 = (w * i as f64).cos();
        let (mut cm, mut c) = (1.0, c1);
        let mut s = 0.0;
        for k in 1..=km {
            s += c / k as f64;
            let next = 2.0 * c1 * c - cm;
            cm = c;
            c = next;
        }
        let mut v = s;
        for r in res.iter_mut() {
            v = r.process(v);
        }
        y.push(hp.process(v));
    }
    let rms = (y.iter().map(|v| v * v).sum::<f64>() / n as f64).sqrt().max(1e-30);
    let g = 10f64.powf(level_db / 20.0) / rms;
    y.iter_mut().for_each(|v| *v *= g);
    y
}

/// The steady-state power gain (dB) of a response `h2(f)` = |H(f)|² for a vowel at `f0`: the source's
/// harmonics through the formants and the 70 Hz high-pass, weighted by h2.
pub fn vowel_gain_db(f0: f64, formants: ([f64; 4], [f64; 4]), fs: f64, h2: impl Fn(f64) -> f64) -> f64 {
    let res: Vec<Biquad> = (0..4).map(|i| Biquad::formant(formants.0[i], formants.1[i], fs)).collect();
    let hp = Biquad::butter_hp(70.0, fs);
    let (mut pin, mut pout) = (0.0, 0.0);
    for k in 1..=kmax(f0, fs) {
        let f = k as f64 * f0;
        let mut p = 1.0 / (k * k) as f64;
        for r in &res {
            p *= r.mag2(f, fs);
        }
        p *= hp.mag2(f, fs);
        pin += p;
        pout += p * h2(f);
    }
    10.0 * (pout / pin).log10()
}

/// A xorshift generator (64-bit), uniform on [−1, 1).
#[derive(Clone, Copy, Debug)]
pub struct Xor(pub u64);

impl Xor {
    #[inline]
    pub fn next_f64(&mut self) -> f64 {
        let mut s = self.0;
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        self.0 = s;
        (s >> 11) as f64 / (1u64 << 52) as f64 - 1.0
    }
}

/// Pink noise (Kellet's three-pole filter on white), normalised to `level_db` dBFS RMS.
pub fn pink(n: usize, level_db: f64, seed: u64) -> Vec<f64> {
    let b = [0.049922035, -0.095993537, 0.050612699, -0.004408786];
    let a = [1.0, -2.494956002, 2.017265875, -0.522189400];
    let mut rng = Xor(seed.max(1));
    let (mut x, mut y) = ([0.0f64; 4], [0.0f64; 4]);
    let mut out = Vec::with_capacity(n);
    for _ in 0..n + 4800 {
        x.rotate_right(1);
        y.rotate_right(1);
        x[0] = rng.next_f64();
        y[0] = b[0] * x[0] + b[1] * x[1] + b[2] * x[2] + b[3] * x[3] - a[1] * y[1] - a[2] * y[2] - a[3] * y[3];
        out.push(y[0]);
    }
    let out: Vec<f64> = out.split_off(4800);
    let rms = (out.iter().map(|v| v * v).sum::<f64>() / n as f64).sqrt().max(1e-30);
    let g = 10f64.powf(level_db / 20.0) / rms;
    out.into_iter().map(|v| v * g).collect()
}

/// Band noise: white through a 2nd-order Butterworth high-pass at `lo` and low-pass at `hi`, `level_db`
/// dBFS RMS (a breathy vowel's aspiration, r2's 2 – 8 kHz).
pub fn band_noise(n: usize, lo: f64, hi: f64, level_db: f64, fs: f64, seed: u64) -> Vec<f64> {
    let mut rng = Xor(seed.max(1));
    let mut hp = Biquad::butter_hp(lo, fs);
    let k = (PI * hi / fs).tan();
    let nn = 1.0 / (1.0 + std::f64::consts::SQRT_2 * k + k * k);
    let mut lp = Biquad::new([k * k * nn, 2.0 * k * k * nn, k * k * nn], [1.0, 2.0 * (k * k - 1.0) * nn, (1.0 - std::f64::consts::SQRT_2 * k + k * k) * nn]);
    let mut y: Vec<f64> = (0..n).map(|_| lp.process(hp.process(rng.next_f64()))).collect();
    let rms = (y.iter().map(|v| v * v).sum::<f64>() / n.max(1) as f64).sqrt().max(1e-30);
    let g = 10f64.powf(level_db / 20.0) / rms;
    y.iter_mut().for_each(|v| *v *= g);
    y
}

/// A sibilant: band noise 4 – 10 kHz (four poles each side) with 10 ms ramps, `level_db` dBFS RMS.
pub fn sibilant(n: usize, level_db: f64, fs: f64, seed: u64) -> Vec<f64> {
    let mut rng = Xor(seed.max(1));
    let mut hp = [Biquad::butter_hp(4000.0, fs), Biquad::butter_hp(4000.0, fs)];
    let lp = |f: f64| {
        let k = (PI * f / fs).tan();
        let nn = 1.0 / (1.0 + std::f64::consts::SQRT_2 * k + k * k);
        Biquad::new([k * k * nn, 2.0 * k * k * nn, k * k * nn], [1.0, 2.0 * (k * k - 1.0) * nn, (1.0 - std::f64::consts::SQRT_2 * k + k * k) * nn])
    };
    let mut lps = [lp(10000.0), lp(10000.0)];
    let ramp = (0.01 * fs) as usize;
    let mut y: Vec<f64> = (0..n)
        .map(|i| {
            let mut v = rng.next_f64();
            for f in hp.iter_mut().chain(lps.iter_mut()) {
                v = f.process(v);
            }
            v * (i.min(n - 1 - i) as f64 / ramp as f64).min(1.0)
        })
        .collect();
    let mid = &y[ramp.min(n / 2)..n - ramp.min(n / 2)];
    let rms = (mid.iter().map(|v| v * v).sum::<f64>() / mid.len().max(1) as f64).sqrt().max(1e-30);
    let g = 10f64.powf(level_db / 20.0) / rms;
    y.iter_mut().for_each(|v| *v *= g);
    y
}
