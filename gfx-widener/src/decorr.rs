// SPDX-License-Identifier: GPL-3.0-or-later
//! The side generators' fixed parts (README §2.2): the two-band velvet filter and the all-pass sets.
//!
//! - **Velvet**: side = V_long ∗ LP(M) + V_short ∗ (M − LP(M)), LP a one-pole at 1 kHz (the pair sums
//!   to M exactly). V_long: 16 pulses over `time`; V_short: 14 pulses over `time` / 4. The histories
//!   of LP(M) and M − LP(M) are shared by every velvet in use (the widener's two crossfade slots);
//!   a [`Velvet`] only holds integer positions and gains, recomputed on a `time` change without
//!   allocating. The sequences are `tools/vnd-design.py`'s output (`vnd_tables.rs`).
//! - **Allpass**: 6 second-order all-passes (TPT SVF, x − 2k·BP) per set, `ap_tables.rs`.
//!
//! Moved to gfx-dsp when a second effect wants a decorrelator (README §2.8).

use crate::ap_tables;
use crate::vnd_tables::{LONG, N_LONG, N_SHORT, SHORT};
use gfx_dsp::delay::DelayLine;
use gfx_dsp::svf::{Svf, SvfMode};

/// The split between the long and the short sequence (Hz).
pub const SPLIT_HZ: f64 = 1000.0;
/// V_short spans `time` / this.
pub const SHORT_RATIO: f64 = 4.0;
/// Velvet clamps `time` to at least this (ms): shorter, the lows cannot be wide and uncoloured.
pub const MIN_TIME_MS: f32 = 10.0;

/// The one-pole split and the two band histories the velvet taps read.
#[derive(Clone, Debug, Default)]
pub struct VelvetBands {
    a: f64,
    lp: f64,
    pub low: DelayLine,
    pub high: DelayLine,
}

impl VelvetBands {
    pub fn prepare(&mut self, sample_rate: f32, max_delay: usize) {
        self.a = 1.0 - (-2.0 * std::f64::consts::PI * SPLIT_HZ / sample_rate as f64).exp();
        self.low.prepare(max_delay);
        self.high.prepare(max_delay);
        self.lp = 0.0;
    }

    pub fn reset(&mut self) {
        self.lp = 0.0;
        self.low.reset();
        self.high.reset();
    }

    #[inline]
    pub fn push(&mut self, m: f32) {
        self.lp += self.a * (m as f64 - self.lp);
        let lp = self.lp as f32;
        self.low.push(lp);
        self.high.push(m - lp);
    }
}

/// One variant at one `time`: integer positions and gains.
#[derive(Clone, Debug)]
pub struct Velvet {
    lp: [usize; N_LONG],
    lg: [f32; N_LONG],
    sp: [usize; N_SHORT],
    sg: [f32; N_SHORT],
}

impl Default for Velvet {
    fn default() -> Self {
        Self { lp: [1; N_LONG], lg: [0.0; N_LONG], sp: [1; N_SHORT], sg: [0.0; N_SHORT] }
    }
}

impl Velvet {
    /// `variant` 0 – 7; `time_ms` clamped to [`MIN_TIME_MS`] – 40.
    pub fn set(&mut self, variant: usize, time_ms: f32, sample_rate: f32) {
        let v = variant.min(LONG.len() - 1);
        let len = time_ms.clamp(MIN_TIME_MS, 40.0) as f64 * 0.001 * sample_rate as f64;
        for (k, &(f, g)) in LONG[v].iter().enumerate() {
            self.lp[k] = ((f * len).round() as usize).max(1);
            self.lg[k] = g as f32;
        }
        for (k, &(f, g)) in SHORT[v].iter().enumerate() {
            self.sp[k] = ((f * len / SHORT_RATIO).round() as usize).max(1);
            self.sg[k] = g as f32;
        }
    }

    /// The longest position (samples).
    pub fn length(&self) -> usize {
        self.lp.iter().chain(self.sp.iter()).copied().max().unwrap_or(1)
    }

    #[inline]
    pub fn process(&self, bands: &VelvetBands) -> f32 {
        let mut acc = 0.0f32;
        for k in 0..N_LONG {
            acc += self.lg[k] * bands.low.tap(self.lp[k]);
        }
        for k in 0..N_SHORT {
            acc += self.sg[k] * bands.high.tap(self.sp[k]);
        }
        acc
    }
}

/// One all-pass set: its six sections' states and coefficients.
#[derive(Clone, Debug, Default)]
pub struct Allpass {
    s: [Svf; ap_tables::N_SECTIONS],
}

impl Allpass {
    pub fn set(&mut self, variant: usize, sample_rate: f32) {
        let set = &ap_tables::SETS[variant.min(ap_tables::SETS.len() - 1)];
        for (sv, &(f, q)) in self.s.iter_mut().zip(set.iter()) {
            sv.set(SvfMode::AllPass, f, q, 0.0, sample_rate);
        }
    }

    pub fn reset(&mut self) {
        self.s.iter_mut().for_each(|s| s.reset());
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.s.iter_mut().fold(x, |y, s| s.process(y))
    }
}
