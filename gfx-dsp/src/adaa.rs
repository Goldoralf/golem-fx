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
        }
    }

    #[inline]
    pub fn antiderivative(self, x: f64) -> f64 {
        match self {
            Shaper::Tanh => ln_cosh(x),
            Shaper::Asym(b) => ln_cosh(x + b) - x * b.tanh(),
        }
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
}
