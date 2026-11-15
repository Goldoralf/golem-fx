// SPDX-License-Identifier: GPL-3.0-or-later
//! 2× oversampling with polyphase IIR half-band filters (Laurent de Soras' HIIR design: two parallel
//! chains of first-order allpasses). Minimum phase: no reported latency, a few samples of group delay
//! at low frequencies. A dry signal that must stay phase-coherent with a processed one goes through
//! the same up / down pair ([`Oversampler2x::identity`]).

use std::f64::consts::PI;

/// Allpass coefficients of a half-band filter with `n` coefficients and a transition band
/// `transition` (fraction of the sample rate, 0 – 0.5). HIIR's `PolyphaseIir2Designer::compute_coefs`.
pub fn halfband_coefs(n: usize, transition: f64) -> Vec<f64> {
    let (k, q) = transition_params(transition);
    let order = n * 2 + 1;
    (0..n).map(|i| coef(i, k, q, order)).collect()
}

fn transition_params(transition: f64) -> (f64, f64) {
    let k = ((1.0 - transition * 2.0) * PI / 4.0).tan().powi(2);
    let kksqrt = (1.0 - k * k).powf(0.25);
    let e = 0.5 * (1.0 - kksqrt) / (1.0 + kksqrt);
    let e2 = e * e;
    let e4 = e2 * e2;
    let q = e * (1.0 + e4 * (2.0 + e4 * (15.0 + 150.0 * e4)));
    (k, q)
}

fn coef(index: usize, k: f64, q: f64, order: usize) -> f64 {
    let c = (index + 1) as f64;
    let num = acc_num(q, order, c) * q.powf(0.25);
    let den = acc_den(q, order, c) + 0.5;
    let ww = num / den;
    let wwsq = ww * ww;
    let x = ((1.0 - wwsq * k) * (1.0 - wwsq / k)).sqrt() / (1.0 + wwsq);
    (1.0 - x) / (1.0 + x)
}

fn acc_num(q: f64, order: usize, c: f64) -> f64 {
    let (mut i, mut j, mut acc) = (0i32, 1.0, 0.0);
    loop {
        let t = q.powi(i * (i + 1)) * (((i * 2 + 1) as f64) * c * PI / order as f64).sin() * j;
        acc += t;
        j = -j;
        i += 1;
        if t.abs() <= 1e-100 || i > 200 {
            return acc;
        }
    }
}

fn acc_den(q: f64, order: usize, c: f64) -> f64 {
    let (mut i, mut j, mut acc) = (1i32, -1.0, 0.0);
    loop {
        let t = q.powi(i * i) * ((i * 2) as f64 * c * PI / order as f64).cos() * j;
        acc += t;
        j = -j;
        i += 1;
        if t.abs() <= 1e-100 || i > 200 {
            return acc;
        }
    }
}

/// The two allpass chains; coefficient `i` goes to chain `i % 2`.
#[derive(Clone, Debug)]
struct Chains {
    coefs: Vec<f64>,
    x: Vec<f64>,
    y: Vec<f64>,
}

impl Chains {
    fn new(coefs: Vec<f64>) -> Self {
        let n = coefs.len();
        Self { coefs, x: vec![0.0; n], y: vec![0.0; n] }
    }

    fn reset(&mut self) {
        self.x.iter_mut().for_each(|v| *v = 0.0);
        self.y.iter_mut().for_each(|v| *v = 0.0);
    }

    /// HIIR `StageProcFpu::process_sample_pos`: `a` runs through the even coefficients, `b` the odd.
    #[inline]
    fn process(&mut self, mut a: f64, mut b: f64) -> (f64, f64) {
        let n = self.coefs.len();
        let mut i = 0;
        while i + 1 < n {
            let t0 = self.x[i];
            let t1 = self.x[i + 1];
            self.x[i] = a;
            self.x[i + 1] = b;
            a = (a - self.y[i]) * self.coefs[i] + t0;
            b = (b - self.y[i + 1]) * self.coefs[i + 1] + t1;
            self.y[i] = a;
            self.y[i + 1] = b;
            i += 2;
        }
        if i < n {
            let t0 = self.x[i];
            self.x[i] = a;
            a = (a - self.y[i]) * self.coefs[i] + t0;
            self.y[i] = a;
        }
        (a, b)
    }
}

/// One channel of 2× up- and down-sampling.
#[derive(Clone, Debug)]
pub struct Oversampler2x {
    up: Chains,
    down: Chains,
}

/// Default design: 8 coefficients, transition 0.04 → stop-band rejection beyond 100 dB.
pub const DEFAULT_COEFS: usize = 8;
pub const DEFAULT_TRANSITION: f64 = 0.04;

impl Oversampler2x {
    pub fn new() -> Self {
        let c = halfband_coefs(DEFAULT_COEFS, DEFAULT_TRANSITION);
        Self { up: Chains::new(c.clone()), down: Chains::new(c) }
    }

    pub fn reset(&mut self) {
        self.up.reset();
        self.down.reset();
    }

    /// One input sample → two samples at twice the rate.
    #[inline]
    pub fn up(&mut self, x: f32) -> [f32; 2] {
        let (a, b) = self.up.process(x as f64, x as f64);
        [a as f32, b as f32]
    }

    /// Two samples at twice the rate → one output sample.
    #[inline]
    pub fn down(&mut self, x: [f32; 2]) -> f32 {
        let (a, b) = self.down.process(x[1] as f64, x[0] as f64);
        (0.5 * (a + b)) as f32
    }

    /// Copies another stage's state without allocating (same design).
    pub fn copy_from(&mut self, other: &Oversampler2x) {
        for (a, b) in [(&mut self.up, &other.up), (&mut self.down, &other.down)] {
            if a.x.len() == b.x.len() {
                a.x.copy_from_slice(&b.x);
                a.y.copy_from_slice(&b.y);
            }
        }
    }

    /// Up then down with nothing in between: the same group delay as a processed path.
    #[inline]
    pub fn identity(&mut self, x: f32) -> f32 {
        let u = self.up(x);
        self.down(u)
    }
}

impl Default for Oversampler2x {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn through(freq: f64, sr: f64) -> f64 {
        let mut o = Oversampler2x::new();
        let n = 48000;
        let w = 2.0 * PI * freq / sr;
        let (mut ein, mut eout) = (0.0, 0.0);
        for i in 0..n {
            let x = (w * i as f64).sin() as f32;
            let y = o.identity(x);
            if i > n / 2 {
                ein += (x as f64).powi(2);
                eout += (y as f64).powi(2);
            }
        }
        10.0 * (eout / ein).log10()
    }

    #[test]
    fn passband_is_flat() {
        for f in [100.0, 1000.0, 10000.0, 18000.0] {
            let g = through(f, 48000.0);
            assert!(g.abs() < 0.05, "{f} Hz: {g} dB");
        }
    }

    #[test]
    fn rejects_images_and_aliases() {
        // a tone at 30 kHz on the doubled rate (above the base Nyquist) must not come back down
        let mut o = Oversampler2x::new();
        let w = 2.0 * PI * 30000.0 / 96000.0;
        let mut peak = 0.0f32;
        for i in 0..48000 {
            let a = (w * (2 * i) as f64).sin() as f32;
            let b = (w * (2 * i + 1) as f64).sin() as f32;
            let y = o.down([a, b]);
            if i > 24000 {
                peak = peak.max(y.abs());
            }
        }
        let db = 20.0 * peak.log10();
        assert!(db < -90.0, "alias at {db} dB");
    }
}

#[cfg(test)]
mod order_test {
    use super::*;

    fn probe(y: &[f32], f: f64, sr: f64) -> f64 {
        let w = 2.0 * PI * f / sr;
        let (mut re, mut im) = (0.0, 0.0);
        for (i, v) in y.iter().enumerate() {
            let win = 0.5 - 0.5 * (2.0 * PI * i as f64 / y.len() as f64).cos();
            re += *v as f64 * win * (w * i as f64).cos();
            im += *v as f64 * win * (w * i as f64).sin();
        }
        (re * re + im * im).sqrt()
    }

    #[test]
    fn upsampled_stream_has_no_image() {
        let mut o = Oversampler2x::new();
        let mut up = Vec::new();
        for i in 0..8192 {
            let x = (2.0 * PI * 5000.0 * i as f64 / 48000.0).sin() as f32;
            up.extend_from_slice(&o.up(x));
        }
        let s = &up[4096..];
        let main = probe(s, 5000.0, 96000.0);
        let image = probe(s, 43000.0, 96000.0);
        let db = 20.0 * (image / main).log10();
        assert!(db < -90.0, "image at {db:.1} dB");
    }
}

