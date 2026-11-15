// SPDX-License-Identifier: GPL-3.0-or-later
//! A backward-adaptive narrowband speech coder on 8 kHz virtual samples (gfx-radio §2.3, "Cell Codec"),
//! zero latency: everything it does comes from audio already heard.
//!
//! - **Frame clock**: 160 virtual samples (20 ms); the caller gives the position in the frame (one clock
//!   for every channel and both chain slots) and whether the frame is lost.
//! - [`NoiseSuppressor`] (TX side): 7 band-pass detectors at half-octave centres 350 – 2,800 Hz measure
//!   each band's frame energy; a minimum-statistics floor per band (the 1.5 s minimum of the frame
//!   levels); a band within 6 dB of its floor is cut on the next frame by up to `max_cut` dB through a
//!   cascade of 7 TPT bells (Q 2) at the same centres, ramped over 3 ms. A cut band whose energy over
//!   the last 2.5 ms rises above its floor + 6 dB opens again at once (ramped): without it every onset
//!   after a pause lost up to 23 dB for one frame. A 0 dB TPT bell is an exact identity.
//! - [`Predictor`]: GSM 06.10's short-term stage made backward-adaptive (the G.728 way): an order-8
//!   autocorrelation LPC of the frame that just ended (Hamming window, 60 Hz lag window, Levinson-
//!   Durbin, |k| ≤ 0.98) drives an **analysis lattice** (signal → residual e) and a **synthesis lattice**
//!   with the reflection coefficients quantised in the log-area-ratio domain (step q). The residual is
//!   coarsened toward a regular pulse grid (one sample in three, × 3) by α. Both lattices move to the new
//!   coefficients over 3 ms together. At q = 0, α = 0 the pair is an identity sample by sample.
//! - [`GilbertElliott`]: bursty frame losses (mean burst 2 frames).
//! - [`Plc`]: packet-loss concealment on the decoder's output (ITU-T G.711 App. I's schedule, zero
//!   latency): the last pitch period repeated, its last quarter overlap-added with the quarter period
//!   before it (causal), a 2 ms crossfade in from the live signal, unattenuated for 10 ms, then −20 % per
//!   10 ms to silence at 60 ms, and a quarter-period crossfade back on return.

use crate::detect::PitchDetector;
use crate::lfo::Rng;
use crate::svf::{Svf, SvfMode};
use std::f64::consts::PI;

/// The virtual rate (Hz).
pub const RATE: f64 = 8000.0;
/// Samples per frame (20 ms).
pub const FRAME: usize = 160;
/// LPC order.
pub const ORDER: usize = 8;
/// Coefficient ramp (3 ms).
pub const RAMP: usize = 24;
/// Reflection coefficients are clamped to this.
pub const K_MAX: f64 = 0.98;
/// The noise suppressor's band centres (Hz).
pub const NS_BANDS: [f64; 7] = [350.0, 495.0, 700.0, 990.0, 1400.0, 1980.0, 2800.0];
/// The minimum-statistics window (frames: 1.5 s).
pub const NS_FRAMES: usize = 75;
/// A band within this of its floor (dB) is noise.
pub const NS_MARGIN_DB: f64 = 6.0;
/// The suppressor's onset check (samples: 2.5 ms).
pub const NS_SUB: usize = 20;
/// The PLC history (decoder output, 200 ms).
pub const HISTORY: usize = 1600;

/// Order-8 autocorrelation LPC of one frame: Hamming window, Gaussian 60 Hz lag window, a white-noise
/// correction of 1e-4, Levinson-Durbin with |k| ≤ 0.98. Returns the reflection coefficients (the
/// sign convention of [`Predictor`]'s lattices: f_m = f_(m−1) + k_m · b_(m−1)[n − 1]).
pub fn lpc(frame: &[f64; FRAME], window: &[f64; FRAME], lag: &[f64; ORDER + 1]) -> [f64; ORDER] {
    let mut w = [0.0; FRAME];
    for i in 0..FRAME {
        w[i] = frame[i] * window[i];
    }
    let mut r = [0.0; ORDER + 1];
    for l in 0..=ORDER {
        let mut s = 0.0;
        for i in 0..FRAME - l {
            s += w[i] * w[i + l];
        }
        r[l] = s * lag[l];
    }
    r[0] = r[0] * 1.0001 + 1e-12;
    let mut a = [0.0; ORDER + 1];
    a[0] = 1.0;
    let mut e = r[0];
    let mut ks = [0.0; ORDER];
    for i in 1..=ORDER {
        let mut acc = r[i];
        for j in 1..i {
            acc += a[j] * r[i - j];
        }
        let k = if e > 1e-12 { (-acc / e).clamp(-K_MAX, K_MAX) } else { 0.0 };
        ks[i - 1] = k;
        let prev = a;
        for j in 1..i {
            a[j] = prev[j] + k * prev[i - j];
        }
        a[i] = k;
        e *= 1.0 - k * k;
    }
    ks
}

/// k quantised uniformly in the log-area-ratio domain (LAR = ln((1 + k) / (1 − k))) with step `q`.
#[inline]
pub fn quantise_lar(k: f64, q: f64) -> f64 {
    if q <= 0.0 {
        return k;
    }
    let lar = ((1.0 + k) / (1.0 - k)).ln();
    let l = (lar / q).round() * q;
    (0.5 * l).tanh().clamp(-K_MAX, K_MAX)
}

/// The analysis / synthesis lattice pair.
#[derive(Clone, Debug)]
pub struct Predictor {
    frame: [f64; FRAME],
    window: [f64; FRAME],
    lag: [f64; ORDER + 1],
    ka: [f64; ORDER],
    ka_t: [f64; ORDER],
    ks: [f64; ORDER],
    ks_t: [f64; ORDER],
    /// the analysis lattice's delayed backward errors
    fa: [f64; ORDER],
    /// the synthesis lattice's
    bs: [f64; ORDER],
    /// samples into the current ramp (RAMP = done)
    ramp: usize,
    /// the quantised coefficients of the frame in use (tests)
    last_ks: [f64; ORDER],
}

impl Default for Predictor {
    fn default() -> Self {
        let mut window = [0.0; FRAME];
        for (i, w) in window.iter_mut().enumerate() {
            *w = 0.54 - 0.46 * (2.0 * PI * i as f64 / (FRAME - 1) as f64).cos();
        }
        let mut lag = [0.0; ORDER + 1];
        for (l, v) in lag.iter_mut().enumerate() {
            let x = 2.0 * PI * 60.0 / RATE * l as f64;
            *v = (-0.5 * x * x).exp();
        }
        Self { frame: [0.0; FRAME], window, lag, ka: [0.0; ORDER], ka_t: [0.0; ORDER], ks: [0.0; ORDER], ks_t: [0.0; ORDER], fa: [0.0; ORDER], bs: [0.0; ORDER], ramp: RAMP, last_ks: [0.0; ORDER] }
    }
}

impl Predictor {
    pub fn reset(&mut self) {
        *self = Self { window: self.window, lag: self.lag, ..Self::default() };
    }

    /// The quantised coefficients of the frame in use.
    pub fn coefficients(&self) -> &[f64; ORDER] {
        &self.last_ks
    }

    /// One virtual sample at frame position `pos`, with the LAR step `q` and the excitation mix `alpha`.
    /// At `pos` 0 the frame that just ended is analysed and the ramp to its coefficients starts.
    #[inline]
    pub fn process(&mut self, x: f64, pos: usize, q: f64, alpha: f64, grid: bool) -> f64 {
        if pos == 0 {
            // the old ramp (if any) lands, then the new one starts
            self.ka = self.ka_t;
            self.ks = self.ks_t;
            let k = lpc(&self.frame, &self.window, &self.lag);
            self.ka_t = k;
            for m in 0..ORDER {
                self.ks_t[m] = quantise_lar(k[m], q);
            }
            self.last_ks = self.ks_t;
            self.ramp = 0;
        }
        self.frame[pos] = x;
        let mut ka = self.ka_t;
        let mut ks = self.ks_t;
        if self.ramp < RAMP {
            self.ramp += 1;
            let s = self.ramp as f64 / RAMP as f64;
            for m in 0..ORDER {
                ka[m] = self.ka[m] + (self.ka_t[m] - self.ka[m]) * s;
                ks[m] = self.ks[m] + (self.ks_t[m] - self.ks[m]) * s;
            }
        }
        // analysis lattice: x → e
        let mut f = x;
        let mut bprev = x;
        let mut nb = [0.0; ORDER];
        for m in 0..ORDER {
            let fnew = f + ka[m] * self.fa[m];
            let bnew = self.fa[m] + ka[m] * f;
            nb[m] = bprev;
            bprev = bnew;
            f = fnew;
        }
        self.fa = nb;
        let mut e = f;
        // excitation coarsening toward the regular pulse grid
        if alpha > 0.0 {
            let ep = if grid { 3.0 * e } else { 0.0 };
            e += alpha * (ep - e);
        }
        // synthesis lattice: e → y
        let mut f = e;
        for m in (0..ORDER).rev() {
            f -= ks[m] * self.bs[m];
            if m < ORDER - 1 {
                self.bs[m + 1] = self.bs[m] + ks[m] * f;
            }
        }
        self.bs[0] = f;
        f
    }
}

/// The band noise suppressor (TX side).
#[derive(Clone, Debug)]
pub struct NoiseSuppressor {
    det: [Svf; 7],
    bell: [Svf; 7],
    energy: [f64; 7],
    /// the last 2.5 ms's energy, and the floor of the frame in use (dB)
    sub: [f64; 7],
    floor: [f64; 7],
    levels: [[f64; NS_FRAMES]; 7],
    at: usize,
    /// bell gains: current, ramp start and target (dB)
    gain: [f64; 7],
    from: [f64; 7],
    target: [f64; 7],
    ramp: usize,
}

impl Default for NoiseSuppressor {
    fn default() -> Self {
        let mut det: [Svf; 7] = Default::default();
        let mut bell: [Svf; 7] = Default::default();
        for b in 0..7 {
            det[b].set(SvfMode::BandPass, NS_BANDS[b] as f32, 2.0, 0.0, RATE as f32);
            bell[b].set(SvfMode::Bell, NS_BANDS[b] as f32, 2.0, 0.0, RATE as f32);
        }
        Self { det, bell, energy: [0.0; 7], sub: [0.0; 7], floor: [f64::INFINITY; 7], levels: [[f64::INFINITY; NS_FRAMES]; 7], at: 0, gain: [0.0; 7], from: [0.0; 7], target: [0.0; 7], ramp: RAMP }
    }
}

impl NoiseSuppressor {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The bells' current gains (dB).
    pub fn gains(&self) -> &[f64; 7] {
        &self.gain
    }

    /// One virtual sample at frame position `pos`; `max_cut` dB (≥ 0) is the deepest cut.
    #[inline]
    pub fn process(&mut self, x: f64, pos: usize, max_cut: f64) -> f64 {
        if pos == 0 {
            for b in 0..7 {
                let l = 10.0 * (self.energy[b] / FRAME as f64 + 1e-14).log10();
                self.levels[b][self.at] = l;
                let floor = self.levels[b].iter().fold(f64::INFINITY, |m, v| m.min(*v));
                let t = if max_cut > 0.0 && l - floor < NS_MARGIN_DB { -max_cut } else { 0.0 };
                self.floor[b] = floor;
                self.from[b] = self.gain[b];
                self.target[b] = t;
                self.energy[b] = 0.0;
            }
            self.at = (self.at + 1) % NS_FRAMES;
            self.ramp = 0;
        }
        let xf = x as f32;
        for b in 0..7 {
            let d = self.det[b].process(xf) as f64;
            self.energy[b] += d * d;
            self.sub[b] += d * d;
        }
        if pos % NS_SUB == NS_SUB - 1 {
            // speech in a cut band: open it now rather than a frame late
            let mut open = false;
            for b in 0..7 {
                let l = 10.0 * (self.sub[b] / NS_SUB as f64 + 1e-14).log10();
                if self.target[b] < 0.0 && l - self.floor[b] >= NS_MARGIN_DB {
                    self.target[b] = 0.0;
                    open = true;
                }
                self.sub[b] = 0.0;
            }
            if open {
                self.from = self.gain;
                self.ramp = 0;
            }
        }
        if self.ramp < RAMP {
            self.ramp += 1;
            let s = self.ramp as f64 / RAMP as f64;
            for b in 0..7 {
                let g = self.from[b] + (self.target[b] - self.from[b]) * s;
                if g != self.gain[b] {
                    self.gain[b] = g;
                    self.bell[b].set(SvfMode::Bell, NS_BANDS[b] as f32, 2.0, g as f32, RATE as f32);
                }
            }
        }
        let mut y = xf;
        for b in 0..7 {
            y = self.bell[b].process(y);
        }
        y as f64
    }
}

/// The two-state Gilbert–Elliott loss process (bad = lost), stepped once per frame.
#[derive(Clone, Debug)]
pub struct GilbertElliott {
    rng: Rng,
    bad: bool,
    p_gb: f64,
    p_bg: f64,
}

impl Default for GilbertElliott {
    fn default() -> Self {
        Self { rng: Rng::new(0x6E10_55E5), bad: false, p_gb: 0.0, p_bg: 0.5 }
    }
}

impl GilbertElliott {
    /// Loss rate (0 – < 1) and mean burst length (frames ≥ 1).
    pub fn set(&mut self, rate: f64, burst: f64) {
        let rate = rate.clamp(0.0, 0.95);
        self.p_bg = 1.0 / burst.max(1.0);
        self.p_gb = rate * self.p_bg / (1.0 - rate);
    }

    pub fn reset(&mut self) {
        self.rng.reset();
        self.bad = false;
    }

    /// The next frame: lost or not.
    #[inline]
    pub fn step(&mut self) -> bool {
        let u = self.rng.next_f64();
        self.bad = if self.bad { u >= self.p_bg } else { u < self.p_gb };
        self.bad
    }
}

/// The largest period the concealment repeats (virtual samples: 75 Hz).
const P_MAX: usize = 112;
/// The unvoiced repeat (10 ms).
const P_UNVOICED: usize = 80;
/// The crossfade from the live signal into the repeat (2 ms).
const ENTRY: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq)]
enum PlcState {
    Live,
    Lost,
    Return,
}

/// Packet-loss concealment on the decoder's output.
#[derive(Clone)]
pub struct Plc {
    hist: Vec<f64>,
    at: usize,
    det: PitchDetector,
    period: usize,
    rep: [f64; P_MAX],
    p: usize,
    q: usize,
    t: usize,
    state: PlcState,
    /// samples into the return crossfade
    back: usize,
}

impl Default for Plc {
    fn default() -> Self {
        Self { hist: Vec::new(), at: 0, det: PitchDetector::default(), period: P_UNVOICED, rep: [0.0; P_MAX], p: P_UNVOICED, q: 20, t: 0, state: PlcState::Live, back: 0 }
    }
}

impl Plc {
    /// Allocates the history and the pitch detector (75 – 1,000 Hz on the virtual samples).
    pub fn prepare(&mut self) {
        self.hist = vec![0.0; HISTORY];
        self.det.prepare_decimated(RATE as f32, 75.0, 1000.0, true);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.hist.iter_mut().for_each(|v| *v = 0.0);
        self.at = 0;
        self.det.reset();
        self.period = P_UNVOICED;
        self.p = P_UNVOICED;
        self.state = PlcState::Live;
        self.t = 0;
        self.back = 0;
    }

    /// The period the next loss would repeat (virtual samples).
    pub fn period(&self) -> usize {
        self.period
    }

    /// Whether a repeat is playing.
    pub fn concealing(&self) -> bool {
        self.state != PlcState::Live
    }

    #[inline]
    fn past(&self, back: usize) -> f64 {
        self.hist[(self.at + HISTORY - back) % HISTORY]
    }

    /// The G.711 App. I attenuation t samples into a loss.
    #[inline]
    pub fn attenuation(t: usize) -> f64 {
        if t < 80 {
            1.0
        } else {
            (1.0 - 0.2 * (t - 80) as f64 / 80.0).max(0.0)
        }
    }

    #[inline]
    fn repeat(&self) -> f64 {
        self.rep[self.t % self.p] * Self::attenuation(self.t)
    }

    /// One decoder sample `x`; `lost` for the current frame. Returns what the listener hears.
    #[inline]
    pub fn process(&mut self, x: f64, lost: bool) -> f64 {
        let y = match (self.state, lost) {
            (PlcState::Live, false) => x,
            (PlcState::Live, true) | (PlcState::Return, true) => {
                if self.state == PlcState::Live {
                    // freeze the last period; its last quarter fades toward the quarter before it
                    let p = self.period.clamp(8, P_MAX);
                    let q = (p / 4).max(2);
                    for j in 0..p {
                        let a = self.past(p - j);
                        self.rep[j] = if j >= p - q {
                            let w = (j + 1 - (p - q)) as f64 / (q + 1) as f64;
                            a + (self.past(2 * p - j) - a) * w
                        } else {
                            a
                        };
                    }
                    self.p = p;
                    self.q = q;
                    self.t = 0;
                }
                self.state = PlcState::Lost;
                let r = self.repeat();
                let w = ((self.t + 1) as f64 / ENTRY as f64).min(1.0);
                self.t += 1;
                x + (r - x) * w
            }
            (PlcState::Lost, true) => {
                let r = self.repeat();
                let w = ((self.t + 1) as f64 / ENTRY as f64).min(1.0);
                self.t += 1;
                x + (r - x) * w
            }
            (PlcState::Lost, false) | (PlcState::Return, false) => {
                if self.state == PlcState::Lost {
                    self.state = PlcState::Return;
                    self.back = 0;
                }
                let r = self.repeat();
                self.t += 1;
                self.back += 1;
                let w = self.back as f64 / (self.q + 1) as f64;
                if self.back >= self.q {
                    self.state = PlcState::Live;
                }
                r + (x - r) * w.min(1.0)
            }
        };
        self.hist[self.at] = y;
        self.at = (self.at + 1) % HISTORY;
        if self.det.push_decimated(y as f32) {
            self.period = match self.det.hz() {
                Some(hz) if hz > 0.0 => ((RATE / hz as f64).round() as usize).clamp(8, P_MAX),
                _ => P_UNVOICED,
            };
        }
        y
    }
}

/// The coder's settings from gfx-radio's `codec` (0 – 100 %): LAR step, excitation mix, deepest cut.
pub fn law(codec_pct: f64) -> (f64, f64, f64) {
    let n = codec_pct.clamp(0.0, 100.0);
    (0.01 * n, 0.005 * n, 0.3 * n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic voiced signal with gliding formants and unvoiced gaps (the review's prototype).
    fn voice(n: usize) -> Vec<f64> {
        let mut rng = Rng::new(7);
        let mut y = Vec::with_capacity(n);
        let mut ph = 0.0;
        let mut src_lp = 0.0;
        let mut st = [[0.0f64; 2]; 3];
        let formants = [(700.0, 1200.0, 2600.0), (300.0, 2300.0, 3000.0), (500.0, 900.0, 2500.0), (400.0, 1900.0, 2600.0)];
        for i in 0..n {
            let t = i as f64 / RATE;
            let f0 = 110.0 * 2f64.powf(0.3 * (2.0 * PI * 0.7 * t).sin());
            let last = ph;
            ph += f0 / RATE;
            let pulse = if ph.floor() > last.floor() { 1.0 } else { 0.0 };
            src_lp = 0.95 * src_lp + pulse;
            let voiced = (2.0 * PI * 0.5 * t + 1.0).sin() > -0.5;
            let mut x = if voiced { 0.1 * src_lp } else { 0.03 * rng.bipolar() as f64 };
            let a = t * 2.5;
            let k = a as usize % formants.len();
            let k2 = (k + 1) % formants.len();
            let fr = a.fract();
            let fs = [formants[k].0 * (1.0 - fr) + formants[k2].0 * fr, formants[k].1 * (1.0 - fr) + formants[k2].1 * fr, formants[k].2 * (1.0 - fr) + formants[k2].2 * fr];
            for j in 0..3 {
                let r = (-PI * (60.0 + 40.0 * j as f64) / RATE).exp();
                let c = 2.0 * r * (2.0 * PI * fs[j] / RATE).cos();
                let v = (1.0 - r) * x + c * st[j][0] - r * r * st[j][1];
                st[j][1] = st[j][0];
                st[j][0] = v;
                x = v;
            }
            y.push(x);
        }
        let peak = y.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        y.iter().map(|v| v / peak * 0.5).collect()
    }

    fn lvl(x: &[f64]) -> f64 {
        10.0 * (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64 + 1e-30).log10()
    }

    fn code(x: &[f64], q: f64, alpha: f64) -> (Vec<f64>, f64) {
        let mut p = Predictor::default();
        let mut kmax = 0.0f64;
        let y = x
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                let y = p.process(v, i % FRAME, q, alpha, i % 3 == 0);
                kmax = p.coefficients().iter().fold(kmax, |m, k| m.max(k.abs()));
                y
            })
            .collect();
        (y, kmax)
    }

    #[test]
    fn identity_at_codec_zero() {
        let x = voice(64000);
        let (y, _) = code(&x, 0.0, 0.0);
        let e: Vec<f64> = x.iter().zip(&y).map(|(a, b)| a - b).collect();
        let r = lvl(&e) - lvl(&x);
        assert!(r < -180.0, "residue {r:.1} dB");
    }

    #[test]
    fn coarse_settings_stay_bounded() {
        let x = voice(64000);
        for (q, a, lo, hi) in [(0.5, 0.0, -8.0, 8.0), (0.5, 0.25, -9.0, 9.0), (0.5, 0.5, -10.0, 10.0)] {
            let (y, kmax) = code(&x, q, a);
            assert!(kmax <= K_MAX + 1e-12);
            let e: Vec<f64> = x.iter().zip(&y).map(|(a, b)| a - b).collect();
            let err = lvl(&e) - lvl(&x);
            assert!(err < 0.0 && err > -40.0, "q {q} α {a}: error {err:.1} dB");
            let mut worst = (0.0f64, 0.0f64);
            for f in (FRAME..x.len() - FRAME).step_by(FRAME) {
                if lvl(&x[f..f + FRAME]) > -50.0 {
                    let d = lvl(&y[f..f + FRAME]) - lvl(&x[f..f + FRAME]);
                    worst = (worst.0.min(d), worst.1.max(d));
                }
            }
            assert!(worst.0 > lo && worst.1 < hi, "q {q} α {a}: frame levels {worst:?}");
        }
    }

    #[test]
    fn suppressor_is_an_identity_at_zero_and_cuts_steady_noise() {
        let mut rng = Rng::new(5);
        let x: Vec<f64> = (0..32000).map(|_| (0.01 * rng.bipolar()) as f64).collect();
        let mut ns = NoiseSuppressor::default();
        for (i, &v) in x.iter().enumerate() {
            let xq = v as f32 as f64;
            assert_eq!(ns.process(xq, i % FRAME, 0.0), xq);
        }
        let mut ns = NoiseSuppressor::default();
        let y: Vec<f64> = x.iter().enumerate().map(|(i, &v)| ns.process(v, i % FRAME, 15.0)).collect();
        let d = lvl(&y[16000..]) - lvl(&x[16000..]);
        assert!(d < -3.0, "steady noise only {d:.1} dB down");
        for g in ns.gains() {
            assert!(*g <= 0.0 && *g >= -15.0);
        }
    }

    #[test]
    fn gilbert_elliott_rate_and_bursts() {
        let mut g = GilbertElliott::default();
        g.set(0.3, 2.0);
        let n = 30000; // 10 minutes of frames
        let (mut lost, mut bursts, mut prev) = (0usize, 0usize, false);
        for _ in 0..n {
            let l = g.step();
            if l {
                lost += 1;
                if !prev {
                    bursts += 1;
                }
            }
            prev = l;
        }
        let rate = lost as f64 / n as f64;
        let burst = lost as f64 / bursts as f64;
        assert!((rate - 0.3).abs() < 0.03, "rate {rate}");
        assert!((burst - 2.0).abs() < 0.2, "burst {burst}");
        g.set(0.0, 2.0);
        g.reset();
        assert!((0..10000).all(|_| !g.step()));
    }

    #[test]
    fn plc_repeats_the_period_with_the_g711_schedule() {
        let mut p = Plc::default();
        p.prepare();
        let f0 = 8000.0 / 61.0;
        let n = 8000;
        let x: Vec<f64> = (0..n).map(|i| 0.3 * (2.0 * PI * f0 * i as f64 / RATE).sin() + 0.1 * (4.0 * PI * f0 * i as f64 / RATE).sin()).collect();
        let loss = 4800..4800 + 640;
        let y: Vec<f64> = x.iter().enumerate().map(|(i, &v)| p.process(v, loss.contains(&i))).collect();
        // the repeat holds the period (±2 %) and the level for 10 ms
        let s = loss.start;
        for t in ENTRY..80 {
            assert!((y[s + t] - x[s + t]).abs() < 0.03, "t {t}: {} vs {}", y[s + t], x[s + t]);
        }
        // −20 % per 10 ms: 30 ms in, about 0.6
        let pk = |a: usize| y[s + a..s + a + 61].iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let ref_pk = x[s..s + 61].iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let r = pk(240) / ref_pk;
        assert!((r - 0.6).abs() < 0.1, "30 ms: {r}");
        assert!(y[s + 480..s + 640].iter().all(|v| *v == 0.0), "silence after 60 ms");
        // back to live
        let e = loss.end + 40;
        assert!((y[e] - x[e]).abs() < 1e-12);
        // no step on entry or return: sample-to-sample change stays within the signal's own
        let max_step = x.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f64::max);
        for i in s - 2..e {
            assert!((y[i + 1] - y[i]).abs() < 1.6 * max_step, "step at {i}");
        }
    }

    #[test]
    fn law_is_the_readme_s() {
        assert_eq!(law(50.0), (0.5, 0.25, 15.0));
        assert_eq!(law(0.0), (0.0, 0.0, 0.0));
    }
}
