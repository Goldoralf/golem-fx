// SPDX-License-Identifier: GPL-3.0-or-later
//! Wah pedal models and the voice that plays them (shared by gfx-wah and gfx-autowah).
//!
//! Each pedal is played from its measured response — peak frequency, Q, peak gain and the level under
//! the peak at seven pedal positions — through a TPT state-variable filter (resonant low-pass +
//! band-pass), behind the pedal's own DC blocker. The tables are those golem measured on guitarix's
//! circuit models (golem_wah.jsfx, golem_colwah.jsfx, MIT); three voices are added (Classic: Smith's
//! fitted Cry Baby; Clyde: Fractal's settings; Bass 105Q).

use crate::adaa::{Adaa1, Shaper};
use crate::svf::Svf;

/// Pedal positions of the tables (the model's internal position W after its pot law).
pub const W: [f32; 7] = [0.02, 0.1, 0.25, 0.5, 0.75, 0.9, 0.98];

/// A pedal: response at the seven positions, its DC blocker and its pot law.
#[derive(Clone, Copy, Debug)]
pub struct Model {
    pub name: &'static str,
    /// peak frequency (Hz)
    pub f: [f32; 7],
    /// Q (bandwidth at −3 dB)
    pub q: [f32; 7],
    /// peak gain (dB)
    pub g: [f32; 7],
    /// gain under the peak, at 100 Hz (dB)
    pub fl: [f32; 7],
    /// DC blocker (Hz)
    pub dc: f32,
    pub law: Law,
}

/// Toe position u (0 = heel, 1 = toe) → the table's position W.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Law {
    /// W = logpot_k(1 − max(lo, u))
    InvLog(f32, f32),
    /// W = logpot_k(max(lo, u))
    Log(f32, f32),
    /// W = max(lo, u)
    Lin(f32),
    /// W = 1 − max(lo, u)
    InvLin(f32),
}

#[inline]
pub fn logpot(k: f32, x: f32) -> f32 {
    if k <= 0.0 {
        x
    } else {
        ((k * x).exp() - 1.0) / (k.exp() - 1.0)
    }
}

impl Law {
    pub fn apply(self, u: f32) -> f32 {
        match self {
            Law::InvLog(k, lo) => logpot(k, 1.0 - u.max(lo)),
            Law::Log(k, lo) => logpot(k, u.max(lo)),
            Law::Lin(lo) => u.max(lo),
            Law::InvLin(lo) => 1.0 - u.max(lo),
        }
    }
}

const fn geo(f0: f32, f1: f32) -> [f32; 7] {
    // a table from heel (W = 0.98) f0 to toe (W = 0.02) f1 is filled at run time; placeholder
    [f1, f1, f1, f0, f0, f0, f0]
}

/// The eleven voices, in the `model` enum's order (the catalogue's seven first).
pub static MODELS: [Model; 11] = [
    Model { name: "Colorsound", f: [933.0, 532.0, 362.0, 262.0, 219.0, 201.0, 195.0], q: [6.7, 8.5, 11.4, 11.1, 8.5, 8.5, 8.5], g: [24.8, 25.1, 24.7, 24.2, 24.1, 23.9, 24.1], fl: [0.5, 0.8, 1.3, 2.0, 2.7, 3.1, 3.4], dc: 157.0, law: Law::InvLog(5.0, 0.01) },
    Model { name: "Dallas", f: [1453.0, 1254.0, 1050.0, 879.0, 759.0, 715.0, 674.0], q: [5.6, 6.7, 6.7, 8.5, 8.5, 11.4, 11.1], g: [23.7, 23.4, 23.3, 23.3, 23.4, 23.2, 22.8], fl: [-4.0, -3.9, -3.9, -3.8, -3.7, -3.7, -3.7], dc: 584.0, law: Law::InvLog(5.0, 0.01) },
    Model { name: "Foxx", f: [313.0, 322.0, 352.0, 408.0, 517.0, 654.0, 805.0], q: [16.9, 11.1, 11.4, 8.5, 8.5, 6.9, 5.6], g: [28.3, 27.3, 27.2, 26.1, 24.9, 24.1, 23.5], fl: [2.0, 1.9, 1.8, 1.6, 1.3, 1.1, 1.0], dc: 228.0, law: Law::Log(1.0, 0.01) },
    Model { name: "Jen", f: [1736.0, 1082.0, 759.0, 564.0, 473.0, 433.0, 420.0], q: [3.1, 4.2, 5.6, 6.7, 6.7, 6.7, 8.5], g: [17.6, 17.7, 18.0, 18.4, 18.8, 18.8, 19.3], fl: [-4.1, -4.0, -3.8, -3.6, -3.5, -3.4, -3.4], dc: 228.0, law: Law::InvLog(5.0, 0.0) },
    Model { name: "Maestro", f: [1217.0, 933.0, 715.0, 548.0, 459.0, 420.0, 408.0], q: [3.8, 5.6, 6.9, 11.4, 16.9, 16.9, 16.9], g: [24.2, 26.5, 29.7, 33.9, 38.8, 43.2, 43.3], fl: [9.7, 9.8, 9.8, 9.9, 10.0, 10.0, 10.0], dc: 65.0, law: Law::InvLog(1.0, 0.0) },
    Model { name: "Selmer", f: [396.0, 420.0, 459.0, 548.0, 736.0, 1050.0, 1685.0], q: [6.7, 6.9, 6.9, 6.9, 4.9, 3.8, 2.4], g: [29.0, 28.9, 28.6, 28.3, 27.8, 27.4, 27.2], fl: [8.7, 8.7, 8.6, 8.4, 8.1, 8.0, 7.9], dc: 697.0, law: Law::Lin(0.0) },
    // Vox V847: guitarix runs it toe = low; here heel = low like every other pedal
    Model { name: "Vox V847", f: [1050.0, 906.0, 715.0, 581.0, 502.0, 473.0, 459.0], q: [4.8, 4.9, 6.7, 6.7, 8.5, 8.5, 6.9], g: [17.9, 17.9, 18.0, 18.3, 18.4, 18.5, 18.4], fl: [-4.2, -4.2, -4.0, -3.9, -3.7, -3.7, -3.6], dc: 228.0, law: Law::InvLin(0.0) },
    Model { name: "GCB-95", f: [1736.0, 1050.0, 736.0, 548.0, 459.0, 420.0, 408.0], q: [3.1, 4.8, 5.6, 8.5, 8.5, 8.5, 8.5], g: [18.9, 19.0, 19.3, 19.5, 19.6, 19.7, 19.7], fl: [-3.2, -3.1, -2.9, -2.7, -2.5, -2.4, -2.4], dc: 228.0, law: Law::InvLog(5.0, 0.0) },
    // Smith's fit of a Cry Baby (CCRMA): fc = 450 · 2^(2.3 u), Q = 2^(2(1 − u) + 1)
    Model { name: "Classic", f: geo(450.0, 2221.0), q: [2.0, 2.4, 2.8, 4.0, 5.7, 7.0, 7.7], g: [20.0; 7], fl: [-3.0; 7], dc: 150.0, law: Law::InvLin(0.0) },
    // Fractal's Clyde settings: 369 – 2369 Hz, resonance 4.17
    Model { name: "Clyde", f: geo(369.0, 2369.0), q: [4.17; 7], g: [20.0; 7], fl: [-3.0; 7], dc: 150.0, law: Law::InvLin(0.0) },
    // bass wah (105Q-style voicing [unverified]): lower range, lows kept
    Model { name: "Bass 105Q", f: geo(180.0, 1100.0), q: [3.0, 3.4, 4.0, 4.8, 5.4, 5.8, 6.0], g: [16.0; 7], fl: [0.0; 7], dc: 20.0, law: Law::InvLin(0.0) },
];

/// Fills the log-spaced frequency tables of the three added voices (W decreasing = towards the toe).
pub fn freq_at(m: &Model, idx: usize) -> f32 {
    match m.name {
        "Classic" => 450.0 * 2f32.powf(2.3 * (1.0 - W[idx])),
        "Clyde" => 369.0 * (2369.0f32 / 369.0).powf(1.0 - W[idx]),
        "Bass 105Q" => 180.0 * (1100.0f32 / 180.0).powf(1.0 - W[idx]),
        _ => m.f[idx],
    }
}

/// Response at position w: (fc, Q, peak dB, floor dB), interpolated (log for the frequency).
pub fn lookup(m: &Model, w: f32) -> (f32, f32, f32, f32) {
    let w = w.clamp(W[0], W[6]);
    let mut i = 0;
    while i < 5 && w > W[i + 1] {
        i += 1;
    }
    let t = (w - W[i]) / (W[i + 1] - W[i]);
    let (f0, f1) = (freq_at(m, i), freq_at(m, i + 1));
    let fc = (f0.ln() + t * (f1.ln() - f0.ln())).exp();
    let lerp = |a: &[f32; 7]| a[i] + t * (a[i + 1] - a[i]);
    (fc, lerp(&m.q), lerp(&m.g), lerp(&m.fl))
}

pub const GCB95: usize = 7;

/// The voice of one pedal: DC blocker + table-driven SVF + drive. Shared with gfx-autowah.
#[derive(Clone, Default)]
pub struct Voice {
    svf: Svf,
    dc_x: f32,
    dc_y: f32,
    adaa: Adaa1,
    v1: f32,
}

impl Voice {
    pub fn reset(&mut self) {
        self.svf.reset();
        self.dc_x = 0.0;
        self.dc_y = 0.0;
        self.adaa.reset(Shaper::Tanh);
        self.v1 = 0.0;
    }

    /// Sets the filter for model `m` at position `w`, with Q and range scaling (1 = the model's).
    pub fn set(&mut self, m: &Model, w: f32, q_scale: f32, range_scale: f32, sr: f32) {
        let (fc, q, gpk, fl) = lookup(m, w);
        let centre = (lookup(m, W[0]).0.ln() + lookup(m, W[6]).0.ln()) * 0.5;
        let fc = (centre + range_scale * (fc.ln() - centre)).exp();
        let q = (q * q_scale).max(0.3);
        let lpg = 10f32.powf(fl / 20.0);
        let peak = 10f32.powf(gpk / 20.0);
        // band-pass in quadrature with the low-pass: together they reach the measured peak
        let bpg = (peak * peak - (lpg * q).powi(2)).max(0.0).sqrt();
        self.svf.set_raw(fc, q, 0.0, bpg / q, lpg, sr);
    }

    #[inline]
    pub fn process(&mut self, x: f32, dc_coef: f32, drive: f32) -> f32 {
        let y = x - self.dc_x + dc_coef * self.dc_y;
        self.dc_x = x;
        self.dc_y = y;
        let v = self.svf.process(y);
        // the resonance clips (Holters & Zölzer): a soft clip, antialiased; its state always runs so
        // `drive` can come in without a step, and blends in with `drive` itself
        let k = 0.15 + 2.0 * drive;
        let clipped = self.adaa.process(Shaper::Tanh, v * k) / k;
        // the ADAA output sits half a sample late: blend against the same-delayed linear signal
        let v_half = 0.5 * (v + self.v1);
        self.v1 = v;
        if drive > 0.0 {
            v_half + (clipped - v_half) * (drive * 4.0).min(1.0)
        } else {
            v
        }
    }
}

