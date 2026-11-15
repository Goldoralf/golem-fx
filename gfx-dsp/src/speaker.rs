// SPDX-License-Identifier: GPL-3.0-or-later
//! Small loudspeakers (gfx-radio §2.5): the large-signal motor, the horn throat and the body filters.
//!
//! - [`Motor`]: the normalised Klippel lumped model, a 2-state ODE in the excursion x (units of
//!   X_max) driven by the amplifier's voltage u:
//!
//!   ```text
//!   x'' = ω_s² · [ b(x) · u − k(x) · x ] − ω_s · ( 1/Q_ms + b(x)² / Q_es ) · x'
//!   b(x) = (1 + β·x_b²) / (1 + β·(x − x_b)²)      force factor: bell-shaped, never negative
//!   k(x) = 1 + κ1·x + κ·x² + κ3·x⁴                  suspension: asymmetry, stiffening, bottoming
//!   p    = x'' / ω_s²                               far-field pressure (cone acceleration)
//!   ```
//!
//!   At low level b = k = 1 and the motor is the Thiele-Small 2nd-order high-pass at ω_s with
//!   1 / Q_ts = 1 / Q_ms + 1 / Q_es. Trapezoidal rule, a predictor and 2 fixed-point iterations per
//!   sample; p is read from the right-hand side (no numerical differentiation). |x| is clamped at 4.
//!   Run it oversampled (gfx-radio: 2× or 4×, with the amplifier).
//! - [`Rattle`]: a rubbing voice coil above 0.85 X_max: the smooth excess gates a > 2 kHz noise and a
//!   band-limited impulse at each direction change.
//! - [`Throat`]: the air's own nonlinearity in a horn throat, y = p + η · HP₂₀₀(d/dt p²), after a 4th-order
//!   low-pass at the horn's upper edge (the motor's acceleration band-limits nothing): a 2nd harmonic
//!   rising 6 dB / oct and in proportion to the pressure.
//! - [`Body`]: up to [`MAX_SECTIONS`] matched second-order sections (cone break-up bells, box bump,
//!   high roll-off, horn cut-off), each tagged as a low edge, a high edge or a bell so that a caller
//!   can move the edges (`band`) and scale the bells (`resonance`).

use crate::lfo::Rng;
use crate::matched::{design, Analog};
use crate::svf::{Svf, SvfMode};
use std::f64::consts::TAU;

/// The motor's constants (normalised: x in units of X_max).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotorParams {
    /// Resonance f_s (Hz).
    pub fs_hz: f64,
    /// Total Q (Q_ts) and mechanical Q (Q_ms); Q_es follows: 1 / Q_es = 1 / Q_ts − 1 / Q_ms.
    pub q_ts: f64,
    pub q_ms: f64,
    /// Bl(x) fall-off β and rest-position offset x_b.
    pub beta: f64,
    pub x_b: f64,
    /// K(x) = 1 + κ1·x + κ·x² + κ3·x⁴.
    pub kappa1: f64,
    pub kappa: f64,
    pub kappa3: f64,
}

impl MotorParams {
    /// A linear motor (b = k = 1): the Thiele-Small high-pass.
    pub const fn linear(fs_hz: f64, q_ts: f64, q_ms: f64) -> Self {
        Self { fs_hz, q_ts, q_ms, beta: 0.0, x_b: 0.0, kappa1: 0.0, kappa: 0.0, kappa3: 0.0 }
    }

    /// The force factor b(x) (1 at rest).
    #[inline]
    pub fn b(&self, x: f64) -> f64 {
        let d = x - self.x_b;
        (1.0 + self.beta * self.x_b * self.x_b) / (1.0 + self.beta * d * d)
    }

    /// The stiffness k(x) (1 at rest).
    #[inline]
    pub fn k(&self, x: f64) -> f64 {
        let x2 = x * x;
        1.0 + self.kappa1 * x + self.kappa * x2 + self.kappa3 * x2 * x2
    }

    /// The small-signal response |p / u| at `f` Hz: the analytic 2nd-order high-pass.
    pub fn small_signal(&self, f: f64) -> f64 {
        let s = f / self.fs_hz;
        let re = 1.0 - s * s;
        let im = s / self.q_ts;
        s * s / (re * re + im * im).sqrt()
    }
}

/// The largest excursion the state may reach (X_max units): a guard against pathological automation.
pub const X_CLAMP: f64 = 4.0;
/// Fixed-point iterations per sample.
pub const ITERATIONS: usize = 2;

/// The motor ODE's state, at the rate it is run at.
#[derive(Clone, Debug)]
pub struct Motor {
    p: MotorParams,
    w: f64,
    w2: f64,
    inv_qms: f64,
    inv_qes: f64,
    h: f64,
    iters: usize,
    x: f64,
    v: f64,
    /// the right-hand side at the current state and the previous input (the next step's a0)
    a: f64,
    u1: f64,
}

impl Default for Motor {
    fn default() -> Self {
        let mut m = Self { p: MotorParams::linear(400.0, 0.8, 4.0), w: 0.0, w2: 0.0, inv_qms: 0.0, inv_qes: 0.0, h: 1.0 / 96000.0, iters: ITERATIONS, x: 0.0, v: 0.0, a: 0.0, u1: 0.0 };
        m.set(MotorParams::linear(400.0, 0.8, 4.0), 96000.0);
        m
    }
}

impl Motor {
    /// Constants and the rate the ODE runs at (the oversampled rate). The state is kept.
    pub fn set(&mut self, p: MotorParams, rate: f64) {
        self.p = p;
        self.w = TAU * p.fs_hz;
        self.w2 = self.w * self.w;
        self.inv_qms = 1.0 / p.q_ms;
        self.inv_qes = (1.0 / p.q_ts - 1.0 / p.q_ms).max(0.0);
        self.h = 1.0 / rate;
    }

    /// Moves only the resonance (gfx-radio's `band` on the low edge).
    pub fn set_fs(&mut self, fs_hz: f64) {
        if fs_hz != self.p.fs_hz {
            self.p.fs_hz = fs_hz;
            self.w = TAU * fs_hz;
            self.w2 = self.w * self.w;
        }
    }

    /// Fixed-point iterations per sample (2 by default; tests compare with more).
    pub fn set_iterations(&mut self, n: usize) {
        self.iters = n.max(1);
    }

    pub fn params(&self) -> &MotorParams {
        &self.p
    }

    pub fn reset(&mut self) {
        self.x = 0.0;
        self.v = 0.0;
        self.a = 0.0;
        self.u1 = 0.0;
    }

    /// The excursion (X_max units) and velocity.
    pub fn state(&self) -> (f64, f64) {
        (self.x, self.v)
    }

    #[inline]
    fn rhs(&self, x: f64, v: f64, u: f64) -> f64 {
        let b = self.p.b(x);
        self.w2 * (b * u - self.p.k(x) * x) - self.w * (self.inv_qms + b * b * self.inv_qes) * v
    }

    /// One step with the drive `u`: returns the pressure p (the acceleration over ω_s²).
    #[inline]
    pub fn process(&mut self, u: f64) -> f64 {
        let (x, v, a0, h) = (self.x, self.v, self.a, self.h);
        // predictor (explicit Euler), then the trapezoidal rule by fixed-point iteration
        let mut xn = x + h * v;
        let mut vn = v + h * a0;
        for _ in 0..self.iters {
            let a1 = self.rhs(xn, vn, u);
            vn = v + 0.5 * h * (a0 + a1);
            xn = x + 0.5 * h * (v + vn);
        }
        if !xn.is_finite() || !vn.is_finite() {
            xn = 0.0;
            vn = 0.0;
        }
        self.x = xn.clamp(-X_CLAMP, X_CLAMP);
        self.v = vn;
        self.u1 = u;
        self.a = self.rhs(self.x, self.v, u);
        self.a / self.w2
    }
}

/// The voice-coil rub: noise above 2 kHz and a click at each turn, gated by the excursion past 0.85.
#[derive(Clone, Debug)]
pub struct Rattle {
    hp: [Svf; 2],
    rng: Rng,
    dir: f64,
    quiet: u32,
    idle: bool,
}

/// Where the coil starts to rub (X_max units).
pub const RATTLE_ONSET: f64 = 0.85;

impl Default for Rattle {
    fn default() -> Self {
        Self { hp: [Svf::new(), Svf::new()], rng: Rng::new(0x2A77_1E00), dir: 0.0, quiet: 0, idle: true }
    }
}

impl Rattle {
    pub fn prepare(&mut self, rate: f64) {
        self.hp[0].set(SvfMode::HighPass, 2000.0, 0.541_196, 0.0, rate as f32);
        self.hp[1].set(SvfMode::HighPass, 2000.0, 1.306_563, 0.0, rate as f32);
    }

    pub fn reset(&mut self) {
        for s in self.hp.iter_mut() {
            s.reset();
        }
        self.rng.reset();
        self.dir = 0.0;
        self.quiet = 0;
        self.idle = true;
    }

    /// The rub for the motor state (x, v), scaled by `amount` (0 – 1). Always runs its filters, so it
    /// never switches in from a stale state; zero when the coil stays inside 0.85 X_max.
    #[inline]
    pub fn process(&mut self, x: f64, v: f64, amount: f64) -> f64 {
        if amount <= 0.0 && self.idle {
            if v != 0.0 {
                self.dir = v.signum();
            }
            return 0.0;
        }
        let excess = (x.abs() - RATTLE_ONSET).max(0.0);
        let mut e = 0.0;
        if excess > 0.0 && amount > 0.0 {
            e = 0.3 * self.rng.bipolar() as f64;
            let d = v.signum();
            if d != self.dir && self.dir != 0.0 {
                e += 4.0 * d;
            }
        }
        if v != 0.0 {
            self.dir = v.signum();
        }
        let y = { let t = self.hp[0].process((amount * excess * e) as f32); self.hp[1].process(t) };
        // idle once the filters have rung out with nothing in (skipped then: no CPU at `rattle` 0)
        self.quiet = if y.abs() < 1e-12 && e == 0.0 { self.quiet + 1 } else { 0 };
        self.idle = self.quiet > 64;
        if self.idle {
            for s in self.hp.iter_mut() {
                s.reset();
            }
        }
        y as f64
    }
}

/// The horn throat's air nonlinearity.
#[derive(Clone, Debug)]
pub struct Throat {
    lp: [Svf; 2],
    hp: Svf,
    sq1: f64,
    eta: f64,
    /// fs / (2π · 1 kHz): the first difference scaled so η is the 2nd-harmonic ratio per unit pressure
    /// at 1 kHz
    scale: f64,
    rate: f64,
}

impl Default for Throat {
    fn default() -> Self {
        Self { lp: [Svf::new(), Svf::new()], hp: Svf::new(), sq1: 0.0, eta: 0.0, scale: 0.0, rate: 48000.0 }
    }
}

impl Throat {
    pub fn prepare(&mut self, rate: f64) {
        self.rate = rate;
        self.scale = rate / (TAU * 1000.0);
        self.hp.set(SvfMode::HighPass, 200.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, rate as f32);
        self.set_edge(6000.0);
    }

    /// The horn's upper edge (the 4th-order Butterworth low-pass before p², matched sections: the same
    /// response at every rate).
    pub fn set_edge(&mut self, hz: f64) {
        let f = hz.min(0.45 * self.rate);
        self.lp[0].set_biquad(&design(Analog::low_pass(0.541_196), f, self.rate));
        self.lp[1].set_biquad(&design(Analog::low_pass(1.306_563), f, self.rate));
    }

    /// η: the 2nd harmonic's ratio to the fundamental for a unit-peak pressure at 1 kHz.
    pub fn set_eta(&mut self, eta: f64) {
        self.eta = eta;
    }

    pub fn reset(&mut self) {
        for s in self.lp.iter_mut() {
            s.reset();
        }
        self.hp.reset();
        self.sq1 = 0.0;
    }

    #[inline]
    pub fn process(&mut self, p: f32) -> f32 {
        let pl = { let t = self.lp[0].process(p); self.lp[1].process(t) } as f64;
        let sq = pl * pl;
        let d = (sq - self.sq1) * self.scale;
        self.sq1 = sq;
        let h = self.hp.process(d as f32) as f64;
        (pl + self.eta * h) as f32
    }
}

/// What a body section is, for the edge and bell controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Role {
    /// A low edge (high-pass): moves down with `band` > 0.
    Low,
    /// A high edge (low-pass): moves up with `band` > 0.
    High,
    /// A peak or dip: its dB scales with `resonance`.
    Bell,
    /// Fixed.
    Fixed,
}

/// One body section: an analog prototype at a frequency.
#[derive(Clone, Copy, Debug)]
pub enum Shape {
    HighPass(f64),
    LowPass(f64),
    /// (Q, dB)
    Bell(f64, f64),
    /// (Q, dB)
    LowShelf(f64, f64),
    HighShelf(f64, f64),
}

#[derive(Clone, Copy, Debug)]
pub struct Section {
    pub shape: Shape,
    pub hz: f64,
    pub role: Role,
}

impl Section {
    pub const fn new(shape: Shape, hz: f64, role: Role) -> Self {
        Self { shape, hz, role }
    }

    /// The analog prototype and centre for `band` (octaves, ±1) and `resonance` (bell dB factor).
    pub fn prototype(&self, band: f64, resonance: f64) -> (Analog, f64) {
        let hz = match self.role {
            Role::Low => self.hz * 2f64.powf(-band),
            Role::High => self.hz * 2f64.powf(band),
            _ => self.hz,
        };
        let a = match self.shape {
            Shape::HighPass(q) => Analog::high_pass(q),
            Shape::LowPass(q) => Analog::low_pass(q),
            Shape::Bell(q, db) => Analog::bell(q, if self.role == Role::Bell { db * resonance } else { db }),
            Shape::LowShelf(q, db) => Analog::low_shelf(q, db),
            Shape::HighShelf(q, db) => Analog::high_shelf(q, db),
        };
        (a, hz)
    }

    /// |H(f)| of the analog prototype (the voicing target).
    pub fn analog_mag(&self, f: f64, band: f64, resonance: f64) -> f64 {
        let (a, hz) = self.prototype(band, resonance);
        a.mag2(f / hz).sqrt()
    }
}

pub const MAX_SECTIONS: usize = 8;

/// Up to [`MAX_SECTIONS`] matched sections in series (TPT SVFs, coefficients ramped on a change).
#[derive(Clone, Debug, Default)]
pub struct Body {
    secs: [Option<Section>; MAX_SECTIONS],
    svf: [Svf; MAX_SECTIONS],
    n: usize,
}

impl Body {
    /// Sets the sections (state kept where a section stays), designed at `band` / `resonance`.
    pub fn set(&mut self, sections: &[Section], band: f64, resonance: f64, sr: f64) {
        self.n = sections.len().min(MAX_SECTIONS);
        for i in 0..MAX_SECTIONS {
            self.secs[i] = sections.get(i).copied();
        }
        self.design(band, resonance, sr, 0);
    }

    /// Redesigns for `band` (octaves) and `resonance` (bell factor), ramped over `ramp` samples.
    pub fn design(&mut self, band: f64, resonance: f64, sr: f64, ramp: u32) {
        for i in 0..self.n {
            if let Some(s) = self.secs[i] {
                let (a, hz) = s.prototype(band, resonance);
                let b = design(a, hz.min(0.45 * sr), sr);
                if ramp > 0 {
                    self.svf[i].set_biquad_ramped(&b, ramp);
                } else {
                    self.svf[i].set_biquad(&b);
                }
            }
        }
    }

    pub fn reset(&mut self) {
        for s in self.svf.iter_mut() {
            s.reset();
        }
    }

    /// The analog target |H(f)|.
    pub fn analog_mag(&self, f: f64, band: f64, resonance: f64) -> f64 {
        self.secs[..self.n].iter().flatten().map(|s| s.analog_mag(f, band, resonance)).product()
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let mut y = x;
        for s in self.svf[..self.n].iter_mut() {
            y = s.process(y);
        }
        y
    }
}

/// A sine at `f` Hz, `n` samples at `sr` (tests).
#[cfg(test)]
fn tone(f: f64, amp: f64, n: usize, sr: f64) -> Vec<f64> {
    (0..n).map(|i| amp * (TAU * f * i as f64 / sr).sin()).collect()
}

/// Amplitude of the component at `f` (Hann-windowed projection).
#[cfg(test)]
fn amp_at(y: &[f64], f: f64, sr: f64) -> f64 {
    let n = y.len();
    let (mut c, mut s, mut ws) = (0.0, 0.0, 0.0);
    for (i, v) in y.iter().enumerate() {
        let w = 0.5 - 0.5 * (TAU * i as f64 / n as f64).cos();
        let ph = TAU * f * i as f64 / sr;
        c += v * w * ph.cos();
        s += v * w * ph.sin();
        ws += w;
    }
    2.0 * c.hypot(s) / ws
}

#[cfg(test)]
mod tests {
    use super::*;

    fn motor(p: MotorParams, rate: f64) -> Motor {
        let mut m = Motor::default();
        m.set(p, rate);
        m.reset();
        m
    }

    #[test]
    fn small_signal_is_the_thiele_small_high_pass() {
        let rate = 96000.0;
        for (fs, q) in [(400.0, 0.6), (650.0, 1.2), (700.0, 1.0), (120.0, 0.8)] {
            let p = MotorParams { fs_hz: fs, q_ts: q, q_ms: 4.0, beta: 0.4, x_b: 0.05, kappa1: 0.1, kappa: 2.0, kappa3: 20.0 };
            let mut worst = 0.0f64;
            for f in [50.0, 100.0, 200.0, fs, 1000.0, 2000.0, 5000.0, 10000.0, 15000.0] {
                let mut m = motor(p, rate);
                let n = (rate * 0.3) as usize;
                let x = tone(f, 1e-4, n, rate);
                let y: Vec<f64> = x.iter().map(|&u| m.process(u)).collect();
                let a = amp_at(&y[n / 2..], f, rate) / 1e-4;
                worst = worst.max((20.0 * (a / p.small_signal(f)).log10()).abs());
            }
            assert!(worst < 0.05, "f_s {fs} Q {q}: {worst:.4} dB");
        }
    }

    #[test]
    fn force_factor_stays_positive_and_bell_shaped() {
        let p = MotorParams { fs_hz: 650.0, q_ts: 1.0, q_ms: 4.0, beta: 0.5, x_b: 0.1, kappa1: 0.0, kappa: 1.0, kappa3: 0.0 };
        for i in -400..=400 {
            let x = i as f64 * 0.01;
            assert!(p.b(x) > 0.0);
            assert!(p.b(x) <= p.b(p.x_b) + 1e-12);
        }
        assert!((p.b(0.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn full_scale_square_never_diverges() {
        let rate = 96000.0;
        let n = (rate * 0.5) as usize;
        for k3 in [0.0, 5.0, 50.0] {
            for g in [1.0, 3.0, 10.0, 31.6] {
                let p = MotorParams { fs_hz: 700.0, q_ts: 0.8, q_ms: 4.0, beta: 0.5, x_b: 0.0, kappa1: 0.0, kappa: 3.0, kappa3: k3 };
                let mut m = motor(p, rate);
                let mut r = Rattle::default();
                r.prepare(rate);
                for i in 0..n {
                    let u = g * if (TAU * 20.0 * i as f64 / rate).sin() >= 0.0 { 1.0 } else { -1.0 };
                    let y = m.process(u);
                    let (x, v) = m.state();
                    let z = r.process(x, v, 1.0);
                    assert!(y.is_finite() && z.is_finite() && x.abs() <= X_CLAMP, "k3 {k3} g {g}");
                }
            }
        }
    }

    #[test]
    fn breakup_grows_with_level_and_even_harmonics_need_asymmetry() {
        let rate = 96000.0;
        let f = 300.0;
        let n = (rate * 0.4) as usize;
        let thd = |p: MotorParams, amp: f64| {
            let mut m = motor(p, rate);
            let y: Vec<f64> = tone(f, amp, n, rate).iter().map(|&u| m.process(u)).collect();
            let y = &y[n / 2..];
            let a1 = amp_at(y, f, rate);
            let h2 = amp_at(y, 2.0 * f, rate);
            let h3 = amp_at(y, 3.0 * f, rate);
            (h2.hypot(h3) / a1, h2 / a1)
        };
        let sym = MotorParams { fs_hz: 400.0, q_ts: 0.8, q_ms: 4.0, beta: 0.4, x_b: 0.0, kappa1: 0.0, kappa: 2.0, kappa3: 10.0 };
        let mut last = 0.0;
        for db in [-30.0, -20.0, -10.0, 0.0] {
            let (t, _) = thd(sym, 10f64.powf(db / 20.0));
            assert!(t > last, "THD must rise with level: {t} at {db} dB");
            last = t;
        }
        let (_, even_sym) = thd(sym, 0.5);
        let asym = MotorParams { x_b: 0.1, kappa1: 0.2, ..sym };
        let (_, even_asym) = thd(asym, 0.5);
        assert!(even_sym < 1e-4, "symmetric motor: 2nd harmonic {even_sym}");
        assert!(even_asym > 30.0 * even_sym.max(1e-6), "offset motor: 2nd harmonic {even_asym}");
    }

    #[test]
    fn no_dc_at_the_output_and_two_iterations_agree_with_eight() {
        let rate = 96000.0;
        let n = (rate * 1.0) as usize;
        let p = MotorParams { fs_hz: 350.0, q_ts: 1.0, q_ms: 4.0, beta: 0.4, x_b: 0.1, kappa1: 0.2, kappa: 2.0, kappa3: 10.0 };
        let x = tone(120.0, 1.5, n, rate);
        let mut a = motor(p, rate);
        let mut b = motor(p, rate);
        b.set_iterations(8);
        let ya: Vec<f64> = x.iter().map(|&u| a.process(u)).collect();
        let yb: Vec<f64> = x.iter().map(|&u| b.process(u)).collect();
        let mean: f64 = ya[n / 2..].iter().sum::<f64>() / (n / 2) as f64;
        let peak = ya.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        assert!(mean.abs() < 1e-3 * peak, "DC {mean} re peak {peak}");
        let rms = |y: &[f64]| (y.iter().map(|v| v * v).sum::<f64>() / y.len() as f64).sqrt();
        let d = 20.0 * (rms(&ya[n / 2..]) / rms(&yb[n / 2..])).log10();
        assert!(d.abs() < 1.0, "2 vs 8 iterations: {d:.2} dB");
    }

    #[test]
    fn throat_second_harmonic_rises_6_db_per_octave() {
        let sr = 48000.0;
        let n = 48000;
        let h2 = |f: f64, eta: f64| {
            let mut t = Throat::default();
            t.prepare(sr);
            t.set_edge(9000.0);
            t.set_eta(eta);
            let x = tone(f, 0.3, n, sr);
            let y: Vec<f64> = x.iter().map(|&v| t.process(v as f32) as f64).collect();
            let y = &y[n / 2..];
            amp_at(y, 2.0 * f, sr) / amp_at(y, f, sr)
        };
        let r = 20.0 * (h2(3000.0, 0.1) / h2(1000.0, 0.1)).log10();
        assert!((r - 9.5).abs() < 1.5, "1 → 3 kHz: {r:.2} dB");
        // η = 0.1 at a unit-peak pressure: 0.3 peak → 3 % at 1 kHz
        let a = h2(1000.0, 0.1);
        assert!((a - 0.03).abs() < 0.004, "{a}");
        assert!(h2(1000.0, 0.0) < 1e-6);
    }

    #[test]
    fn body_follows_its_analog_target() {
        let sr = 48000.0;
        let secs = [
            Section::new(Shape::HighPass(0.7), 300.0, Role::Low),
            Section::new(Shape::Bell(1.5, 5.0), 1000.0, Role::Bell),
            Section::new(Shape::LowPass(0.9), 3400.0, Role::High),
        ];
        for (band, res) in [(0.0, 1.0), (0.5, 2.0), (-1.0, 0.0)] {
            let mut b = Body::default();
            b.set(&secs, band, res, sr);
            for f in [100.0, 300.0, 1000.0, 3000.0, 6000.0] {
                let g = crate::svf::measure_gain_db(|x| b.process(x), f as f32, sr as f32) as f64;
                let t = 20.0 * b.analog_mag(f, band, res).log10();
                assert!((g - t).abs() < 0.3, "band {band} res {res} at {f}: {g:.2} vs {t:.2}");
                b.reset();
            }
        }
    }
}
