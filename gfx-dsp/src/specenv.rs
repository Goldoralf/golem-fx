// SPDX-License-Identifier: GPL-3.0-or-later
//! The spectral envelope and the filters made from it (gfx-formant §2.2). Three parts a caller takes
//! separately; f0, the voicing and the ratios come from the caller (no detector here):
//!
//! 1. [`Analyzer`]: a frame's power spectrum and its **true envelope** (Röbel & Rodet, DAFx 2005).
//!    - Windows, zero-padded into an N-point FFT (N = 2,048 at 44.1 / 48 kHz, doubled per rate family):
//!      [`Window::Periods`], a symmetric Hann of exactly k periods (k = max(3, ⌈8 ms / T⌉), sampled on
//!      its own fractional length; CheapTrick's rule), **three power spectra** T / 3 apart averaged;
//!      [`Window::Hann`], N samples (tonal frames: chords, instruments); [`Window::Short`], N / 4 samples,
//!      asymmetric (rise 3 / 4, fall 1 / 4: noise-like frames).
//!    - The log spectrum max-pooled on three centred bins to N / 4 + 1 bins, then TE: cepstral
//!      liftering at order O, the spectrum replaced by max(spectrum, envelope), repeated until the
//!      spectrum exceeds the envelope by less than 2 dB everywhere or `max_iter`. Always a **cold
//!      start** (a warm start turns TE into a peak-hold). Steppable, so a caller spreads it over samples.
//!    - [`Analyzer::flatness`]: the fine-structure flatness (the power over its ±8-bin mean: geometric
//!      over arithmetic mean, weighted by that mean), ≈ 0.56 on noise of any colour, low on partials
//!      (the frame class).
//! 2. [`Ratio`]: the log ratio L(f) in dB on the N / 2 + 1 bins: shift log E(f / α) − log E(f) (E read
//!    with linear interpolation, continued past Nyquist with its last octave's slope, ≤ 0 dB / oct),
//!    vowel templates (Peterson & Barney, five resonators, interpolated in log frequency a · e · i · o
//!    · u, energy-matched to E, moved by α), tilt (± 12 dB around 1 kHz), the boost clamp (+24 dB),
//!    the noise-floor guard (no boost where E is 60 dB under its peak, the full boost 6 dB above that
//!    floor, a ramp between: continuous, so L has no step a short FIR cannot follow), fades under 40 Hz and over
//!    0.45 fs, the level (the analysis power kept, smoothed 30 ms over hops, ±12 dB) and a weight. A
//!    **residual** mode composes only what grains that already carry E(f / α) still need (vowel, tilt).
//! 3. [`MinPhase`]: the cepstral minimum-phase FIR of L (real cepstrum, folded, exponentiated),
//!    truncated to N / 4 taps with an N / 32-tap half-Hann taper; [`KernelFade`]: the current and the
//!    previous kernel crossfaded linearly over a hop on a [`crate::fir::Fir`] history (two direct
//!    convolutions, AVX2 when present: zero latency). An identity kernel is skipped: the output is
//!    then the input, bit for bit.
//!
//! Sizes scale with the rate family (×1 at 44.1 / 48 kHz, ×2 at 88.2 / 96, ×4 at 176.4 / 192), so every
//! time constant stays the same. Memory is sized in `prepare`; nothing allocates afterwards.

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::sync::Arc;

/// FFT size, hop, FIR taps and taper at 44.1 / 48 kHz (times [`rate_scale`]).
pub const NFFT: usize = 2048;
pub const HOP: usize = 256;
pub const TAPS: usize = 512;
pub const TAPER: usize = 64;
/// TE's stop criterion (dB).
pub const TE_DELTA: f32 = 2.0;
/// Boost clamp, noise-floor guard depth, level clamp (dB).
pub const BOOST_MAX: f32 = 24.0;
pub const FLOOR_DB: f32 = 60.0;
/// The guard's ramp: the boost allowed per dB above the floor (+24 dB reached 6 dB above it).
pub const FLOOR_RAMP: f32 = 4.0;
pub const LEVEL_MAX: f64 = 12.0;
/// Level smoothing (s).
pub const LEVEL_S: f64 = 0.030;
/// Tilt clamp (dB).
pub const TILT_MAX: f32 = 12.0;

/// Vowel templates F1, F2, F3 (Hz), in the order a e i o u (Peterson & Barney 1952, men).
pub const VOWELS: [[f32; 3]; 5] = [[730.0, 1090.0, 2440.0], [530.0, 1840.0, 2480.0], [270.0, 2290.0, 3010.0], [570.0, 840.0, 2410.0], [300.0, 870.0, 2240.0]];
/// F4, F5 (fixed) and the five bandwidths (Hz).
pub const F45: [f32; 2] = [3300.0, 4500.0];
pub const BANDWIDTHS: [f32; 5] = [80.0, 90.0, 120.0, 150.0, 200.0];

/// The rate family's size factor: 1 at 44.1 / 48 kHz, 2 at 88.2 / 96, 4 at 176.4 / 192.
pub fn rate_scale(sample_rate: f32) -> usize {
    if sample_rate > 132_300.0 {
        4
    } else if sample_rate > 66_150.0 {
        2
    } else {
        1
    }
}

/// An analysis window (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Window {
    /// Symmetric Hann of k whole periods (period in samples), three spectra T / 3 apart.
    Periods(f64),
    /// Hann over N samples.
    Hann,
    /// Asymmetric, N / 4 samples (rise 3 / 4, fall 1 / 4).
    Short,
}

/// Part 1: power spectrum and true envelope.
pub struct Analyzer {
    sr: f64,
    n: usize,
    fwd: Arc<dyn RealToComplex<f32>>,
    cfwd: Arc<dyn RealToComplex<f32>>,
    cinv: Arc<dyn ComplexToReal<f32>>,
    frame: Vec<f32>,
    spec: Vec<Complex<f32>>,
    win: Vec<f32>,
    power: Vec<f32>,
    logp: Vec<f32>,
    a0: Vec<f32>,
    a: Vec<f32>,
    v: Vec<f32>,
    cep: Vec<f32>,
    cspec: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    env: Vec<f32>,
    order: usize,
    max_iter: usize,
    iter: usize,
    done: bool,
}

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl Analyzer {
    pub fn new() -> Self {
        let mut a = Self {
            sr: 48000.0,
            n: 0,
            fwd: RealFftPlanner::<f32>::new().plan_fft_forward(2),
            cfwd: RealFftPlanner::<f32>::new().plan_fft_forward(2),
            cinv: RealFftPlanner::<f32>::new().plan_fft_inverse(2),
            frame: Vec::new(),
            spec: Vec::new(),
            win: Vec::new(),
            power: Vec::new(),
            logp: Vec::new(),
            a0: Vec::new(),
            a: Vec::new(),
            v: Vec::new(),
            cep: Vec::new(),
            cspec: Vec::new(),
            scratch: Vec::new(),
            env: Vec::new(),
            order: 24,
            max_iter: 8,
            iter: 0,
            done: true,
        };
        a.prepare(48000.0);
        a
    }

    /// Plans and buffers for the rate family of `sample_rate`.
    pub fn prepare(&mut self, sample_rate: f32) {
        self.sr = sample_rate as f64;
        let n = NFFT * rate_scale(sample_rate);
        if n != self.n {
            self.n = n;
            let mut pl = RealFftPlanner::<f32>::new();
            self.fwd = pl.plan_fft_forward(n);
            self.cfwd = pl.plan_fft_forward(n / 2);
            self.cinv = pl.plan_fft_inverse(n / 2);
            let s = self.fwd.get_scratch_len().max(self.cfwd.get_scratch_len()).max(self.cinv.get_scratch_len());
            self.scratch = vec![Complex::default(); s];
            self.frame = vec![0.0; n];
            self.spec = vec![Complex::default(); n / 2 + 1];
            self.win = vec![0.0; n];
            self.power = vec![0.0; n / 2 + 1];
            self.logp = vec![0.0; n / 2 + 1];
            let np = n / 4 + 1;
            self.a0 = vec![0.0; np];
            self.a = vec![0.0; np];
            self.v = vec![0.0; np];
            self.cep = vec![0.0; n / 2];
            self.cspec = vec![Complex::default(); np];
            self.env = vec![0.0; n / 2 + 1];
        }
        self.done = true;
    }

    /// FFT size N.
    pub fn nfft(&self) -> usize {
        self.n
    }

    /// Bins of the spectrum and the envelope: N / 2 + 1.
    pub fn bins(&self) -> usize {
        self.n / 2 + 1
    }

    /// The frame's averaged power spectrum (N / 2 + 1 bins).
    pub fn power(&self) -> &[f32] {
        &self.power
    }

    /// The same in dB.
    pub fn log_power(&self) -> &[f32] {
        &self.logp
    }

    /// The true envelope (dB, N / 2 + 1 bins) after [`Self::te_finish`].
    pub fn envelope(&self) -> &[f32] {
        &self.env
    }

    /// The envelope for writing (a caller's own smoothing over frames).
    pub fn envelope_mut(&mut self) -> &mut [f32] {
        &mut self.env
    }

    /// Samples of history a window needs before its end (the three spectra included).
    pub fn span(&self, w: Window) -> usize {
        match w {
            Window::Periods(t) => {
                let (lf, _) = self.periods_len(t);
                lf.ceil() as usize + (2.0 * t / 3.0).round() as usize + 1
            }
            Window::Hann => self.n,
            Window::Short => self.n / 4,
        }
    }

    /// (fractional length, periods) of the k-period window.
    fn periods_len(&self, t: f64) -> (f64, usize) {
        let t = t.max(2.0);
        let k = 3usize.max((0.008 * self.sr / t).ceil() as usize);
        let mut lf = k as f64 * t;
        if lf > self.n as f64 {
            lf = (self.n as f64 / t).floor().max(1.0) * t;
        }
        (lf, k)
    }

    /// Fills `win[..len]`; returns len.
    fn make_window(&mut self, w: Window) -> usize {
        match w {
            Window::Periods(t) => {
                let (lf, _) = self.periods_len(t);
                let m = (lf.ceil() as usize).min(self.n);
                let off = lf - m as f64;
                for i in 0..m {
                    let tau = i as f64 + 0.5 + off;
                    self.win[i] = if tau > 0.0 { (0.5 - 0.5 * (std::f64::consts::TAU * tau / lf).cos()) as f32 } else { 0.0 };
                }
                m
            }
            Window::Hann => {
                let m = self.n;
                for i in 0..m {
                    self.win[i] = (0.5 - 0.5 * (std::f64::consts::TAU * (i as f64 + 0.5) / m as f64).cos()) as f32;
                }
                m
            }
            Window::Short => {
                let m = self.n / 4;
                let fall = m / 4;
                let rise = m - fall;
                for i in 0..rise {
                    self.win[i] = (0.5 - 0.5 * (std::f64::consts::PI * (i as f64 + 0.5) / rise as f64).cos()) as f32;
                }
                for i in 0..fall {
                    self.win[rise + i] = (0.5 + 0.5 * (std::f64::consts::PI * (i as f64 + 0.5) / fall as f64).cos()) as f32;
                }
                m
            }
        }
    }

    /// The power spectrum of the frame whose newest sample is at absolute time `end − 1` in the ring
    /// `hist` (`mask` = its length − 1). Returns the window-weighted mean square of the newest window
    /// (the frame's level). Three spectra for [`Window::Periods`], averaged in power.
    pub fn analyse(&mut self, hist: &[f32], mask: usize, end: u64, w: Window) -> f64 {
        let m = self.make_window(w);
        let sumw: f64 = self.win[..m].iter().map(|&v| v as f64).sum();
        let scale = (2.0 / sumw.max(1e-12)) as f32;
        let offsets: [f64; 3] = match w {
            Window::Periods(t) => [0.0, (t / 3.0).round(), (2.0 * t / 3.0).round()],
            _ => [0.0, f64::NAN, f64::NAN],
        };
        let count = offsets.iter().filter(|o| o.is_finite()).count();
        self.power.iter_mut().for_each(|p| *p = 0.0);
        let mut level = 0.0f64;
        for (j, &o) in offsets.iter().enumerate() {
            if !o.is_finite() {
                continue;
            }
            let e = end as i64 - o as i64;
            let start = e - m as i64;
            let mut ms = 0.0f64;
            for i in 0..m {
                let x = hist[((start + i as i64) as usize) & mask];
                ms += (x as f64) * (x as f64) * self.win[i] as f64;
                self.frame[i] = x * self.win[i] * scale;
            }
            if j == 0 {
                level = ms / sumw.max(1e-12);
            }
            self.frame[m..].iter_mut().for_each(|v| *v = 0.0);
            let _ = self.fwd.process_with_scratch(&mut self.frame, &mut self.spec, &mut self.scratch);
            let inv = 1.0 / count as f32;
            for (p, c) in self.power.iter_mut().zip(self.spec.iter()) {
                *p += c.norm_sqr() * inv;
            }
        }
        for (l, &p) in self.logp.iter_mut().zip(self.power.iter()) {
            *l = 10.0 * (p + 1e-18).log10();
        }
        level
    }

    /// Fine-structure flatness of the power spectrum between `lo` and `hi` Hz.
    pub fn flatness(&self, lo: f32, hi: f32) -> f32 {
        let nb = self.bins();
        let df = self.sr as f32 / self.n as f32;
        let k0 = ((lo / df) as usize).max(9);
        let k1 = ((hi / df) as usize).min(nb - 10);
        if k1 <= k0 {
            return 0.0;
        }
        const W: usize = 8;
        let mut s: f64 = self.power[k0 - W..=k0 + W].iter().map(|&p| p as f64).sum();
        // weighted by the local power, so empty regions between partials or above them do not count
        let (mut lg, mut ar, mut wsum) = (0.0f64, 0.0f64, 0.0f64);
        for k in k0..=k1 {
            if k > k0 {
                s += self.power[k + W] as f64 - self.power[k - W - 1] as f64;
            }
            let mean = s.max(0.0) / (2 * W + 1) as f64;
            if mean <= 1e-30 {
                continue;
            }
            let r = (self.power[k] as f64 / mean).max(1e-12);
            lg += mean * r.ln();
            ar += mean * r;
            wsum += mean;
        }
        if wsum <= 0.0 || ar <= 0.0 {
            return 0.0;
        }
        ((lg / wsum).exp() / (ar / wsum)) as f32
    }

    /// Starts TE on the current log spectrum: max-pooled (centred, three bins), cepstral order
    /// `order` (quefrency in samples), at most `max_iter` iterations.
    pub fn te_start(&mut self, order: usize, max_iter: usize) {
        let nb = self.bins();
        let np = self.a0.len();
        let lm = &self.logp;
        let at = |k: i64| -> f32 {
            let k = if k < 0 { -k } else if k >= nb as i64 { 2 * (nb as i64 - 1) - k } else { k };
            lm[k as usize]
        };
        for j in 0..np {
            let c = 2 * j as i64;
            self.a0[j] = at(c - 1).max(at(c)).max(at(c + 1));
        }
        self.a.copy_from_slice(&self.a0);
        self.order = order.clamp(1, np - 2);
        self.max_iter = max_iter.max(1);
        self.iter = 0;
        self.done = false;
    }

    /// One TE iteration; true when TE has converged or reached its cap.
    pub fn te_step(&mut self) -> bool {
        if self.done {
            return true;
        }
        let nc = self.cep.len();
        for (c, &a) in self.cspec.iter_mut().zip(self.a.iter()) {
            *c = Complex::new(a, 0.0);
        }
        let _ = self.cinv.process_with_scratch(&mut self.cspec, &mut self.cep, &mut self.scratch);
        let o = self.order;
        let inv = 1.0 / nc as f32;
        for (q, c) in self.cep.iter_mut().enumerate() {
            *c = if q <= o || q >= nc - o { *c * inv } else { 0.0 };
        }
        let _ = self.cfwd.process_with_scratch(&mut self.cep, &mut self.cspec, &mut self.scratch);
        let mut excess = f32::MIN;
        for ((v, c), &a0) in self.v.iter_mut().zip(self.cspec.iter()).zip(self.a0.iter()) {
            *v = c.re;
            excess = excess.max(a0 - *v);
        }
        self.iter += 1;
        if excess <= TE_DELTA || self.iter >= self.max_iter {
            self.done = true;
        } else {
            for ((a, &v), &a0) in self.a.iter_mut().zip(self.v.iter()).zip(self.a0.iter()) {
                *a = a0.max(v);
            }
        }
        self.done
    }

    /// TE's iterations so far.
    pub fn te_iterations(&self) -> usize {
        self.iter
    }

    /// The pooled envelope interpolated back onto the N / 2 + 1 bins.
    pub fn te_finish(&mut self) {
        let np = self.v.len();
        for k in 0..self.env.len() {
            let j = k / 2;
            self.env[k] = if k % 2 == 0 || j + 1 >= np { self.v[j.min(np - 1)] } else { 0.5 * (self.v[j] + self.v[j + 1]) };
        }
    }

    /// TE in one call: returns the iterations.
    pub fn true_envelope(&mut self, order: usize, max_iter: usize) -> usize {
        self.te_start(order, max_iter);
        while !self.te_step() {}
        self.te_finish();
        self.iter
    }
}

/// What a [`Ratio`] composes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    /// Envelope ratio α (> 1: formants up).
    pub alpha: f64,
    /// Vowel position 0 – 1 (a e i o u at 0, ¼, ½, ¾, 1) and amount 0 – 1.
    pub vowel: f32,
    pub vowel_amt: f32,
    /// Tilt, dB / octave around 1 kHz.
    pub tilt: f32,
    /// Weight of the whole ratio 0 – 1.
    pub weight: f32,
}

impl Default for Shape {
    fn default() -> Self {
        Self { alpha: 1.0, vowel: 0.0, vowel_amt: 0.0, tilt: 0.0, weight: 1.0 }
    }
}

/// Part 2: the log ratio L(f).
#[derive(Clone, Debug, Default)]
pub struct Ratio {
    sr: f64,
    nb: usize,
    hop: usize,
    l: Vec<f32>,
    vow: Vec<f32>,
    target: Vec<f32>,
    cw: Vec<[f32; 4]>,
    g_s: f64,
    g_on: bool,
}

impl Ratio {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sizes for the rate family of `sample_rate` (N / 2 + 1 bins).
    pub fn prepare(&mut self, sample_rate: f32) {
        self.sr = sample_rate as f64;
        let m = rate_scale(sample_rate);
        let n = NFFT * m;
        self.nb = n / 2 + 1;
        self.hop = HOP * m;
        self.l = vec![0.0; self.nb];
        self.vow = vec![0.0; self.nb];
        self.target = vec![0.0; self.nb];
        self.cw = (0..self.nb)
            .map(|k| {
                let w = std::f64::consts::TAU * k as f64 / n as f64;
                [w.cos() as f32, w.sin() as f32, (2.0 * w).cos() as f32, (2.0 * w).sin() as f32]
            })
            .collect();
        self.reset();
    }

    pub fn reset(&mut self) {
        self.g_s = 0.0;
        self.g_on = false;
        self.l.iter_mut().for_each(|v| *v = 0.0);
    }

    /// L (dB) of the last composition.
    pub fn l(&self) -> &[f32] {
        &self.l
    }

    /// The target envelope E + L (dB) of the last composition (a breath shaped like the new voice).
    pub fn target(&self) -> &[f32] {
        &self.target
    }

    /// The smoothed level correction (dB) folded into L.
    pub fn level_db(&self) -> f64 {
        self.g_s
    }

    /// The vowel template at position `pos` (0 – 1), dB on the bins, into `vow`.
    fn vowel_template(&mut self, pos: f32) {
        let x = (pos.clamp(0.0, 1.0) * 4.0) as f64;
        let i = (x.floor() as usize).min(3);
        let fr = x - i as f64;
        let mut f = [0.0f64; 5];
        for k in 0..3 {
            let (a, b) = (VOWELS[i][k] as f64, VOWELS[i + 1][k] as f64);
            f[k] = (a.ln() * (1.0 - fr) + b.ln() * fr).exp();
        }
        f[3] = F45[0] as f64;
        f[4] = F45[1] as f64;
        let mut coefs = [(0.0f32, 0.0f32, 0.0f32); 5];
        for k in 0..5 {
            let r = (-std::f64::consts::PI * BANDWIDTHS[k] as f64 / self.sr).exp();
            let th = std::f64::consts::TAU * f[k] / self.sr;
            let a1 = -2.0 * r * th.cos();
            let a2 = r * r;
            let b0 = 1.0 + a1 + a2;
            coefs[k] = (a1 as f32, a2 as f32, (b0 * b0) as f32);
        }
        for (v, c) in self.vow.iter_mut().zip(self.cw.iter()) {
            let mut db = 0.0f32;
            for &(a1, a2, b2) in coefs.iter() {
                let re = 1.0 + a1 * c[0] + a2 * c[2];
                let im = a1 * c[1] + a2 * c[3];
                db += 10.0 * (b2 / (re * re + im * im).max(1e-30)).log10();
            }
            *v = db;
        }
    }

    /// Composes L from the envelope `env` (dB) and the analysis power `power` (both N / 2 + 1 bins).
    /// `residual`: the source already carries E(f / α) (grains): only the vowel and the tilt are
    /// composed, against E(f / α). Returns false when L ≡ 0 (identity: the caller bypasses its filter).
    pub fn compose(&mut self, env: &[f32], power: &[f32], s: &Shape, residual: bool) -> bool {
        let nb = self.nb;
        let alpha = s.alpha.clamp(0.25, 4.0);
        let v = s.vowel_amt.clamp(0.0, 1.0);
        let identity = s.weight <= 0.0 || (v == 0.0 && s.tilt == 0.0 && (residual || alpha == 1.0));
        if identity {
            self.l.iter_mut().for_each(|x| *x = 0.0);
            self.target.copy_from_slice(&env[..nb]);
            self.g_on = false;
            self.g_s = 0.0;
            return false;
        }
        if v > 0.0 {
            self.vowel_template(s.vowel);
        }
        let slope_of = |a: &[f32]| (a[nb - 1] - a[(nb - 1) / 2]).min(0.0);
        let se = slope_of(env);
        let sv = if v > 0.0 { slope_of(&self.vow) } else { 0.0 };
        let read = |a: &[f32], pos: f64, slope: f32| -> f32 {
            let last = (nb - 1) as f64;
            if pos >= last {
                a[nb - 1] + slope * (pos / last).log2() as f32
            } else {
                let i = pos.floor() as usize;
                let f = (pos - i as f64) as f32;
                a[i] + (a[i + 1] - a[i]) * f
            }
        };
        // the vowel's energy matched to E's, weighted by the input spectrum: Σ P · 10^((V + c − E) / 10)
        // = Σ P (in power, so the template's peaks, not its skirts, carry the match)
        let mut c = 0.0f32;
        if v > 0.0 {
            let (mut num, mut den) = (0.0f64, 0.0f64);
            for k in 1..nb {
                let w = power[k] as f64;
                num += w * 10f64.powf((read(&self.vow, k as f64 / alpha, sv) - env[k]) as f64 * 0.1);
                den += w;
            }
            c = if num > 0.0 && den > 0.0 { (-10.0 * (num / den).log10()) as f32 } else { 0.0 };
        }
        let emax = env[1..].iter().fold(f32::MIN, |m, &e| m.max(e));
        let df = self.sr / (2.0 * (nb - 1) as f64);
        let nyq = 0.5 * self.sr;
        for k in 0..nb {
            let pos = k as f64 / alpha;
            let ew = read(env, pos, se);
            let mut t = ew;
            if v > 0.0 {
                t = (1.0 - v) * ew + v * (read(&self.vow, pos, sv) + c);
            }
            let f = (k as f64 * df).max(1.0);
            if s.tilt != 0.0 {
                t += (s.tilt * (f / 1000.0).log2() as f32).clamp(-TILT_MAX, TILT_MAX);
            }
            let mut l = t - if residual { ew } else { env[k] };
            // the noise-floor guard, continuous: no boost at the floor, the full boost 6 dB above it
            l = l.min((FLOOR_RAMP * (env[k] - (emax - FLOOR_DB))).max(0.0));
            l = l.min(BOOST_MAX);
            let fade = (f / 40.0).clamp(0.0, 1.0) * ((nyq - f) / (0.05 * self.sr)).clamp(0.0, 1.0);
            self.l[k] = l * fade as f32;
        }
        // the level: the analysis power kept (read at f / α for grains, which moved it)
        let (mut p0, mut p1) = (0.0f64, 0.0f64);
        for k in 0..nb {
            let p = if residual { power[((k as f64 / alpha).round() as usize).min(nb - 1)] } else { power[k] } as f64;
            p0 += p;
            p1 += p * 10f64.powf(self.l[k] as f64 * 0.1);
        }
        let g = if p0 > 0.0 && p1 > 0.0 { 10.0 * (p0 / p1).log10() } else { 0.0 };
        if self.g_on {
            let a = (-(self.hop as f64) / (LEVEL_S * self.sr)).exp();
            self.g_s = a * self.g_s + (1.0 - a) * g;
        } else {
            self.g_s = g;
            self.g_on = true;
        }
        let g = self.g_s.clamp(-LEVEL_MAX, LEVEL_MAX) as f32;
        let w = s.weight.min(1.0);
        for k in 0..nb {
            self.l[k] = (self.l[k] + g) * w;
            self.target[k] = env[k] + self.l[k];
        }
        true
    }
}

/// Part 3: the cepstral minimum-phase FIR of a log magnitude.
pub struct MinPhase {
    n: usize,
    taps: usize,
    taper: usize,
    fwd: Arc<dyn RealToComplex<f32>>,
    inv: Arc<dyn ComplexToReal<f32>>,
    buf: Vec<f32>,
    spec: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
}

impl Default for MinPhase {
    fn default() -> Self {
        Self::new()
    }
}

impl MinPhase {
    pub fn new() -> Self {
        let mut m = Self {
            n: 0,
            taps: 0,
            taper: 0,
            fwd: RealFftPlanner::<f32>::new().plan_fft_forward(2),
            inv: RealFftPlanner::<f32>::new().plan_fft_inverse(2),
            buf: Vec::new(),
            spec: Vec::new(),
            scratch: Vec::new(),
        };
        m.prepare(48000.0);
        m
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        let s = rate_scale(sample_rate);
        let n = NFFT * s;
        self.taps = TAPS * s;
        self.taper = TAPER * s;
        if n != self.n {
            self.n = n;
            let mut pl = RealFftPlanner::<f32>::new();
            self.fwd = pl.plan_fft_forward(n);
            self.inv = pl.plan_fft_inverse(n);
            self.scratch = vec![Complex::default(); self.fwd.get_scratch_len().max(self.inv.get_scratch_len())];
            self.buf = vec![0.0; n];
            self.spec = vec![Complex::default(); n / 2 + 1];
        }
    }

    pub fn taps(&self) -> usize {
        self.taps
    }

    /// The minimum-phase kernel of `l_db` (N / 2 + 1 bins), truncated and tapered, written **reversed**
    /// into `out_rev` (`taps` long, for [`crate::fir::Fir::process`]). `gain_db` is added to every bin.
    pub fn design(&mut self, l_db: &[f32], gain_db: f32, out_rev: &mut [f32]) {
        let n = self.n;
        let half = n / 2;
        let k = std::f32::consts::LN_10 / 20.0;
        for (c, &l) in self.spec.iter_mut().zip(l_db.iter()) {
            *c = Complex::new((l + gain_db) * k, 0.0);
        }
        let _ = self.inv.process_with_scratch(&mut self.spec, &mut self.buf, &mut self.scratch);
        // fold to minimum phase: c0, 2 cn (0 < n < N / 2), c(N / 2), 0 above
        let inv = 1.0 / n as f32;
        self.buf[0] *= inv;
        for q in 1..half {
            self.buf[q] *= 2.0 * inv;
        }
        self.buf[half] *= inv;
        self.buf[half + 1..].iter_mut().for_each(|v| *v = 0.0);
        let _ = self.fwd.process_with_scratch(&mut self.buf, &mut self.spec, &mut self.scratch);
        for c in self.spec.iter_mut() {
            *c = c.exp();
        }
        self.spec[0].im = 0.0;
        self.spec[half].im = 0.0;
        let _ = self.inv.process_with_scratch(&mut self.spec, &mut self.buf, &mut self.scratch);
        let taps = self.taps;
        for i in 0..taps {
            let mut h = self.buf[i] * inv;
            if i >= taps - self.taper {
                let j = i - (taps - self.taper);
                h *= (0.5 + 0.5 * (std::f64::consts::PI * (j as f64 + 0.5) / self.taper as f64).cos()) as f32;
            }
            out_rev[taps - 1 - i] = h;
        }
    }
}

/// The kernels of one filter: the previous and the current crossfaded linearly over a hop, the next
/// one prepared in a third slot. Several FIR histories (channels) share it.
#[derive(Clone, Debug, Default)]
pub struct KernelFade {
    k: [Vec<f32>; 3],
    ident: [bool; 3],
    prev: usize,
    cur: usize,
    next: usize,
    lam: f32,
    step: f32,
}

impl KernelFade {
    pub fn new() -> Self {
        Self::default()
    }

    /// Kernels of `taps` (reversed), crossfaded over `hop` samples. Resets to the identity.
    pub fn prepare(&mut self, taps: usize, hop: usize) {
        for k in self.k.iter_mut() {
            *k = vec![0.0; taps];
        }
        self.step = 1.0 / hop.max(1) as f32;
        self.reset();
    }

    pub fn reset(&mut self) {
        self.ident = [true; 3];
        self.prev = 0;
        self.cur = 1;
        self.next = 2;
        self.lam = 1.0;
    }

    /// The next kernel's slot (reversed taps), to be designed into.
    pub fn next_mut(&mut self) -> &mut [f32] {
        self.ident[self.next] = false;
        &mut self.k[self.next]
    }

    /// The next kernel is the identity.
    pub fn set_next_identity(&mut self) {
        self.ident[self.next] = true;
    }

    /// The next kernel becomes the current one; the crossfade from the current one starts.
    pub fn commit(&mut self) {
        let old = self.prev;
        self.prev = self.cur;
        self.cur = self.next;
        self.next = old;
        self.lam = if self.ident[self.prev] && self.ident[self.cur] { 1.0 } else { 0.0 };
    }

    /// Ends the crossfade at once: the current kernel alone (a filter switched in from nothing).
    pub fn settle(&mut self) {
        self.lam = 1.0;
    }

    /// Both kernels are the identity: the filter passes its input exactly.
    #[inline]
    pub fn is_identity(&self) -> bool {
        self.ident[self.cur] && (self.lam >= 1.0 || self.ident[self.prev])
    }

    /// The current kernel (reversed taps; meaningless when identity).
    pub fn current(&self) -> &[f32] {
        &self.k[self.cur]
    }

    /// Advances the crossfade by one sample (call once per sample, before `apply`).
    #[inline]
    pub fn tick(&mut self) {
        if self.lam < 1.0 {
            self.lam = (self.lam + self.step).min(1.0);
        }
    }

    /// The output on `fir`'s history (after its `push` of `x`, the newest sample).
    #[inline]
    pub fn apply(&self, fir: &crate::fir::Fir, x: f32) -> f32 {
        if self.is_identity() {
            return x;
        }
        let out = |i: usize| if self.ident[i] { x } else { fir.process(&self.k[i]) };
        if self.lam >= 1.0 {
            out(self.cur)
        } else if !self.ident[self.prev] && !self.ident[self.cur] {
            let (a, b) = fir.process2(&self.k[self.prev], &self.k[self.cur]);
            a + self.lam * (b - a)
        } else {
            let (a, b) = (out(self.prev), out(self.cur));
            a + self.lam * (b - a)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::Fir;

    /// A harmonic source (1 / k amplitudes) through a five-resonator vowel, as in the prototype.
    fn vowel_signal(f0: f64, sr: f64, n: usize, formants: &[f32; 3]) -> Vec<f32> {
        let mut x = vec![0.0f64; n];
        let mut k = 1;
        while k as f64 * f0 < 0.45 * sr {
            for (i, v) in x.iter_mut().enumerate() {
                *v += (std::f64::consts::TAU * k as f64 * f0 * i as f64 / sr).cos() / k as f64;
            }
            k += 1;
        }
        let fr = [formants[0], formants[1], formants[2], F45[0], F45[1]];
        let mut y: Vec<f64> = x;
        for (f, bw) in fr.iter().zip(BANDWIDTHS.iter()) {
            let r = (-std::f64::consts::PI * *bw as f64 / sr).exp();
            let a1 = -2.0 * r * (std::f64::consts::TAU * *f as f64 / sr).cos();
            let a2 = r * r;
            let b0 = 1.0 + a1 + a2;
            let (mut y1, mut y2) = (0.0, 0.0);
            for v in y.iter_mut() {
                let o = b0 * *v - a1 * y1 - a2 * y2;
                y2 = y1;
                y1 = o;
                *v = o;
            }
        }
        y.iter().map(|&v| (0.05 * v) as f32).collect()
    }

    fn ring(x: &[f32]) -> (Vec<f32>, usize) {
        let len = x.len().next_power_of_two();
        let mut r = vec![0.0; len];
        r[..x.len()].copy_from_slice(x);
        (r, len - 1)
    }

    /// The analytic vowel envelope (dB, resonators × the 1 / f source) at frequency f.
    fn true_env(f: f64, f0: f64, sr: f64, formants: &[f32; 3]) -> f64 {
        let fr = [formants[0], formants[1], formants[2], F45[0], F45[1]];
        let w = std::f64::consts::TAU * f / sr;
        let mut db = 20.0 * (f0 / f.max(1.0)).log10();
        for (fk, bw) in fr.iter().zip(BANDWIDTHS.iter()) {
            let r = (-std::f64::consts::PI * *bw as f64 / sr).exp();
            let a1 = -2.0 * r * (std::f64::consts::TAU * *fk as f64 / sr).cos();
            let a2 = r * r;
            let b0 = 1.0 + a1 + a2;
            let re = 1.0 + a1 * w.cos() + a2 * (2.0 * w).cos();
            let im = a1 * w.sin() + a2 * (2.0 * w).sin();
            db += 10.0 * (b0 * b0 / (re * re + im * im)).log10();
        }
        db
    }

    #[test]
    fn true_envelope_passes_through_the_harmonics() {
        // TE on the k-period window follows the vowel's envelope at the harmonics (200 Hz – 5 kHz)
        for &sr in &[48000.0f64, 96000.0] {
            for &f0 in &[100.0f64, 220.0] {
                let a = VOWELS[0];
                let x = vowel_signal(f0, sr, (0.5 * sr) as usize, &a);
                let (h, mask) = ring(&x);
                let mut an = Analyzer::new();
                an.prepare(sr as f32);
                let t = sr / f0;
                an.analyse(&h, mask, x.len() as u64, Window::Periods(t));
                let order = ((0.5 * t).floor() as usize).clamp(24 * rate_scale(sr as f32), 400 * rate_scale(sr as f32));
                let it = an.true_envelope(order, 8);
                assert!(it <= 8);
                let e = an.envelope();
                let df = sr / an.nfft() as f64;
                let mut d = Vec::new();
                let mut k = 1;
                while k as f64 * f0 < 5000.0 {
                    let f = k as f64 * f0;
                    if f >= 200.0 {
                        let b = f / df;
                        let i = b.floor() as usize;
                        let v = e[i] as f64 + (e[i + 1] - e[i]) as f64 * (b - i as f64);
                        d.push(v - true_env(f, f0, sr, &a));
                    }
                    k += 1;
                }
                let mean = d.iter().sum::<f64>() / d.len() as f64;
                let rms = (d.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / d.len() as f64).sqrt();
                assert!(rms < 2.0, "sr {sr} f0 {f0}: envelope error {rms:.2} dB rms ({it} iterations)");
            }
        }
    }

    /// A reversed kernel's response (dB) at frequency `f`.
    fn response_db(k: &[f32], f: f64, sr: f64) -> f64 {
        let w = std::f64::consts::TAU * f / sr;
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, &c) in k.iter().rev().enumerate() {
            re += c as f64 * (w * i as f64).cos();
            im -= c as f64 * (w * i as f64).sin();
        }
        10.0 * (re * re + im * im).log10()
    }

    #[test]
    fn minimum_phase_kernel_realises_its_target() {
        // README §5 FIR truncation (the review's measure): the 512-tap kernel against the full-length
        // one at the harmonics of the bench vowel (f0 100 Hz, 100 Hz – 0.45 fs, where L ≥ −20 dB and
        // above the noise-floor guard), ±4, ±7, ±12 st; and the full-length kernel against L itself on
        // those bins (the cepstral design)
        for &sr in &[44100.0f32, 48000.0, 96000.0, 192000.0] {
            let srd = sr as f64;
            let a = VOWELS[0];
            let mut seed = 5u32;
            let x: Vec<f32> = vowel_signal(100.0, srd, (0.4 * srd) as usize, &a)
                .iter()
                .map(|&v| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    v + 1e-4 * (seed as f32 / u32::MAX as f32 - 0.5)
                })
                .collect();
            let (h, mask) = ring(&x);
            let mut an = Analyzer::new();
            an.prepare(sr);
            an.analyse(&h, mask, x.len() as u64, Window::Periods(srd / 100.0));
            an.true_envelope((0.5 * srd / 100.0) as usize, 8);
            let mut rt = Ratio::new();
            rt.prepare(sr);
            let mut mp = MinPhase::new();
            mp.prepare(sr);
            let mut full = MinPhase::new();
            full.prepare(sr);
            full.taps = full.n;
            let n = an.nfft();
            let emax = an.envelope()[1..].iter().fold(f32::MIN, |m, &v| m.max(v));
            for st in [-12.0f64, -7.0, -4.0, 4.0, 7.0, 12.0] {
                let s = Shape { alpha: (st / 12.0).exp2(), ..Shape::default() };
                assert!(rt.compose(an.envelope(), an.power(), &s, false));
                let mut k = vec![0.0f32; mp.taps()];
                mp.design(rt.l(), 0.0, &mut k);
                let mut kf = vec![0.0f32; full.taps()];
                full.design(rt.l(), 0.0, &mut kf);
                let (mut trunc, mut exact, mut sq, mut cnt) = (0.0f64, 0.0f64, 0.0f64, 0usize);
                let mut f = 100.0;
                while f <= 0.45 * srd {
                    let b = (f * n as f64 / srd).round() as usize;
                    if rt.l()[b] >= -20.0 && an.envelope()[b] >= emax - FLOOR_DB - 6.0 {
                        let fb = b as f64 * srd / n as f64;
                        let (d, df) = (response_db(&k, fb, srd), response_db(&kf, fb, srd));
                        trunc = trunc.max((d - df).abs());
                        sq += (d - df) * (d - df);
                        cnt += 1;
                        exact = exact.max((df - rt.l()[b] as f64).abs());
                    }
                    f += 100.0;
                }
                let rms = (sq / cnt as f64).sqrt();
                eprintln!("sr {sr} {st} st: 512 vs full {trunc:.3} dB max, {rms:.3} rms; full vs L {exact:.3} dB");
                // measured: the cepstral design within 0.012 dB; the truncation 0.56 – 2.0 dB at the
                // worst harmonic from −7 to +12 st (0.15 – 0.49 dB rms), 2.8 – 3.3 dB at −12 st (1.2 – 1.4 rms;
                // README As coded)
                let bound = if st < -10.0 { 3.5 } else { 2.5 };
                assert!(trunc < bound && rms < bound / 2.0 && exact < 0.05, "sr {sr} {st} st: 512 vs full {trunc:.3} dB max {rms:.3} rms, full vs L {exact:.3} dB");
            }
        }
    }

    #[test]
    fn identity_composes_nothing_and_passes_bit_exact() {
        let mut rt = Ratio::new();
        rt.prepare(48000.0);
        let env = vec![-20.0f32; 1025];
        let p = vec![1.0f32; 1025];
        assert!(!rt.compose(&env, &p, &Shape::default(), false));
        assert!(rt.l().iter().all(|&v| v == 0.0));
        // tilt alone is not the identity, nor the vowel alone in residual mode
        assert!(rt.compose(&env, &p, &Shape { tilt: 1.0, ..Shape::default() }, false));
        assert!(rt.compose(&env, &p, &Shape { vowel_amt: 0.5, ..Shape::default() }, true));
        assert!(!rt.compose(&env, &p, &Shape { alpha: 1.3, ..Shape::default() }, true));
        let mut kf = KernelFade::new();
        kf.prepare(512, 256);
        let mut fir = Fir::new();
        fir.prepare(512);
        let mut seed = 3u32;
        for _ in 0..2000 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let x = seed as f32 / u32::MAX as f32 - 0.5;
            fir.push(x);
            kf.tick();
            assert_eq!(kf.apply(&fir, x).to_bits(), x.to_bits());
        }
    }

    #[test]
    fn kernel_fade_crossfades_linearly() {
        let mut kf = KernelFade::new();
        kf.prepare(4, 4);
        let mut fir = Fir::new();
        fir.prepare(4);
        // a gain of 2 (reversed kernel: the newest tap last)
        kf.next_mut().copy_from_slice(&[0.0, 0.0, 0.0, 2.0]);
        kf.commit();
        let mut ys = Vec::new();
        for _ in 0..6 {
            fir.push(1.0);
            kf.tick();
            ys.push(kf.apply(&fir, 1.0));
        }
        assert_eq!(ys, vec![1.25, 1.5, 1.75, 2.0, 2.0, 2.0]);
    }

    #[test]
    fn cold_start_te_comes_down_after_a_vowel_change() {
        // the envelope of /i/ analysed after /a/ equals /i/'s own: nothing is held
        let sr = 48000.0f64;
        let mut an = Analyzer::new();
        an.prepare(sr as f32);
        let xa = vowel_signal(150.0, sr, 24000, &VOWELS[0]);
        let xi = vowel_signal(150.0, sr, 24000, &VOWELS[2]);
        let (ha, ma) = ring(&xa);
        an.analyse(&ha, ma, xa.len() as u64, Window::Periods(320.0));
        an.true_envelope(160, 8);
        let (hi, mi) = ring(&xi);
        an.analyse(&hi, mi, xi.len() as u64, Window::Periods(320.0));
        an.true_envelope(160, 8);
        let e1: Vec<f32> = an.envelope().to_vec();
        let mut fresh = Analyzer::new();
        fresh.prepare(sr as f32);
        fresh.analyse(&hi, mi, xi.len() as u64, Window::Periods(320.0));
        fresh.true_envelope(160, 20);
        let e2 = fresh.envelope();
        let df = sr / 2048.0;
        let worst = (0..1025).filter(|&k| (200.0..5000.0).contains(&(k as f64 * df))).map(|k| (e1[k] - e2[k]).abs()).fold(0.0f32, f32::max);
        assert!(worst < 2.0, "{worst} dB");
    }

    #[test]
    fn flatness_tells_noise_from_partials() {
        let sr = 48000.0f64;
        let mut an = Analyzer::new();
        an.prepare(sr as f32);
        let mut seed = 99u32;
        let noise: Vec<f32> = (0..8192)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed as f32 / u32::MAX as f32 - 0.5
            })
            .collect();
        let (h, m) = ring(&noise);
        an.analyse(&h, m, 8192, Window::Hann);
        let fnoise = an.flatness(300.0, 8000.0);
        let chord: Vec<f32> = (0..8192)
            .map(|i| {
                [82.41f64, 123.47, 164.81, 207.65]
                    .iter()
                    .map(|f| (1..12).map(|k| ((std::f64::consts::TAU * f * k as f64 * i as f64 / sr).sin() / k as f64) as f32).sum::<f32>())
                    .sum()
            })
            .collect();
        let (h, m) = ring(&chord);
        an.analyse(&h, m, 8192, Window::Hann);
        let fchord = an.flatness(300.0, 8000.0);
        // white noise ≈ 0.6 (the exponential law, Hann-correlated bins), a dense four-note chord ≈ 0.4
        assert!(fnoise > 0.55 && fchord < 0.45, "noise {fnoise} chord {fchord}");
    }
}
