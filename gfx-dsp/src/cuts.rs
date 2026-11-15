// SPDX-License-Identifier: GPL-3.0-or-later
//! Low and high cuts with stops (gfx-cab README §2.5; gfx-cab and gfx-irfx): Butterworth 12 or 24 dB / oct from gfx-dsp `svf` (f64
//! state), gfx-eq's ids and stops. At its stop a cut is bypassed exactly; leaving or reaching the stop
//! crossfades in or out over 30 ms. A slope change crossfades 12 ↔ 24 over 20 ms (both run meanwhile).
//! Coefficients follow the smoothed frequency on a 16-sample grid of the effect's own clock, so the
//! output does not depend on the host's block size.

use crate::exp_smooth::ExpSmoother;
use crate::smooth::LinearRamp;
use crate::svf::{Svf, SvfMode};

const Q12: f32 = std::f32::consts::FRAC_1_SQRT_2;
const Q24: [f32; 2] = [0.541_196_1, 1.306_563];
const GRID: u64 = 16;

/// One cut (high-pass or low-pass).
pub struct Cut {
    mode: SvfMode,
    stop: f32,
    f: ExpSmoother,
    on: LinearRamp,
    slope: LinearRamp,
    s12: Svf,
    s24: [Svf; 2],
    set_f: f32,
    sr: f32,
    /// the filters hold state
    live: bool,
}

impl Cut {
    /// `low`: a high-pass (low cut) with its stop at `min`; else a low-pass with its stop at `max`.
    pub fn new(low: bool, min: f32, max: f32) -> Self {
        let stop = if low { min } else { max };
        Self {
            mode: if low { SvfMode::HighPass } else { SvfMode::LowPass },
            stop,
            f: ExpSmoother::new(min, max, stop),
            on: LinearRamp::new(30.0, 0.0),
            slope: LinearRamp::new(20.0, 0.0),
            s12: Svf::new(),
            s24: [Svf::new(), Svf::new()],
            set_f: f32::NAN,
            sr: 48000.0,
            live: false,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sr = sample_rate;
        self.f.prepare(sample_rate);
        self.on.prepare(sample_rate);
        self.slope.prepare(sample_rate);
    }

    /// The frequency (Hz); `now` = the effect's clock (EXP rule).
    pub fn set_freq(&mut self, hz: f32, now: u64) {
        self.f.set_target(hz, now);
        self.on.set_target(if hz == self.stop { 0.0 } else { 1.0 });
    }

    /// 0 = 12 dB / oct, 1 = 24 dB / oct.
    pub fn set_slope(&mut self, s: f32) {
        self.slope.set_target(if s >= 0.5 { 1.0 } else { 0.0 });
    }

    fn design(&mut self, f: f32) {
        self.set_f = f;
        self.s12.set(self.mode, f, Q12, 0.0, self.sr);
        self.s24[0].set(self.mode, f, Q24[0], 0.0, self.sr);
        self.s24[1].set(self.mode, f, Q24[1], 0.0, self.sr);
    }

    /// Smoothers on their targets, filters silent.
    pub fn reset(&mut self) {
        self.f.reset();
        self.on.settle();
        self.slope.settle();
        self.s12.reset();
        self.s24.iter_mut().for_each(|s| s.reset());
        self.live = false;
        let f = self.f.value();
        self.design(f);
    }

    /// True when the cut is out of the path (at its stop, settled).
    pub fn bypassed(&self) -> bool {
        self.on.is_settled() && self.on.value() == 0.0
    }

    #[inline]
    pub fn process(&mut self, x: f32, clock: u64) -> f32 {
        let f = self.f.next();
        if self.bypassed() {
            if self.live {
                self.live = false;
                self.s12.reset();
                self.s24.iter_mut().for_each(|s| s.reset());
            }
            return x;
        }
        self.live = true;
        if clock.is_multiple_of(GRID) && f != self.set_f {
            self.design(f);
        }
        let on = self.on.next();
        let s = self.slope.next();
        let y12 = self.s12.process(x);
        let y24a = self.s24[0].process(x);
        let y24 = self.s24[1].process(y24a);
        let y = if s == 0.0 { y12 } else if s == 1.0 { y24 } else { y12 + s * (y24 - y12) };
        if on == 1.0 {
            y
        } else {
            x + on * (y - x)
        }
    }
}

/// Low cut then high cut.
pub struct Cuts {
    pub low: Cut,
    pub high: Cut,
}

impl Cuts {
    pub fn new(low_min: f32, low_max: f32, high_min: f32, high_max: f32) -> Self {
        Self { low: Cut::new(true, low_min, low_max), high: Cut::new(false, high_min, high_max) }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.low.prepare(sample_rate);
        self.high.prepare(sample_rate);
    }

    pub fn reset(&mut self) {
        self.low.reset();
        self.high.reset();
    }

    #[inline]
    pub fn process(&mut self, x: f32, clock: u64) -> f32 {
        let y = self.low.process(x, clock);
        self.high.process(y, clock)
    }
}
