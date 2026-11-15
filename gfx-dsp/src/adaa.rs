// SPDX-License-Identifier: GPL-3.0-or-later
//! First-order antiderivative antialiasing (Parker, Zavalishin & Le Bivic, DAFx 2016; Bilbao, Esqueda,
//! Parker & Välimäki, IEEE SPL 2017) for memoryless shapers:
//! y[n] = (F(x[n]) − F(x[n−1])) / (x[n] − x[n−1]), F the antiderivative of the shaper.
//! It adds a fixed half-sample delay; paths blended with a dry must match it ([`HalfSample`]).

/// A memoryless shaper with a closed-form antiderivative.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shaper {
    /// tanh(x): odd harmonics.
    Tanh,
    /// tanh(x + b) − tanh(b): asymmetric, even harmonics, zero at zero.
    Asym(f64),
    /// clamp(x, −1, 1): the hard rail (gfx-radio's transistor amps and TX limiters). F = x² / 2 inside,
    /// |x| − ½ outside.
    Hard,
}

#[inline]
fn ln_cosh(x: f64) -> f64 {
    let a = x.abs();
    a + (-2.0 * a).exp().ln_1p() - std::f64::consts::LN_2
}

impl Shaper {
    #[inline]
    pub fn f(self, x: f64) -> f64 {
        match self {
            Shaper::Tanh => x.tanh(),
            Shaper::Asym(b) => (x + b).tanh() - b.tanh(),
            Shaper::Hard => x.clamp(-1.0, 1.0),
        }
    }

    #[inline]
    pub fn antiderivative(self, x: f64) -> f64 {
        match self {
            Shaper::Tanh => ln_cosh(x),
            Shaper::Asym(b) => ln_cosh(x + b) - x * b.tanh(),
            Shaper::Hard => hard_integral(x, 1.0),
        }
    }
}

/// The antiderivative of clamp(x, −c, c): x² / 2 inside, c·|x| − c² / 2 outside (C¹).
#[inline]
fn hard_integral(x: f64, c: f64) -> f64 {
    let a = x.abs();
    if a <= c {
        0.5 * x * x
    } else {
        c * a - 0.5 * c * c
    }
}

/// A hard clip at ±`c` whose ceiling may move every sample (a sagging rail), for [`Adaa1Fn`].
#[derive(Clone, Copy, Debug)]
pub struct HardClip {
    pub c: f64,
}

impl Antiderivative for HardClip {
    #[inline]
    fn f(&self, x: f64) -> f64 {
        x.clamp(-self.c, self.c)
    }
    #[inline]
    fn antiderivative(&self, x: f64) -> f64 {
        hard_integral(x, self.c)
    }
}

/// ADAA1 state for one channel.
#[derive(Clone, Debug, Default)]
pub struct Adaa1 {
    x1: f64,
    f1: f64,
    last: Option<Shaper>,
}

impl Adaa1 {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self, shaper: Shaper) {
        self.x1 = 0.0;
        self.f1 = shaper.antiderivative(0.0);
        self.last = Some(shaper);
    }

    #[inline]
    pub fn process(&mut self, shaper: Shaper, x: f32) -> f32 {
        let x = x as f64;
        if Some(shaper) != self.last {
            // a different shaper: its antiderivative at the previous input, or the difference spikes
            self.f1 = shaper.antiderivative(self.x1);
            self.last = Some(shaper);
        }
        let fx = shaper.antiderivative(x);
        let d = x - self.x1;
        let y = if d.abs() < 1e-6 { shaper.f(0.5 * (x + self.x1)) } else { (fx - self.f1) / d };
        self.x1 = x;
        self.f1 = fx;
        y as f32
    }
}

/// The same half-sample delay as ADAA1, for a path that must stay aligned with a shaped one.
#[derive(Clone, Debug, Default)]
pub struct HalfSample {
    x1: f32,
}

impl HalfSample {
    pub fn reset(&mut self) {
        self.x1 = 0.0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = 0.5 * (x + self.x1);
        self.x1 = x;
        y
    }
}


/// A memoryless function with a closed-form antiderivative, for [`Adaa1Fn`] (shapers whose parameters
/// move: both antiderivatives are taken with the current parameters, so a change never spikes).
pub trait Antiderivative {
    fn f(&self, x: f64) -> f64;
    fn antiderivative(&self, x: f64) -> f64;
}

/// ADAA1 for any [`Antiderivative`]: y = (F(x) − F(x₁)) / (x − x₁), the midpoint value when the step
/// is tiny. Half a sample of delay, as [`Adaa1`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Adaa1Fn {
    x1: f64,
}

impl Adaa1Fn {
    pub fn reset(&mut self) {
        self.x1 = 0.0;
    }

    #[inline]
    pub fn process<G: Antiderivative>(&mut self, g: &G, x: f64) -> f64 {
        let d = x - self.x1;
        let y = if d.abs() < 1e-5 * (1.0 + x.abs()) { g.f(0.5 * (x + self.x1)) } else { (g.antiderivative(x) - g.antiderivative(self.x1)) / d };
        self.x1 = x;
        y
    }
}

/// The residual of a driven tanh, tanh(k x) / k − x: added to x, it gives the saturated signal while x
/// itself passes straight (no ADAA roll-off on the linear path; gfx-ringmod `drive`).
#[derive(Clone, Copy, Debug)]
pub struct TanhResidual {
    pub k: f64,
}

impl Antiderivative for TanhResidual {
    #[inline]
    fn f(&self, x: f64) -> f64 {
        (self.k * x).tanh() / self.k - x
    }
    #[inline]
    fn antiderivative(&self, x: f64) -> f64 {
        ln_cosh(self.k * x) / (self.k * self.k) - 0.5 * x * x
    }
}

/// Parker's diode law (*A simple digital model of the diode-based ring-modulator*, DAFx 2011): 0 below
/// `vb`, a quadratic knee h (v − vb)² / (2 vl − 2 vb) up to `vl`, slope h above. C¹.
#[derive(Clone, Copy, Debug)]
pub struct ParkerDiode {
    pub vb: f64,
    pub vl: f64,
    pub h: f64,
}

impl ParkerDiode {
    pub const fn new(vb: f64, vl: f64, h: f64) -> Self {
        Self { vb, vl, h }
    }

    #[inline]
    pub fn d(&self, v: f64) -> f64 {
        if v <= self.vb {
            0.0
        } else if v <= self.vl {
            self.h * (v - self.vb).powi(2) / (2.0 * (self.vl - self.vb))
        } else {
            self.h * (v - self.vl) + 0.5 * self.h * (self.vl - self.vb)
        }
    }

    /// The integral of D from 0 to v: a piecewise cubic / quadratic, continuous.
    #[inline]
    pub fn integral(&self, v: f64) -> f64 {
        let w = self.vl - self.vb;
        if v <= self.vb {
            0.0
        } else if v <= self.vl {
            self.h * (v - self.vb).powi(3) / (6.0 * w)
        } else {
            let u = v - self.vl;
            0.5 * self.h * u * u + 0.5 * self.h * w * u + self.h * w * w / 6.0
        }
    }
}

/// One branch of Parker's ring: G(v) = D₁(v) + D₂(−v), a pair of opposed diodes (even when matched).
#[derive(Clone, Copy, Debug)]
pub struct DiodePair {
    pub d1: ParkerDiode,
    pub d2: ParkerDiode,
}

impl Antiderivative for DiodePair {
    #[inline]
    fn f(&self, v: f64) -> f64 {
        self.d1.d(v) + self.d2.d(-v)
    }
    #[inline]
    fn antiderivative(&self, v: f64) -> f64 {
        self.d1.integral(v) - self.d2.integral(-v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Energy of components that are not harmonics of `f0`, relative to the total (dB).
    fn inharmonic_db(y: &[f32], f0: f64, sr: f64) -> f64 {
        let n = y.len();
        let mut total = 0.0;
        let mut harmonic = 0.0;
        let bins = 4096;
        for k in 1..bins {
            let f = k as f64 * sr / 2.0 / bins as f64;
            let w = 2.0 * std::f64::consts::PI * f / sr;
            let (mut re, mut im) = (0.0, 0.0);
            for (i, &v) in y.iter().enumerate() {
                let win = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos();
                re += v as f64 * win * (w * i as f64).cos();
                im += v as f64 * win * (w * i as f64).sin();
            }
            let p = re * re + im * im;
            total += p;
            let h = f / f0;
            if (h - h.round()).abs() * f0 < 3.0 * sr / 2.0 / bins as f64 {
                harmonic += p;
            }
        }
        10.0 * ((total - harmonic).max(1e-30) / total).log10()
    }

    #[test]
    fn adaa_reduces_aliasing() {
        let sr = 48000.0;
        let f0 = 4987.0;
        let n = 8192;
        let x: Vec<f32> = (0..n).map(|i| (8.0 * (2.0 * std::f64::consts::PI * f0 * i as f64 / sr).sin()) as f32).collect();
        let naive: Vec<f32> = x.iter().map(|&v| (v as f64).tanh() as f32).collect();
        let mut a = Adaa1::new();
        a.reset(Shaper::Tanh);
        let adaa: Vec<f32> = x.iter().map(|&v| a.process(Shaper::Tanh, v)).collect();
        let dn = inharmonic_db(&naive[1024..5120], f0, sr);
        let da = inharmonic_db(&adaa[1024..5120], f0, sr);
        // heavy drive (+18 dB into tanh) at 5 kHz: ADAA1 alone takes off ~4 dB of the inharmonic
        // energy; the 2× oversampling around it (gfx-dsp oversample) does the rest
        assert!(da < dn - 3.0, "naive {dn:.1} dB, adaa {da:.1} dB");
    }

    #[test]
    fn hard_adaa_reduces_aliasing() {
        let sr = 48000.0;
        let f0 = 4987.0;
        let n = 8192;
        let x: Vec<f32> = (0..n).map(|i| (8.0 * (2.0 * std::f64::consts::PI * f0 * i as f64 / sr).sin()) as f32).collect();
        let naive: Vec<f32> = x.iter().map(|&v| v.clamp(-1.0, 1.0)).collect();
        let mut a = Adaa1::new();
        a.reset(Shaper::Hard);
        let adaa: Vec<f32> = x.iter().map(|&v| a.process(Shaper::Hard, v)).collect();
        let dn = inharmonic_db(&naive[1024..5120], f0, sr);
        let da = inharmonic_db(&adaa[1024..5120], f0, sr);
        assert!(da < dn - 3.0, "naive {dn:.1} dB, adaa {da:.1} dB");
    }

    #[test]
    fn small_signals_are_linear() {
        let mut a = Adaa1::new();
        a.reset(Shaper::Asym(0.3));
        let mut h = HalfSample::default();
        for i in 0..1000 {
            let x = 1e-3 * ((i as f32) * 0.05).sin();
            let y = a.process(Shaper::Asym(0.3), x);
            let d = h.process(x);
            // slope of the asymmetric shaper at 0 is 1 − tanh²(b)
            let s = 1.0 - 0.3f32.tanh().powi(2);
            assert!((y - s * d).abs() < 2e-6, "{i}: {y} vs {}", s * d);
        }
    }

    #[test]
    fn antiderivatives_match_their_functions() {
        // dF/dx = f, by central differences, across every piece
        let pair = DiodePair { d1: ParkerDiode::new(0.2 * 1.1, 0.4, 1.0), d2: ParkerDiode::new(0.2 * 0.9, 0.4, 1.0) };
        let t = TanhResidual { k: 6.0 };
        let hc = HardClip { c: 0.7 };
        for i in -300..=300 {
            let x = i as f64 * 0.004 + 1e-4;
            let h = 1e-6;
            let dp = (pair.antiderivative(x + h) - pair.antiderivative(x - h)) / (2.0 * h);
            assert!((dp - pair.f(x)).abs() < 1e-6, "diode at {x}: {dp} vs {}", pair.f(x));
            let dt = (t.antiderivative(x + h) - t.antiderivative(x - h)) / (2.0 * h);
            assert!((dt - t.f(x)).abs() < 1e-5, "tanh residual at {x}: {dt} vs {}", t.f(x));
            let dh = (hc.antiderivative(x + h) - hc.antiderivative(x - h)) / (2.0 * h);
            assert!((dh - hc.f(x)).abs() < 1e-6, "hard clip at {x}: {dh} vs {}", hc.f(x));
            let ds = (Shaper::Hard.antiderivative(x * 3.0 + h) - Shaper::Hard.antiderivative(x * 3.0 - h)) / (2.0 * h);
            assert!((ds - Shaper::Hard.f(x * 3.0)).abs() < 1e-6, "Shaper::Hard at {}", x * 3.0);
        }
    }

    #[test]
    fn adaa_fn_tracks_the_shaper_on_slow_signals() {
        let pair = DiodePair { d1: ParkerDiode::new(0.2, 0.4, 1.0), d2: ParkerDiode::new(0.2, 0.4, 1.0) };
        let mut a = Adaa1Fn::default();
        let mut prev = 0.0;
        for i in 0..2000 {
            let x = (i as f64 * 0.003).sin();
            let y = a.process(&pair, x);
            // a half-sample delay: compare with the shaper at the midpoint
            let m = pair.f(0.5 * (x + prev));
            assert!((y - m).abs() < 1e-4, "{i}: {y} vs {m}");
            prev = x;
        }
    }
}
