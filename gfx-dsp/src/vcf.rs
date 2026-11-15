// SPDX-License-Identifier: GPL-3.0-or-later
//! The synth filter (gfx-polysynth §2.5): gfx-autowah's filter path as coded, as a reusable block.
//!
//! - **Drive**: an ADAA1 tanh of the input × k_d, divided by √k_d (k_d = 10^(drive_dB / 20)); skipped at
//!   0 dB. It sits ahead of the filter, not in its loop: the filter never self-oscillates.
//! - **Filter**: a TPT state-variable filter (gfx-dsp `svf`), low-pass, band-pass or high-pass, × 1
//!   (2 poles) or × 2 (4 poles: a first SVF at Q 0.707, then one at Q, so there is one resonance, not
//!   two stacked). The band-pass is the analogue SVF's resonant output (gain Q at the peak), as in
//!   gfx-autowah.
//!
//! The operation order and the f32 / f64 types are gfx-autowah's (`process_block`, its Low-pass /
//! Band-pass / High-pass branch), so that gfx-autowah can move onto this block under a bit-identical
//! null (its README §2.5 rule; it has not moved yet: the unit test below checks this block against a
//! copy of that inline code). The coefficients are recomputed only when the cut-off, Q, type or slope
//! change: [`Svf::set`] is deterministic, so skipping an identical call changes no sample.

use crate::adaa::{Adaa1, Shaper};
use crate::svf::{Svf, SvfMode};

/// The filter's response (gfx-autowah's first three `type` options, same order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VcfType {
    #[default]
    LowPass,
    BandPass,
    HighPass,
}

impl VcfType {
    /// From an enum index (0 Low-pass, 1 Band-pass, 2 High-pass).
    pub fn from_index(i: usize) -> Self {
        match i {
            1 => VcfType::BandPass,
            2 => VcfType::HighPass,
            _ => VcfType::LowPass,
        }
    }

    fn mode(self) -> SvfMode {
        match self {
            VcfType::LowPass => SvfMode::LowPass,
            VcfType::HighPass => SvfMode::HighPass,
            VcfType::BandPass => SvfMode::BandPass,
        }
    }
}

/// gfx-autowah's `peak` law for a 0 – 1 resonance: Q = 0.5 + 19.5 · reso² (0.5 … 20; +26 dB at 1).
#[inline]
pub fn q_of(reso: f32) -> f32 {
    let r = reso.clamp(0.0, 1.0);
    0.5 + 19.5 * r * r
}

/// Drive (dB into the tanh) → filter, one channel.
#[derive(Clone, Debug)]
pub struct Vcf {
    f1: Svf,
    f2: Svf,
    adaa: Adaa1,
    /// the coefficients' inputs at the last `set` (fc, q, type, four poles, sample rate)
    last: (f32, f32, VcfType, bool, f32),
}

impl Default for Vcf {
    fn default() -> Self {
        Self::new()
    }
}

impl Vcf {
    pub fn new() -> Self {
        let mut v = Self { f1: Svf::new(), f2: Svf::new(), adaa: Adaa1::new(), last: (f32::NAN, 0.0, VcfType::LowPass, false, 0.0) };
        v.adaa.reset(Shaper::Tanh);
        v
    }

    /// Silence in the filter and the shaper's memory.
    pub fn reset(&mut self) {
        self.f1.reset();
        self.f2.reset();
        self.adaa.reset(Shaper::Tanh);
    }

    /// The filter for the coming samples: cut-off (Hz), Q, type, 4 poles or 2. Cheap when nothing
    /// changed (the coefficients are held).
    #[inline]
    pub fn set(&mut self, fc: f32, q: f32, ty: VcfType, four_poles: bool, sample_rate: f32) {
        let key = (fc, q, ty, four_poles, sample_rate);
        if key == self.last {
            return;
        }
        self.last = key;
        let mode = ty.mode();
        if four_poles {
            self.f1.set(mode, fc, std::f32::consts::FRAC_1_SQRT_2, 0.0, sample_rate);
            self.f2.set(mode, fc, q, 0.0, sample_rate);
        } else {
            self.f1.set(mode, fc, q, 0.0, sample_rate);
        }
    }

    /// One sample: drive (dB, 0 = off) then the filter set by [`Self::set`].
    #[inline]
    pub fn process(&mut self, x: f32, drive_db: f32) -> f32 {
        let kd = 10f32.powf(drive_db / 20.0);
        let driven = if drive_db > 0.0 { self.adaa.process(Shaper::Tanh, x * kd) / kd.sqrt() } else { x };
        let (_, q, ty, four, _) = self.last;
        let y = if four { self.f2.process(self.f1.process(driven)) } else { self.f1.process(driven) };
        match ty {
            VcfType::BandPass => y * q,
            _ => y,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// gfx-autowah's inline filter path (lib.rs, `process_block`, the Low-pass / Band-pass / High-pass
    /// branch), copied as it is coded.
    struct Inline {
        f1: Svf,
        f2: Svf,
        adaa: Adaa1,
    }

    impl Inline {
        fn new() -> Self {
            let mut a = Adaa1::new();
            a.reset(Shaper::Tanh);
            Self { f1: Svf::new(), f2: Svf::new(), adaa: a }
        }
        fn tick(&mut self, hx: f32, drive_db: f32, fc: f32, q: f32, ftype: usize, four: bool, sr: f32) -> f32 {
            let kd = 10f32.powf(drive_db / 20.0);
            let driven = if drive_db > 0.0 { self.adaa.process(Shaper::Tanh, hx * kd) / kd.sqrt() } else { hx };
            let mode = match ftype {
                0 => SvfMode::LowPass,
                2 => SvfMode::HighPass,
                _ => SvfMode::BandPass,
            };
            let y = if four {
                self.f1.set(mode, fc, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
                self.f2.set(mode, fc, q, 0.0, sr);
                self.f2.process(self.f1.process(driven))
            } else {
                self.f1.set(mode, fc, q, 0.0, sr);
                self.f1.process(driven)
            };
            match ftype {
                1 => y * q,
                _ => y,
            }
        }
    }

    #[test]
    fn bit_identical_to_autowah_path() {
        // every type × poles, drive 0 and > 0, a swept and a held cut-off, 44.1 and 96 kHz
        for &sr in &[44100.0f32, 96000.0] {
            for ty in 0..3 {
                for four in [false, true] {
                    for drive in [0.0f32, 9.0, 24.0] {
                        let mut a = Inline::new();
                        let mut b = Vcf::new();
                        let mut rng = crate::lfo::Rng::new(7);
                        for i in 0..20000 {
                            let x = 0.6 * rng.bipolar() + 0.3 * (i as f32 * 0.031).sin();
                            let fc = if i < 10000 { 200.0 + 3000.0 * (i as f32 / 10000.0) } else { 900.0 };
                            let q = q_of(0.3 + 0.6 * (i as f32 / 20000.0));
                            let ya = a.tick(x, drive, fc, q, ty, four, sr);
                            b.set(fc, q, VcfType::from_index(ty), four, sr);
                            let yb = b.process(x, drive);
                            assert_eq!(ya.to_bits(), yb.to_bits(), "sr {sr} type {ty} four {four} drive {drive} sample {i}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn resonance_peak_and_no_self_oscillation() {
        let sr = 48000.0;
        // Q 20 at 1 kHz, 2 poles low-pass: +26 dB at the cut-off
        let mut v = Vcf::new();
        v.set(1000.0, q_of(1.0), VcfType::LowPass, false, sr);
        let g = crate::svf::measure_gain_db(|x| v.process(x, 0.0), 1000.0, sr);
        assert!((g - 26.0).abs() < 0.5, "peak {g} dB");
        // an impulse dies: no self-oscillation at full resonance and drive
        let mut v = Vcf::new();
        v.set(1000.0, q_of(1.0), VcfType::LowPass, true, sr);
        let mut last = 0.0f32;
        for i in 0..96000 {
            let y = v.process(if i == 0 { 1.0 } else { 0.0 }, 24.0);
            if i > 90000 {
                last = last.max(y.abs());
            }
        }
        assert!(last < 1e-6, "tail {last}");
    }
}
