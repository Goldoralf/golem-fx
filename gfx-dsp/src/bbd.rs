// SPDX-License-Identifier: GPL-3.0-or-later
//! Bucket-brigade delay in the clock domain (Holters & Parker, DAFx 2018; Raffel & Smith, DAFx 2010).
//!
//! A BBD of N stages clocked at f_clk takes one sample per clock period and gives it back N / 2 periods
//! later: delay = N / (2 f_clk). Modulation moves the **clock**, so pitch and tone change as in the chip.
//!
//! The anti-alias and reconstruction filters are continuous-time 4th-order Butterworth low-passes in
//! partial fractions. Their states are integrated exactly between the audio instants and the clock
//! instants:
//! - the input filter is read at each clock tick, from the audio input held over the audio sample;
//! - the BBD output is a staircase that changes at the ticks, and it is integrated exactly into the
//!   output filter.
//! The only aliasing left is the chip's own, which is the sound. The simulation adds none, with no
//! oversampling and zero latency.
//! The filters track the clock (cutoff = `track` · f_clk, capped).

use crate::lfo::Rng;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct C {
    re: f64,
    im: f64,
}

impl C {
    const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }
    #[inline]
    fn mul(self, o: C) -> C {
        C::new(self.re * o.re - self.im * o.im, self.re * o.im + self.im * o.re)
    }
    #[inline]
    fn add(self, o: C) -> C {
        C::new(self.re + o.re, self.im + o.im)
    }
    #[inline]
    fn sub(self, o: C) -> C {
        C::new(self.re - o.re, self.im - o.im)
    }
    #[inline]
    fn scale(self, k: f64) -> C {
        C::new(self.re * k, self.im * k)
    }
    #[inline]
    fn div(self, o: C) -> C {
        let d = o.re * o.re + o.im * o.im;
        C::new((self.re * o.re + self.im * o.im) / d, (self.im * o.re - self.re * o.im) / d)
    }
    /// e^(p·t)
    #[inline]
    fn exp_t(p: C, t: f64) -> C {
        let m = (p.re * t).exp();
        let (s, c) = (p.im * t).sin_cos();
        C::new(m * c, m * s)
    }
}

/// A 4th-order Butterworth low-pass in partial fractions: H(s) = Σ 2 Re(r / (s − p)) over the two
/// upper-half-plane poles.
#[derive(Clone, Copy, Debug, Default)]
struct Bank {
    p: [C; 2],
    r: [C; 2],
    /// e^(p Ts), (e^(p Ts) − 1) / p
    e: [C; 2],
    g: [C; 2],
}

impl Bank {
    fn set(&mut self, fc: f64, ts: f64) {
        let w = std::f64::consts::TAU * fc;
        let all: [C; 4] = core::array::from_fn(|k| {
            let th = std::f64::consts::FRAC_PI_2 + (2 * k + 1) as f64 * std::f64::consts::PI / 8.0;
            C::new(w * th.cos(), w * th.sin())
        });
        // upper half plane: k = 0 (5π/8) and 1 (7π/8)
        let w4 = w * w * w * w;
        for i in 0..2 {
            let mut den = C::new(1.0, 0.0);
            for (j, pj) in all.iter().enumerate() {
                if j != i {
                    den = den.mul(all[i].sub(*pj));
                }
            }
            self.p[i] = all[i];
            self.r[i] = C::new(w4, 0.0).div(den);
            self.e[i] = C::exp_t(all[i], ts);
            self.g[i] = self.e[i].sub(C::new(1.0, 0.0)).div(all[i]);
        }
    }
}

#[derive(Clone, Debug)]
pub struct Bbd {
    fifo: Vec<f32>,
    len: usize,
    w: usize,
    sr: f64,
    ts: f64,
    phase: f64,
    inc: f64,
    fc_in: f64,
    input: Bank,
    output: Bank,
    x: [C; 2],
    z: [C; 2],
    held: f32,
    track: f64,
    fc_max: f64,
    noise: f32,
    rng: Rng,
}

impl Bbd {
    /// `stages`: the chip's stages (MN3007 1024, MN3207 1024, MN3009 256, SAD1024 512 per half).
    pub fn new(stages: usize) -> Self {
        let len = (stages / 2).max(1);
        Self {
            fifo: vec![0.0; len],
            len,
            w: 0,
            sr: 48000.0,
            ts: 1.0 / 48000.0,
            phase: 0.0,
            inc: 1.0,
            fc_in: 0.0,
            input: Bank::default(),
            output: Bank::default(),
            x: Default::default(),
            z: Default::default(),
            held: 0.0,
            track: 0.45,
            fc_max: 10000.0,
            noise: 0.0,
            rng: Rng::new(0xBBD),
        }
    }

    /// Filter law: cutoff = `track` · f_clk, never above `fc_max`.
    pub fn set_filters(&mut self, track: f64, fc_max: f64) {
        self.track = track;
        self.fc_max = fc_max;
    }

    /// Clock noise / hiss, linear amplitude (0 = none).
    pub fn set_noise(&mut self, amp: f32) {
        self.noise = amp;
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sr = sample_rate as f64;
        self.ts = 1.0 / self.sr;
        self.fc_in = 0.0;
        self.set_delay(0.005);
    }

    pub fn reset(&mut self) {
        self.fifo.iter_mut().for_each(|v| *v = 0.0);
        self.w = 0;
        self.phase = 0.0;
        self.x = Default::default();
        self.z = Default::default();
        self.held = 0.0;
        self.rng.reset();
    }

    /// The shortest delay the engine can run at this sample rate (at most 64 ticks per sample).
    pub fn min_delay(&self) -> f64 {
        self.len as f64 / (64.0 * self.sr)
    }

    /// Sets the delay in seconds by moving the clock.
    pub fn set_delay(&mut self, seconds: f64) {
        let d = seconds.max(self.min_delay());
        let f_clk = self.len as f64 / d;
        self.inc = f_clk / self.sr;
        let fc = (self.track * f_clk).min(self.fc_max).min(0.45 * self.sr);
        // the poles move only when the cutoff moves by more than 0.1 %
        if (fc / self.fc_in.max(1.0) - 1.0).abs() > 1e-3 {
            self.fc_in = fc;
            self.input.set(fc, self.ts);
            self.output.set(fc, self.ts);
        }
    }

    /// The clock frequency (Hz).
    pub fn clock(&self) -> f64 {
        self.inc * self.sr
    }

    #[inline]
    pub fn process(&mut self, u: f32) -> f32 {
        let u = u as f64;
        let start = self.held as f64;
        let mut acc = [C::default(); 2];
        // ticks inside this sample, at fractions δ of the sample
        let mut t = (1.0 - self.phase) / self.inc;
        while t < 1.0 {
            // input filter at δ = t: e^(p δ Ts) x + (e^(p δ Ts) − 1) / p · u
            let mut y = 0.0;
            for i in 0..2 {
                let e = C::exp_t(self.input.p[i], t * self.ts);
                let g = e.sub(C::new(1.0, 0.0)).div(self.input.p[i]);
                let xs = e.mul(self.x[i]).add(g.scale(u));
                y += 2.0 * self.input.r[i].mul(xs).re;
            }
            let mut v = y as f32;
            if self.noise > 0.0 {
                v += self.noise * self.rng.bipolar();
            }
            let out = self.fifo[self.w];
            self.fifo[self.w] = v;
            self.w += 1;
            if self.w == self.len {
                self.w = 0;
            }
            // the output staircase steps by (out − held) at δ: its exact share of this sample
            let dv = (out - self.held) as f64;
            for i in 0..2 {
                let e = C::exp_t(self.output.p[i], (1.0 - t) * self.ts);
                let g = e.sub(C::new(1.0, 0.0)).div(self.output.p[i]);
                acc[i] = acc[i].add(g.scale(dv));
            }
            self.held = out;
            t += 1.0 / self.inc;
        }
        self.phase = (self.phase + self.inc).fract();
        let mut y = 0.0;
        for i in 0..2 {
            self.x[i] = self.input.e[i].mul(self.x[i]).add(self.input.g[i].scale(u));
            self.z[i] = self.output.e[i].mul(self.z[i]).add(self.output.g[i].scale(start)).add(acc[i]);
            y += 2.0 * self.output.r[i].mul(self.z[i]).re;
        }
        y as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(b: &mut Bbd, x: &[f32]) -> Vec<f32> {
        x.iter().map(|v| b.process(*v)).collect()
    }

    #[test]
    fn delay_is_n_over_two_f_clk() {
        let mut b = Bbd::new(1024);
        b.prepare(48000.0);
        b.set_delay(0.005);
        b.reset();
        let mut x = vec![0.0f32; 4800];
        x[100] = 1.0;
        let y = run(&mut b, &x);
        // centre of mass of the response
        let (m, s): (f64, f64) = y.iter().enumerate().fold((0.0, 0.0), |(m, s), (i, v)| (m + i as f64 * v.abs() as f64, s + v.abs() as f64));
        let delay_ms = (m / s - 100.0) / 48.0;
        // 5 ms plus the two filters' group delay (≈ 2 × 0.2 ms at ~5 kHz)
        assert!(delay_ms > 5.0 && delay_ms < 5.8, "{delay_ms:.3} ms");
    }

    #[test]
    fn unity_gain_in_band() {
        let mut b = Bbd::new(1024);
        b.prepare(48000.0);
        b.set_delay(0.003);
        b.reset();
        let x: Vec<f32> = (0..48000).map(|i| 0.5 * (std::f32::consts::TAU * 300.0 * i as f32 / 48000.0).sin()).collect();
        let y = run(&mut b, &x);
        let rms = |v: &[f32]| (v.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt();
        let db = 20.0 * (rms(&y[24000..]) / rms(&x[24000..])).log10();
        assert!(db.abs() < 0.3, "{db:.2} dB");
    }

    #[test]
    fn darker_at_long_delays() {
        let hf = |d: f64| {
            let mut b = Bbd::new(1024);
            b.prepare(48000.0);
            b.set_delay(d);
            b.reset();
            let x: Vec<f32> = (0..24000).map(|i| 0.5 * (std::f32::consts::TAU * 6000.0 * i as f32 / 48000.0).sin()).collect();
            let y = run(&mut b, &x);
            y[12000..].iter().fold(0.0f32, |m, v| m.max(v.abs()))
        };
        assert!(hf(0.002) > 0.4, "short delay keeps 6 kHz: {}", hf(0.002));
        assert!(hf(0.080) < 0.05, "long delay (6.4 kHz clock) removes it: {}", hf(0.080));
    }

    #[test]
    fn modulated_clock_is_finite_and_smooth() {
        let mut b = Bbd::new(1024);
        b.prepare(44100.0);
        b.reset();
        let mut last = 0.0f32;
        for i in 0..88200 {
            let d = 0.004 + 0.003 * (std::f64::consts::TAU * 3.0 * i as f64 / 44100.0).sin();
            b.set_delay(d);
            let x = 0.5 * (std::f32::consts::TAU * 440.0 * i as f32 / 44100.0).sin();
            let y = b.process(x);
            assert!(y.is_finite());
            assert!((y - last).abs() < 0.2, "jump at {i}");
            last = y;
        }
    }
}
