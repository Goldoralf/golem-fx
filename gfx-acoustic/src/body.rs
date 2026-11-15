// SPDX-License-Identifier: GPL-3.0-or-later
//! The body engine (README §2.5): a modal bank of TPT state-variable modes below ≈ 1.2 kHz, two real
//! gains per mode and per mic (a complex residue), and one blended high-band FIR per output channel.
//!
//! - **Mic** = a crossfade of two fitted microphones (Bridge 0, Sound hole 0.5, Neck 1): the mode
//!   gains interpolate piecewise-linearly, and so do the FIR kernels (a crossfade of two FIRs fed the
//!   same input is one FIR), the blend scaled by 1 / √(w² + (1 − w)² + 2 w (1 − w) ρ). The blend is
//!   rebuilt every [`CTL`] samples while the mic moves; the old and new kernels' outputs are
//!   crossfaded over that block and the mode gains ramped.
//! - **Size** moves every mode to f · 100 / size (Q kept, the FIR fixed); the coefficients are
//!   recomputed every [`CTL`] samples and ramped.
//! - **Force-like** bodies (measured from a force: the archive's hammer, a piezo) run D⁻¹ first, the
//!   force → reference conversion's inverse (README §2.7), so F̂ (the sound-hole EMF) drives them.
//! - Stereo: the modes' states are shared, only the gains and the FIR kernels differ (README §2.9).
//! - The bank is flushed to zero once its energy passes −200 dB on a silent input.

use crate::bodies::BODIES;
use gfx_dsp::fir::{self, Fir};
use std::f64::consts::PI;

/// Control period (samples): mic blend, size coefficients.
pub const CTL: usize = 32;
/// The kernels' length at 48 kHz.
pub const FIR_LEN: usize = 1024;
/// Most modes in a body.
pub const MAX_MODES: usize = 48;

/// One mode: frequency (Hz), Q, and per mic (bridge, sound hole, neck) the gains of its band-pass
/// (unity at the peak) and of its low-pass outputs.
#[derive(Clone, Copy, Debug)]
pub struct Mode {
    pub f: f32,
    pub q: f32,
    pub g: [[f32; 2]; 3],
}

/// A model's data (generated: src/bodies.rs, data/bodies.f32).
#[derive(Clone, Copy, Debug)]
pub struct BodyData {
    pub name: &'static str,
    pub measured: bool,
    pub force_like: bool,
    /// Correlation of the bridge–sound hole and sound hole–neck kernels.
    pub rho: [f32; 2],
    pub modes: &'static [Mode],
}

static FIR_DATA: &[u8] = include_bytes!("../data/bodies.f32");

/// Model `m`'s kernel for mic `k` (0 bridge, 1 sound hole, 2 neck) at 48 kHz.
pub fn kernel_48k(m: usize, k: usize) -> Vec<f32> {
    let off = (m * 3 + k) * FIR_LEN * 4;
    FIR_DATA[off..off + FIR_LEN * 4].as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect()
}

/// D(s) = (s + ω₀) / (s + ω_s) and its inverse (README §2.7): ω_s = c / (√2 · x_s), x_s = 200 mm
/// (121 Hz), ω₀ = 2π · 30 Hz.
pub const D_ZERO_HZ: f64 = 30.0;
pub const X_S: f64 = 0.200;

/// A first-order section (s + a) / (s + b), bilinear (a, b in Hz), transposed direct form, f64.
#[derive(Clone, Copy, Debug, Default)]
pub struct FirstOrder {
    b0: f64,
    b1: f64,
    a1: f64,
    s: f64,
}

impl FirstOrder {
    pub fn new(zero_hz: f64, pole_hz: f64, fs: f64) -> Self {
        let k = 2.0 * fs;
        let (a, b) = (2.0 * PI * zero_hz, 2.0 * PI * pole_hz);
        let d = k + b;
        Self { b0: (k + a) / d, b1: (a - k) / d, a1: (b - k) / d, s: 0.0 }
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.s;
        self.s = self.b1 * x - self.a1 * y;
        y
    }

    pub fn reset(&mut self) {
        self.s = 0.0;
    }
}

/// D⁻¹ at `fs`: force estimate from the reference.
pub fn d_inverse(fs: f64) -> FirstOrder {
    FirstOrder::new(gfx_dsp::pickup::position_corner_hz(X_S), D_ZERO_HZ, fs)
}

/// D at `fs`: force → reference.
pub fn d_forward(fs: f64) -> FirstOrder {
    FirstOrder::new(D_ZERO_HZ, gfx_dsp::pickup::position_corner_hz(X_S), fs)
}

/// The three mics' kernels of every model at the running rate, reversed for [`Fir::process`] and
/// padded to a multiple of 32 taps. Built once in `prepare`.
#[derive(Clone, Debug, Default)]
pub struct Kernels {
    pub taps: usize,
    pub k: Vec<[Vec<f32>; 3]>,
}

impl Kernels {
    pub fn build(fs: f64) -> Self {
        let taps = ((FIR_LEN as f64 * fs / 48000.0).ceil() as usize).div_ceil(32) * 32;
        let k = (0..BODIES.len())
            .map(|m| std::array::from_fn(|i| fir::reverse(&fir::resample_kernel_causal(&kernel_48k(m, i), 48000.0, fs), taps)))
            .collect();
        Self { taps, k }
    }
}

/// Interpolation of a mic position p ∈ [0, 1]: the two neighbours (0–1 or 1–2) and the weight.
pub fn mic_pair(p: f32) -> (usize, f32) {
    let x = (p.clamp(0.0, 1.0) * 2.0).min(2.0);
    if x >= 1.0 {
        (1, x - 1.0)
    } else {
        (0, x)
    }
}

/// The FIR blend's level correction for weight w on a pair of correlation ρ.
pub fn blend_scale(w: f32, rho: f32) -> f32 {
    let p = w * w + (1.0 - w) * (1.0 - w) + 2.0 * w * (1.0 - w) * rho;
    1.0 / p.max(1e-3).sqrt()
}

/// One running body (a model's bank and FIR, up to two output channels).
#[derive(Clone, Debug, Default)]
pub struct Body {
    pub model: usize,
    n: usize,
    fs: f64,
    k: Vec<f64>,
    a1: Vec<f64>,
    a2: Vec<f64>,
    a3: Vec<f64>,
    st1: Vec<f64>,
    st2: Vec<f64>,
    st3: Vec<f64>,
    ramp: u32,
    ic1: Vec<f64>,
    ic2: Vec<f64>,
    /// gains per channel: band-pass (× k folded in) and low-pass, their ramp steps
    ga: [Vec<f64>; 2],
    gb: [Vec<f64>; 2],
    sa: [Vec<f64>; 2],
    sb: [Vec<f64>; 2],
    gramp: u32,
    fir: Fir,
    cur: [Vec<f32>; 2],
    next: [Vec<f32>; 2],
    /// the FIR crossfade over the current control block (samples done, active per channel)
    xf: [bool; 2],
    xf_n: usize,
    mic: [f32; 2],
    size: f32,
    dinv: FirstOrder,
    force_like: bool,
    peak: f64,
    channels: usize,
}

impl Body {
    /// Allocates for any model at `fs` with kernels of `taps`.
    pub fn prepare(&mut self, fs: f64, taps: usize, channels: usize) {
        self.fs = fs;
        self.channels = channels.clamp(1, 2);
        for v in [&mut self.k, &mut self.a1, &mut self.a2, &mut self.a3, &mut self.st1, &mut self.st2, &mut self.st3, &mut self.ic1, &mut self.ic2] {
            *v = vec![0.0; MAX_MODES];
        }
        for c in 0..2 {
            self.ga[c] = vec![0.0; MAX_MODES];
            self.gb[c] = vec![0.0; MAX_MODES];
            self.sa[c] = vec![0.0; MAX_MODES];
            self.sb[c] = vec![0.0; MAX_MODES];
            self.cur[c] = vec![0.0; taps];
            self.next[c] = vec![0.0; taps];
        }
        self.fir.prepare(taps);
        self.dinv = d_inverse(fs);
    }

    /// Loads `model` at mic positions and size (no allocation), states cleared.
    pub fn load(&mut self, model: usize, kernels: &Kernels, mic: [f32; 2], size: f32) {
        let d = &BODIES[model];
        self.model = model;
        self.n = d.modes.len().min(MAX_MODES);
        self.force_like = d.force_like;
        for (i, m) in d.modes.iter().take(self.n).enumerate() {
            self.k[i] = 1.0 / m.q as f64;
        }
        self.size = size;
        self.set_coefs(size, false);
        self.mic = mic;
        for c in 0..2 {
            self.set_gains(c, mic[c], false);
            self.blend_into(c, mic[c], kernels, false);
        }
        self.xf = [false; 2];
        self.reset();
    }

    /// Starts this body's FIR on `other`'s input history (a model crossfade: the incoming kernel is
    /// warm, so the switch does not ring its high band as a sudden onset would).
    pub fn warm_from(&mut self, other: &Body) {
        self.fir.copy_history_from(&other.fir);
    }

    pub fn reset(&mut self) {
        for i in 0..MAX_MODES {
            self.ic1[i] = 0.0;
            self.ic2[i] = 0.0;
        }
        self.fir.reset();
        self.dinv.reset();
        self.peak = 0.0;
    }

    fn set_coefs(&mut self, size: f32, ramp: bool) {
        let d = &BODIES[self.model];
        let scale = 100.0 / size.clamp(50.0, 200.0) as f64;
        for (i, m) in d.modes.iter().take(self.n).enumerate() {
            let f = (m.f as f64 * scale).min(0.45 * self.fs);
            let g = (PI * f / self.fs).tan();
            let k = self.k[i];
            let a1 = 1.0 / (1.0 + g * (g + k));
            let a2 = g * a1;
            let a3 = g * a2;
            if ramp {
                let r = 1.0 / CTL as f64;
                self.st1[i] = (a1 - self.a1[i]) * r;
                self.st2[i] = (a2 - self.a2[i]) * r;
                self.st3[i] = (a3 - self.a3[i]) * r;
            } else {
                self.a1[i] = a1;
                self.a2[i] = a2;
                self.a3[i] = a3;
            }
        }
        self.ramp = if ramp { CTL as u32 } else { 0 };
    }

    fn set_gains(&mut self, c: usize, p: f32, ramp: bool) {
        let d = &BODIES[self.model];
        let (j, w) = mic_pair(p);
        let w = w as f64;
        for (i, m) in d.modes.iter().take(self.n).enumerate() {
            let a = ((1.0 - w) * m.g[j][0] as f64 + w * m.g[j + 1][0] as f64) * self.k[i];
            let b = (1.0 - w) * m.g[j][1] as f64 + w * m.g[j + 1][1] as f64;
            if ramp {
                self.sa[c][i] = (a - self.ga[c][i]) / CTL as f64;
                self.sb[c][i] = (b - self.gb[c][i]) / CTL as f64;
            } else {
                self.ga[c][i] = a;
                self.gb[c][i] = b;
            }
        }
    }

    fn blend_into(&mut self, c: usize, p: f32, kernels: &Kernels, to_next: bool) {
        let (j, w) = mic_pair(p);
        let rho = BODIES[self.model].rho[j];
        let ks = &kernels.k[self.model];
        let out = if to_next { &mut self.next[c] } else { &mut self.cur[c] };
        fir::blend(out, &ks[j], &ks[j + 1], w, blend_scale(w, rho));
    }

    /// Control tick (every [`CTL`] samples): mic positions per channel, size.
    pub fn control(&mut self, mic: [f32; 2], size: f32, kernels: &Kernels) {
        if size != self.size {
            self.size = size;
            self.set_coefs(size, true);
        }
        let mut moved = false;
        for c in 0..self.channels {
            self.xf[c] = false;
            if mic[c] != self.mic[c] {
                self.mic[c] = mic[c];
                self.set_gains(c, mic[c], true);
                self.blend_into(c, mic[c], kernels, true);
                self.xf[c] = true;
                moved = true;
            }
        }
        self.gramp = if moved { CTL as u32 } else { 0 };
        self.xf_n = 0;
        // flush: a silent input and a bank under −200 dB
        if self.peak < 1e-12 {
            let e: f64 = (0..self.n).map(|i| self.ic1[i] * self.ic1[i] + self.ic2[i] * self.ic2[i]).sum();
            if e < 1e-20 && e > 0.0 {
                for i in 0..self.n {
                    self.ic1[i] = 0.0;
                    self.ic2[i] = 0.0;
                }
            }
        }
        self.peak = 0.0;
    }

    /// One sample of F̂: (modal, FIR) per channel.
    #[inline]
    pub fn process(&mut self, x: f64) -> ([f64; 2], [f64; 2]) {
        let x = if self.force_like { self.dinv.process(x) } else { x };
        self.peak = self.peak.max(x.abs());
        let n = self.n;
        if self.ramp > 0 {
            self.ramp -= 1;
            for i in 0..n {
                self.a1[i] += self.st1[i];
                self.a2[i] += self.st2[i];
                self.a3[i] += self.st3[i];
            }
        }
        let stereo = self.channels == 2;
        if self.gramp > 0 {
            self.gramp -= 1;
            for c in 0..self.channels {
                if self.xf[c] {
                    for i in 0..n {
                        self.ga[c][i] += self.sa[c][i];
                        self.gb[c][i] += self.sb[c][i];
                    }
                }
            }
        }
        let (mut y0, mut y1) = (0.0f64, 0.0f64);
        {
            let (a1, a2, a3) = (&self.a1[..n], &self.a2[..n], &self.a3[..n]);
            let (ic1, ic2) = (&mut self.ic1[..n], &mut self.ic2[..n]);
            let (ga0, gb0) = (&self.ga[0][..n], &self.gb[0][..n]);
            if stereo {
                let (ga1, gb1) = (&self.ga[1][..n], &self.gb[1][..n]);
                for i in 0..n {
                    let v3 = x - ic2[i];
                    let v1 = a1[i] * ic1[i] + a2[i] * v3;
                    let v2 = ic2[i] + a2[i] * ic1[i] + a3[i] * v3;
                    ic1[i] = 2.0 * v1 - ic1[i];
                    ic2[i] = 2.0 * v2 - ic2[i];
                    y0 += ga0[i] * v1 + gb0[i] * v2;
                    y1 += ga1[i] * v1 + gb1[i] * v2;
                }
            } else {
                for i in 0..n {
                    let v3 = x - ic2[i];
                    let v1 = a1[i] * ic1[i] + a2[i] * v3;
                    let v2 = ic2[i] + a2[i] * ic1[i] + a3[i] * v3;
                    ic1[i] = 2.0 * v1 - ic1[i];
                    ic2[i] = 2.0 * v2 - ic2[i];
                    y0 += ga0[i] * v1 + gb0[i] * v2;
                }
            }
        }
        // the high band
        self.fir.push(x as f32);
        let mut h = [0.0f64; 2];
        let u = (self.xf_n as f32 + 1.0) / CTL as f32;
        for c in 0..self.channels {
            if c == 1 && !self.xf[0] && !self.xf[1] && self.mic[0] == self.mic[1] {
                h[1] = h[0];
                break;
            }
            h[c] = if self.xf[c] {
                let (a, b) = self.fir.process2(&self.cur[c], &self.next[c]);
                (a + (b - a) * u) as f64
            } else {
                self.fir.process(&self.cur[c]) as f64
            };
        }
        self.xf_n += 1;
        if self.xf_n == CTL {
            for c in 0..self.channels {
                if self.xf[c] {
                    std::mem::swap(&mut self.cur[c], &mut self.next[c]);
                    self.xf[c] = false;
                }
            }
        }
        ([y0, y1], h)
    }
}
