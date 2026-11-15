// SPDX-License-Identifier: GPL-3.0-or-later
//! Band-limited oscillators on one f64 phase type (gfx-vocoder §2.5; gfx-ringmod's wavetables and
//! gfx-polysynth's voices are meant to share [`Phasor`] when they are coded).
//!
//! - [`Phasor`]: a phase in [0, 1) and its increment, both f64. A frequency change keeps the phase
//!   (no click).
//! - [`saw`], [`pulse`]: two-point PolyBLEP (Välimäki & Huovilainen 2007) on the phasor; the pulse is
//!   50 %. Peak ±1.
//! - [`buzz`]: equal-amplitude cosine harmonics up to 0.45 fs by the closed-form sum (the discrete
//!   summation formula), the last harmonic faded in by its fractional part so harmonics enter and
//!   leave smoothly as the pitch moves, scaled by 1 / √(n + frac²) (unit power per harmonic set); the
//!   series limit is taken where the denominator vanishes.
//! - [`Supersaw`]: 7 PolyBLEP saws detuned symmetrically by up to `detune` cents, start phases from a
//!   seed (deterministic), a mono sum and an alternated L / R pair.

use crate::lfo::Rng;
use std::f64::consts::TAU;

/// Highest harmonic frequency of [`buzz`], as a fraction of the sample rate.
pub const BUZZ_TOP: f64 = 0.45;

/// A phase accumulator: `t` in [0, 1), `dt` = f / fs per sample.
#[derive(Clone, Copy, Debug, Default)]
pub struct Phasor {
    pub t: f64,
    pub dt: f64,
}

impl Phasor {
    pub fn new(t0: f64) -> Self {
        Self { t: t0.rem_euclid(1.0), dt: 0.0 }
    }

    /// The frequency (Hz) at `sample_rate`; the phase is kept. Clamped to [0, fs / 2).
    #[inline]
    pub fn set_freq(&mut self, hz: f64, sample_rate: f64) {
        self.dt = (hz / sample_rate).clamp(0.0, 0.499_999);
    }

    /// Moves the phase one sample on.
    #[inline]
    pub fn advance(&mut self) {
        self.t += self.dt;
        if self.t >= 1.0 {
            self.t -= 1.0;
        }
    }
}

/// The two-point PolyBLEP residual of a unit step at phase 0 (`t` in [0, 1), increment `dt`).
#[inline]
pub fn poly_blep(t: f64, dt: f64) -> f64 {
    if dt <= 0.0 {
        0.0
    } else if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// PolyBLEP saw at the phasor's phase, rising from −1 to +1.
#[inline]
pub fn saw(p: &Phasor) -> f64 {
    2.0 * p.t - 1.0 - poly_blep(p.t, p.dt)
}

/// PolyBLEP 50 % pulse at the phasor's phase: +1 on the first half.
#[inline]
pub fn pulse(p: &Phasor) -> f64 {
    let t2 = if p.t < 0.5 { p.t + 0.5 } else { p.t - 0.5 };
    let naive = if p.t < 0.5 { 1.0 } else { -1.0 };
    naive + poly_blep(p.t, p.dt) - poly_blep(t2, p.dt)
}

/// Equal-amplitude cosine harmonics up to [`BUZZ_TOP`] · fs (closed form), unit power per harmonic
/// set: Σ_{k ≤ n} cos kθ + frac · cos (n + 1)θ, divided by √(n + frac²).
#[inline]
pub fn buzz(p: &Phasor) -> f64 {
    if p.dt <= 0.0 {
        return 0.0;
    }
    let nf = BUZZ_TOP / p.dt;
    let n = nf.floor();
    let frac = nf - n;
    let th = TAU * p.t;
    let s = (0.5 * th).sin();
    let sum = if s.abs() < 1e-9 { n } else { ((n + 0.5) * th).sin() / (2.0 * s) - 0.5 };
    let y = sum + frac * ((n + 1.0) * th).cos();
    y / (n + frac * frac).max(1e-9).sqrt()
}

/// Saws per supersaw note.
pub const SUPER: usize = 7;

/// Seven detuned PolyBLEP saws (offsets −1, −⅔, −⅓, 0, ⅓, ⅔, 1 × `detune`); saw 3 is the centre.
#[derive(Clone, Debug)]
pub struct Supersaw {
    pub saws: [Phasor; SUPER],
    start: [f64; SUPER],
}

impl Supersaw {
    /// Start phases from `seed` (the centre saw starts at 0, so it is phase-coherent with a plain
    /// saw on the same phasor).
    pub fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let mut start = [0.0; SUPER];
        for (i, s) in start.iter_mut().enumerate() {
            let r = rng.next_f64();
            *s = if i == 3 { 0.0 } else { r };
        }
        let mut saws = [Phasor::default(); SUPER];
        for (p, &s) in saws.iter_mut().zip(&start) {
            p.t = s;
        }
        Self { saws, start }
    }

    /// Back to the seeded start phases.
    pub fn reset(&mut self) {
        for (p, &s) in self.saws.iter_mut().zip(&self.start) {
            p.t = s;
        }
    }

    /// The note's frequency and the spread (cents, the outer saws' offset).
    #[inline]
    pub fn set_freq(&mut self, hz: f64, detune_cents: f64, sample_rate: f64) {
        for (i, p) in self.saws.iter_mut().enumerate() {
            let c = detune_cents * (i as f64 - 3.0) / 3.0;
            p.set_freq(hz * (c / 1200.0).exp2(), sample_rate);
        }
    }

    /// The seven saws: (mono sum / √7, even saws / 2, odd saws / √3); then advances.
    #[inline]
    pub fn next(&mut self) -> (f64, f64, f64) {
        let mut even = 0.0;
        let mut odd = 0.0;
        for (i, p) in self.saws.iter_mut().enumerate() {
            let v = saw(p);
            p.advance();
            if i % 2 == 0 {
                even += v;
            } else {
                odd += v;
            }
        }
        ((even + odd) * (1.0 / 7f64.sqrt()), even * 0.5, odd * (1.0 / 3f64.sqrt()))
    }
}

impl Default for Supersaw {
    fn default() -> Self {
        Self::new(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Amplitude of the component at `f` (Goertzel over a Hann window).
    fn amp(x: &[f64], f: f64, sr: f64) -> f64 {
        let n = x.len();
        let w = TAU * f / sr;
        let (mut re, mut im, mut ws) = (0.0, 0.0, 0.0);
        for (i, v) in x.iter().enumerate() {
            let h = 0.5 - 0.5 * (TAU * i as f64 / n as f64).cos();
            re += h * v * (w * i as f64).cos();
            im += h * v * (w * i as f64).sin();
            ws += h;
        }
        2.0 * (re * re + im * im).sqrt() / ws
    }

    /// Worst inharmonic component (dB under the fundamental) on a 25 Hz grid between harmonics.
    fn worst_alias(x: &[f64], f0: f64, sr: f64) -> f64 {
        let a1 = amp(x, f0, sr);
        let mut worst = 0.0f64;
        let mut f = 50.0;
        while f < 0.5 * sr - 100.0 {
            let k = (f / f0).round();
            if (f - k * f0).abs() > 60.0 {
                worst = worst.max(amp(x, f, sr));
            }
            f += 25.0;
        }
        20.0 * (worst / a1).log10()
    }

    fn render(f: f64, sr: f64, n: usize, w: fn(&Phasor) -> f64) -> Vec<f64> {
        let mut p = Phasor::new(0.0);
        p.set_freq(f, sr);
        (0..n)
            .map(|_| {
                let v = w(&p);
                p.advance();
                v
            })
            .collect()
    }

    #[test]
    fn saw_and_pulse_have_the_right_harmonics() {
        let sr = 48000.0;
        let x = render(220.0, sr, 24000, saw);
        // saw: 2 / (π k) per harmonic
        for k in 1..6 {
            let a = amp(&x, 220.0 * k as f64, sr);
            let want = 2.0 / (std::f64::consts::PI * k as f64);
            assert!((a / want - 1.0).abs() < 0.02, "saw h{k}: {a} vs {want}");
        }
        let y = render(220.0, sr, 24000, pulse);
        // pulse: 4 / (π k) on odd harmonics, nothing on even ones
        assert!((amp(&y, 220.0, sr) / (4.0 / std::f64::consts::PI) - 1.0).abs() < 0.02);
        assert!(amp(&y, 440.0, sr) < 1e-3);
    }

    #[test]
    fn polyblep_aliases_measured() {
        // the inharmonic residue of two-point PolyBLEP (gfx-vocoder §5 carrier purity, before the bank)
        for (f, bound) in [(1234.5, -30.0), (4321.0, -20.0)] {
            for (name, w) in [("saw", saw as fn(&Phasor) -> f64), ("pulse", pulse)] {
                let x = render(f, 48000.0, 16384, w);
                let a = worst_alias(&x, f, 48000.0);
                println!("{name} {f} Hz: worst alias {a:.1} dB");
                assert!(a < bound, "{name} {f}: {a}");
            }
        }
    }

    #[test]
    fn buzz_is_flat_band_limited_and_continuous() {
        let sr = 48000.0;
        let f = 1000.0;
        let x = render(f, sr, 24000, buzz);
        // 21 harmonics up to 21.6 kHz (nf = 21.6): equal amplitudes, the last at its fraction
        let a1 = amp(&x, f, sr);
        for k in [2.0, 5.0, 10.0, 20.0] {
            let a = amp(&x, k * f, sr);
            assert!((a / a1 - 1.0).abs() < 0.02, "h{k}: {}", a / a1);
        }
        assert!((amp(&x, 22000.0, sr) / a1 - 0.6).abs() < 0.03);
        assert!(worst_alias(&x, f, sr) < -90.0);
        // power: n harmonics of amplitude 1 / √(n + frac²) → mean square ≈ 1 / 2
        let ms = x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64;
        assert!((ms - 0.5).abs() < 0.02, "{ms}");
        // a pitch sweep moves harmonics in and out without a step
        let mut p = Phasor::new(0.0);
        let mut last = buzz(&p);
        let mut worst = 0.0f64;
        for i in 0..48000 {
            p.set_freq(400.0 + 600.0 * i as f64 / 48000.0, sr);
            p.advance();
            let v = buzz(&p);
            // against the waveform's own slope bound: Σ k · 2π dt / √n over n + 1 harmonics
            let nf = BUZZ_TOP / p.dt;
            let bound = (nf + 1.0) * (nf + 2.0) / 2.0 * TAU * p.dt / nf.floor().sqrt();
            worst = worst.max((v - last).abs() / bound);
            last = v;
            assert!(v.is_finite());
        }
        assert!(worst < 1.0, "{worst}");
    }

    #[test]
    fn frequency_change_keeps_phase_and_supersaw_is_seeded() {
        let mut p = Phasor::new(0.25);
        p.set_freq(100.0, 48000.0);
        p.advance();
        let t = p.t;
        p.set_freq(5000.0, 48000.0);
        assert_eq!(p.t, t);
        let mut a = Supersaw::new(3);
        let mut b = Supersaw::new(3);
        a.set_freq(220.0, 15.0, 48000.0);
        b.set_freq(220.0, 15.0, 48000.0);
        let xa: Vec<_> = (0..1000).map(|_| a.next()).collect();
        let xb: Vec<_> = (0..1000).map(|_| b.next()).collect();
        assert_eq!(xa, xb);
        a.reset();
        a.set_freq(220.0, 15.0, 48000.0);
        assert_eq!(a.next(), xb[0]);
        // the centre saw is a plain saw from phase 0
        assert_eq!(a.saws[3].dt, 220.0 / 48000.0);
        // detune spreads the outer saws by ±15 cents
        let r = a.saws[6].dt / a.saws[3].dt;
        assert!((1200.0 * r.log2() - 15.0).abs() < 1e-9);
    }
}
