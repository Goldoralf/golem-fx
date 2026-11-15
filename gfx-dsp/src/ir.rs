// SPDX-License-Identifier: GPL-3.0-or-later
//! Offline impulse-response preparation (gfx-cab README §2.2): channel mapping, sinc resampling, onset
//! trim and auto-polarity, length and fade, minimum phase, correlation alignment of a pair, peak or
//! loudness normalisation. Pure f64, deterministic, no I/O: the caller decodes the file and partitions
//! the result (`convolve::Kernel`). Runs off the audio thread; everything here allocates.
//!
//! - [`resample`]: Kaiser-windowed sinc (β = 10, 64 zero crossings per side), cut-off (−6 dB) at half
//!   the input rate going up, 0.45 × the output rate going down; scaled by from / to so the IR's
//!   frequency response keeps its level. Rational ratios use a phase table.
//! - [`onset_trim`]: onset = first sample at −20 dB re the peak, moved back 0.5 ms; the samples before
//!   it are dropped and a raised-cosine fade-in covers the first 0.25 ms kept (nothing when less than
//!   0.5 ms precedes the onset).
//! - [`polarity`]: the sign of the first local extremum that reaches −12 dB under the peak.
//! - [`trim_fade`], [`drop_tail`]: length cut with a half-Hann fade over the last `fade` of the kept
//!   part; trailing samples under −120 dB re the peak removed.
//! - [`min_phase`]: real cepstrum on an FFT of 8× the length (≥ 2¹⁴), log floor −140 dB re the peak.
//! - [`align_correlation`]: band-pass 80 Hz – 3 kHz, cross-correlation over ±2 ms, the positive peak
//!   unless the negative one is more than 1.5 dB larger, parabolic sub-sample lag.
//! - [`loudness_gain`]: the power gain weighted by a pink spectrum over a band, on ≥ 2¹⁶ points.

use realfft::num_complex::Complex;
use realfft::RealFftPlanner;
use std::f64::consts::PI;

/// A decoded IR: its rate and its channels.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub rate: f64,
    pub channels: Vec<Vec<f64>>,
}

impl Source {
    pub fn mono(rate: f64, h: Vec<f64>) -> Self {
        Self { rate, channels: vec![h] }
    }

    /// Length in samples (the longest channel).
    pub fn len(&self) -> usize {
        self.channels.iter().map(|c| c.len()).max().unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// `align` (gfx-cab; enum order final).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Align {
    Raw,
    Onset,
    Correlation,
}

/// `normalize` (enum order final).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Normalize {
    Off,
    Peak,
    Loudness,
}

/// How a file's channels feed the class's outputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChannelMap {
    /// gfx-cab: averaged to mono, except a 2-channel file on two outputs (ch 1 → L, ch 2 → R).
    Cab,
    /// gfx-irfx: 1 ch on both, 2 ch L / R, 4 ch plays channels 1 and 4; one output averages the two.
    Fx,
}

/// Everything a preparation depends on besides the file and the rate (part of the cache key).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub align: Align,
    pub normalize: Normalize,
    pub min_phase: bool,
    pub reverse: bool,
    /// `length` (s); the class limit applies on top.
    pub length_s: f64,
    /// `fade`, 0 – 1.
    pub fade: f64,
    /// The class limit (s).
    pub max_s: f64,
    /// Loudness band (Hz) and reference (dB).
    pub band: (f64, f64),
    pub ref_db: f64,
    pub map: ChannelMap,
    pub out_channels: usize,
}

impl Settings {
    /// A stable key for caches (bit patterns of the floats).
    pub fn key(&self) -> String {
        format!(
            "{:?}/{:?}/{}/{}/{:x}/{:x}/{:x}/{:x}/{:x}/{:x}/{:?}/{}",
            self.align,
            self.normalize,
            self.min_phase as u8,
            self.reverse as u8,
            self.length_s.to_bits(),
            self.fade.to_bits(),
            self.max_s.to_bits(),
            self.band.0.to_bits(),
            self.band.1.to_bits(),
            self.ref_db.to_bits(),
            self.map,
            self.out_channels
        )
    }
}

/// Modified Bessel function of the first kind, order 0 (series).
fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let q = x * x / 4.0;
    for k in 1..200 {
        term *= q / (k as f64 * k as f64);
        sum += term;
        if term < sum * 1e-17 {
            break;
        }
    }
    sum
}

const BETA: f64 = 10.0;
const ZERO_CROSSINGS: f64 = 64.0;

/// Windowed sinc of cut-off `cut` (cycles per sample, −6 dB) and half-width `half` samples, at `t`.
#[inline]
fn wsinc(t: f64, cut: f64, half: f64, i0b: f64) -> f64 {
    if t.abs() >= half {
        return 0.0;
    }
    let x = 2.0 * cut * t;
    let s = if x.abs() < 1e-12 { 1.0 } else { (PI * x).sin() / (PI * x) };
    let r = t / half;
    2.0 * cut * s * bessel_i0(BETA * (1.0 - r * r).max(0.0).sqrt()) / i0b
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// Resamples an IR from `from` to `to` Hz (see the module docs), starting at the IR's time 0: the
/// interpolator's precursor before the first sample is not kept. Equal rates return a copy.
pub fn resample(x: &[f64], from: f64, to: f64) -> Vec<f64> {
    resample_lead(x, from, to).0
}

/// [`resample`] keeping the interpolator's precursor: output sample i is time (i − lead) / `to`.
/// Returns (samples, lead). The onset trim then keeps 0.5 ms of it.
pub fn resample_full(x: &[f64], from: f64, to: f64) -> (Vec<f64>, usize) {
    if from == to || x.is_empty() {
        return (x.to_vec(), 0);
    }
    let lead = (half_width(from, to) * to / from).ceil() as usize;
    resample_core(x, from, to, lead)
}

fn resample_lead(x: &[f64], from: f64, to: f64) -> (Vec<f64>, usize) {
    if from == to || x.is_empty() {
        return (x.to_vec(), 0);
    }
    resample_core(x, from, to, 0)
}

fn half_width(from: f64, to: f64) -> f64 {
    let fc = if to > from { 0.5 * from } else { 0.45 * to };
    ZERO_CROSSINGS / (2.0 * fc / from)
}

fn resample_core(x: &[f64], from: f64, to: f64, lead: usize) -> (Vec<f64>, usize) {
    let fc = if to > from { 0.5 * from } else { 0.45 * to };
    let cut = fc / from;
    let half = half_width(from, to);
    let ratio = from / to;
    let scale = from / to;
    let i0b = bessel_i0(BETA);
    let n_out = ((x.len() as f64 + half) / ratio).ceil() as usize + lead;
    let jmax = half.ceil() as i64 + 1;
    let mut y = vec![0.0; n_out];
    let (fi, ti) = (from.round() as i64, to.round() as i64);
    let integral = (fi as f64 - from).abs() < 1e-9 && (ti as f64 - to).abs() < 1e-9;
    let phases = if integral { ti / gcd(fi as u64, ti as u64) as i64 } else { i64::MAX };
    let taps = (2 * jmax + 1) as usize;
    if phases <= 4096 {
        let p = phases as usize;
        let mut table = vec![0.0; p * taps];
        for ph in 0..p {
            let t = ph as f64 * ratio;
            let frac = t - t.floor();
            for j in -jmax..=jmax {
                table[ph * taps + (j + jmax) as usize] = wsinc(frac - j as f64, cut, half, i0b) * scale;
            }
        }
        for (i, yn) in y.iter_mut().enumerate() {
            // n · from / to exactly in integers
            let n = i as i64 - lead as i64;
            let base = (n * fi).div_euclid(ti);
            let ph = n.rem_euclid(p as i64) as usize;
            let row = &table[ph * taps..(ph + 1) * taps];
            let mut s = 0.0;
            for (k, c) in row.iter().enumerate() {
                let i = base + k as i64 - jmax;
                if i >= 0 && (i as usize) < x.len() {
                    s += x[i as usize] * c;
                }
            }
            *yn = s;
        }
    } else {
        for (i, yn) in y.iter_mut().enumerate() {
            let t = (i as f64 - lead as f64) * ratio;
            let base = t.floor() as i64;
            let frac = t - base as f64;
            let mut s = 0.0;
            for j in -jmax..=jmax {
                let i = base + j;
                if i >= 0 && (i as usize) < x.len() {
                    s += x[i as usize] * wsinc(frac - j as f64, cut, half, i0b);
                }
            }
            *yn = s * scale;
        }
    }
    (y, lead)
}

/// Delays `h` by `d` ≥ 0 samples: an exact shift for an integer, windowed sinc otherwise.
pub fn frac_delay(h: &[f64], d: f64) -> Vec<f64> {
    let d = d.max(0.0);
    let di = d.floor() as usize;
    let fr = d - di as f64;
    if fr < 1e-9 {
        let mut y = vec![0.0; di];
        y.extend_from_slice(h);
        return y;
    }
    let cut = 0.5;
    let half = ZERO_CROSSINGS;
    let i0b = bessel_i0(BETA);
    let j = half as i64 + 1;
    let n = h.len() + di + j as usize + 1;
    let mut y = vec![0.0; n];
    for (m, ym) in y.iter_mut().enumerate() {
        let t = m as f64 - d;
        let base = t.floor() as i64;
        let mut s = 0.0;
        for k in base - j..=base + j + 1 {
            if k >= 0 && (k as usize) < h.len() {
                s += h[k as usize] * wsinc(t - k as f64, cut, half, i0b);
            }
        }
        *ym = s;
    }
    y
}

fn peak_of(chs: &[Vec<f64>]) -> f64 {
    chs.iter().flatten().fold(0.0f64, |m, v| m.max(v.abs()))
}

/// Onset trim (see the module docs), the same cut on every channel. Returns the samples dropped.
pub fn onset_trim(chs: &mut [Vec<f64>], sr: f64) -> usize {
    let peak = peak_of(chs);
    if peak <= 0.0 {
        return 0;
    }
    let thr = peak * 0.1;
    let first = chs.iter().filter_map(|c| c.iter().position(|v| v.abs() >= thr)).min().unwrap_or(0);
    let back = (0.0005 * sr).round() as usize;
    if first < back {
        return 0;
    }
    let cut = first - back;
    if cut == 0 {
        return 0;
    }
    let fade = ((0.00025 * sr).round() as usize).max(1);
    for c in chs.iter_mut() {
        c.drain(..cut.min(c.len()));
        for (i, v) in c.iter_mut().take(fade).enumerate() {
            let w = 0.5 - 0.5 * (PI * i as f64 / fade as f64).cos();
            *v *= w;
        }
    }
    cut
}

/// The sign (±1) of the first local extremum that reaches −12 dB under the peak.
pub fn polarity(h: &[f64]) -> f64 {
    let peak = h.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    if peak <= 0.0 {
        return 1.0;
    }
    let thr = peak * 10f64.powf(-12.0 / 20.0);
    let Some(mut i) = h.iter().position(|v| v.abs() >= thr) else { return 1.0 };
    // walk to the extremum of this lobe (same sign, growing)
    while i + 1 < h.len() && h[i + 1].signum() == h[i].signum() && h[i + 1].abs() >= h[i].abs() {
        i += 1;
    }
    if h[i] < 0.0 {
        -1.0
    } else {
        1.0
    }
}

/// Cuts to `len` samples when shorter than the IR, with a half-Hann fade over the last `fade` (0 – 1)
/// of the kept part.
pub fn trim_fade(chs: &mut [Vec<f64>], len: usize, fade: f64) {
    let longest = chs.iter().map(|c| c.len()).max().unwrap_or(0);
    if len >= longest {
        return;
    }
    let nf = ((fade.clamp(0.0, 1.0) * len as f64).round() as usize).min(len);
    for c in chs.iter_mut() {
        c.truncate(len);
        let l = c.len();
        if nf > 0 && l > 0 {
            let start = len - nf;
            for i in start..l {
                let x = (i - start + 1) as f64 / nf as f64;
                c[i] *= 0.5 + 0.5 * (PI * x).cos();
            }
        }
    }
}

/// Removes trailing samples under −120 dB re the peak (the same length on every channel).
pub fn drop_tail(chs: &mut [Vec<f64>]) {
    let peak = peak_of(chs);
    let thr = peak * 1e-6;
    let last = chs.iter().filter_map(|c| c.iter().rposition(|v| v.abs() > thr)).max().map_or(0, |i| i + 1);
    for c in chs.iter_mut() {
        c.truncate(last);
    }
}

/// The minimum-phase version of `h`, same length (real cepstrum).
pub fn min_phase(h: &[f64]) -> Vec<f64> {
    let len = h.len();
    if len == 0 {
        return Vec::new();
    }
    let n = (8 * len).next_power_of_two().max(1 << 14);
    let mut pl = RealFftPlanner::<f64>::new();
    let f = pl.plan_fft_forward(n);
    let fi = pl.plan_fft_inverse(n);
    let mut buf = vec![0.0; n];
    buf[..len].copy_from_slice(h);
    let mut spec = f.make_output_vec();
    f.process(&mut buf, &mut spec).expect("fft");
    let pk = spec.iter().fold(0.0f64, |m, c| m.max(c.norm()));
    let floor = (pk * 1e-7).max(1e-300);
    let mut lm: Vec<Complex<f64>> = spec.iter().map(|c| Complex::new(c.norm().max(floor).ln(), 0.0)).collect();
    let mut cep = vec![0.0; n];
    fi.process(&mut lm, &mut cep).expect("fft");
    let inv_n = 1.0 / n as f64;
    // fold: causal part doubled
    let mut w = vec![0.0; n];
    w[0] = cep[0] * inv_n;
    for k in 1..n / 2 {
        w[k] = 2.0 * cep[k] * inv_n;
    }
    w[n / 2] = cep[n / 2] * inv_n;
    let mut c = f.make_output_vec();
    f.process(&mut w, &mut c).expect("fft");
    for z in c.iter_mut() {
        let m = z.re.exp();
        *z = Complex::new(m * z.im.cos(), m * z.im.sin());
    }
    c[0].im = 0.0;
    c[n / 2].im = 0.0;
    let mut out = vec![0.0; n];
    fi.process(&mut c, &mut out).expect("fft");
    out.truncate(len);
    out.iter_mut().for_each(|v| *v *= inv_n);
    out
}

/// A second-order RBJ section (f64), for the alignment's band-pass.
struct Bq {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}

impl Bq {
    fn new(hp: bool, f: f64, sr: f64) -> Self {
        let w = 2.0 * PI * f / sr;
        let (s, c) = w.sin_cos();
        let al = s / (2.0 * std::f64::consts::FRAC_1_SQRT_2);
        let a0 = 1.0 + al;
        let b = if hp { [(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0] } else { [(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0] };
        Self { b: [b[0] / a0, b[1] / a0, b[2] / a0], a: [-2.0 * c / a0, (1.0 - al) / a0], z: [0.0; 2] }
    }

    fn run(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

fn band_80_3k(x: &[f64], sr: f64) -> Vec<f64> {
    let mut hp = Bq::new(true, 80.0, sr);
    let mut lp = Bq::new(false, 3000.0, sr);
    let mut y: Vec<f64> = x.iter().map(|&v| lp.run(hp.run(v))).collect();
    // ring-out of the filters past the end (a few ms)
    for _ in 0..(0.02 * sr) as usize {
        y.push(lp.run(hp.run(0.0)));
    }
    y
}

/// The result of [`align_correlation`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Alignment {
    /// B's lag behind A (samples, sub-sample): positive = B is late, so A gets delayed.
    pub lag: f64,
    /// B must be inverted.
    pub flip: bool,
}

/// Aligns B to A (see the module docs).
pub fn align_correlation(a: &[f64], b: &[f64], sr: f64) -> Alignment {
    let win = ((0.5 * sr) as usize).max(1);
    let fa = band_80_3k(&a[..a.len().min(win)], sr);
    let fb = band_80_3k(&b[..b.len().min(win)], sr);
    let lmax = (0.002 * sr).round() as i64;
    let r: Vec<f64> = (-lmax..=lmax)
        .map(|tau| {
            let mut s = 0.0;
            for (n, &x) in fa.iter().enumerate() {
                let m = n as i64 + tau;
                if m >= 0 && (m as usize) < fb.len() {
                    s += x * fb[m as usize];
                }
            }
            s
        })
        .collect();
    let (mut ip, mut pmax) = (0usize, f64::MIN);
    let (mut ineg, mut nmin) = (0usize, f64::MAX);
    for (i, &v) in r.iter().enumerate() {
        if v > pmax {
            pmax = v;
            ip = i;
        }
        if v < nmin {
            nmin = v;
            ineg = i;
        }
    }
    let flip = -nmin > pmax.max(0.0) * 10f64.powf(1.5 / 20.0);
    let (i, sign) = if flip { (ineg, -1.0) } else { (ip, 1.0) };
    let mut frac = 0.0;
    if i > 0 && i + 1 < r.len() {
        let (y0, y1, y2) = (sign * r[i - 1], sign * r[i], sign * r[i + 1]);
        let den = y0 - 2.0 * y1 + y2;
        if den.abs() > 1e-300 {
            frac = (0.5 * (y0 - y2) / den).clamp(-0.5, 0.5);
        }
    }
    Alignment { lag: i as f64 - lmax as f64 + frac, flip }
}

/// The gain that brings the IR's power gain, weighted by pink noise over `band`, to 0 dB (the mean
/// over channels).
pub fn loudness_gain(chs: &[Vec<f64>], sr: f64, band: (f64, f64)) -> f64 {
    let p = weighted_power(chs, sr, band);
    if p <= 0.0 {
        1.0
    } else {
        1.0 / p.sqrt()
    }
}

/// The IR's power gain weighted by pink noise over `band` (linear power, mean over channels).
pub fn weighted_power(chs: &[Vec<f64>], sr: f64, band: (f64, f64)) -> f64 {
    let len = chs.iter().map(|c| c.len()).max().unwrap_or(0);
    if len == 0 {
        return 0.0;
    }
    let n = len.next_power_of_two().max(1 << 16);
    let mut pl = RealFftPlanner::<f64>::new();
    let f = pl.plan_fft_forward(n);
    let mut sum = 0.0;
    let mut wsum = 0.0;
    for c in chs {
        let mut buf = vec![0.0; n];
        buf[..c.len()].copy_from_slice(c);
        let mut spec = f.make_output_vec();
        f.process(&mut buf, &mut spec).expect("fft");
        for (k, z) in spec.iter().enumerate().skip(1) {
            let fr = k as f64 * sr / n as f64;
            if fr >= band.0 && fr <= band.1 {
                let w = 1.0 / fr;
                sum += w * z.norm_sqr();
                wsum += w;
            }
        }
    }
    if wsum > 0.0 {
        sum / wsum
    } else {
        0.0
    }
}

/// The file's channels for the class's outputs (see [`ChannelMap`]).
pub fn map_channels(src: &Source, map: ChannelMap, out: usize) -> Vec<Vec<f64>> {
    let chs = &src.channels;
    let avg = |idx: &[usize]| -> Vec<f64> {
        let len = idx.iter().map(|&i| chs[i].len()).max().unwrap_or(0);
        let mut y = vec![0.0; len];
        for &i in idx {
            for (d, s) in y.iter_mut().zip(&chs[i]) {
                *d += s;
            }
        }
        let k = 1.0 / idx.len() as f64;
        y.iter_mut().for_each(|v| *v *= k);
        y
    };
    if chs.is_empty() {
        return vec![Vec::new(); out];
    }
    let n = chs.len();
    match map {
        ChannelMap::Cab => {
            if out == 2 && n == 2 {
                vec![chs[0].clone(), chs[1].clone()]
            } else if n == 1 {
                vec![chs[0].clone(); out]
            } else {
                let all: Vec<usize> = (0..n).collect();
                vec![avg(&all); out]
            }
        }
        ChannelMap::Fx => {
            let pair: Vec<usize> = match n {
                1 => vec![0, 0],
                2 | 3 => vec![0, 1],
                _ => vec![0, 3],
            };
            if out == 1 {
                if pair[0] == pair[1] {
                    vec![chs[0].clone()]
                } else {
                    vec![avg(&pair)]
                }
            } else {
                vec![chs[pair[0]].clone(), chs[pair[1]].clone()]
            }
        }
    }
}

/// Steps 1 – 5 of §2.2 on one IR: channels, resampling, onset and polarity, length, minimum phase.
pub fn shape(src: &Source, sr: f64, s: &Settings) -> Vec<Vec<f64>> {
    // the interpolator's precursor is kept when the onset trim (or the minimum phase) follows; Raw
    // keeps the file's time origin
    let keep = s.min_phase || s.align != Align::Raw;
    let mut chs: Vec<Vec<f64>> = map_channels(src, s.map, s.out_channels)
        .iter()
        .map(|c| if keep { resample_full(c, src.rate, sr).0 } else { resample(c, src.rate, sr) })
        .collect();
    if !s.min_phase && s.align != Align::Raw {
        onset_trim(&mut chs, sr);
        let mix: Vec<f64> = sum_channels(&chs);
        if polarity(&mix) < 0.0 {
            chs.iter_mut().flatten().for_each(|v| *v = -*v);
        }
    }
    let max = (s.max_s * sr).round() as usize;
    let len = ((s.length_s * sr).round() as usize).clamp(1, max.max(1));
    trim_fade(&mut chs, len, s.fade);
    drop_tail(&mut chs);
    if s.min_phase {
        for c in chs.iter_mut() {
            *c = min_phase(c);
        }
    }
    chs
}

fn sum_channels(chs: &[Vec<f64>]) -> Vec<f64> {
    let len = chs.iter().map(|c| c.len()).max().unwrap_or(0);
    let mut y = vec![0.0; len];
    for c in chs {
        for (d, s) in y.iter_mut().zip(c) {
            *d += s;
        }
    }
    y
}

/// Steps 7 (and `reverse`) on a shaped IR, then f32.
pub fn finish(mut chs: Vec<Vec<f64>>, sr: f64, s: &Settings) -> Vec<Vec<f32>> {
    if s.reverse {
        chs.iter_mut().for_each(|c| c.reverse());
    }
    let g = match s.normalize {
        Normalize::Off => 1.0,
        Normalize::Peak => {
            let p = peak_of(&chs);
            if p > 0.0 {
                1.0 / p
            } else {
                1.0
            }
        }
        Normalize::Loudness => loudness_gain(&chs, sr, s.band) * 10f64.powf(s.ref_db / 20.0),
    };
    chs.iter().map(|c| c.iter().map(|v| (v * g) as f32).collect()).collect()
}

/// One IR, whole chain.
pub fn prepare(src: &Source, sr: f64, s: &Settings) -> Vec<Vec<f32>> {
    finish(shape(src, sr, s), sr, s)
}

/// A pair in `align` Correlation: B aligned to A (the leading one delayed, B flipped when the
/// correlation asks), each then normalised on its own. Other modes prepare each alone.
pub fn prepare_pair(a: &Source, b: &Source, sr: f64, s: &Settings) -> (Vec<Vec<f32>>, Vec<Vec<f32>>, Option<Alignment>) {
    let mut ca = shape(a, sr, s);
    let mut cb = shape(b, sr, s);
    if s.align != Align::Correlation {
        return (finish(ca, sr, s), finish(cb, sr, s), None);
    }
    let al = align_correlation(&sum_channels(&ca), &sum_channels(&cb), sr);
    if al.flip {
        cb.iter_mut().flatten().for_each(|v| *v = -*v);
    }
    if al.lag > 0.0 {
        ca = ca.iter().map(|c| frac_delay(c, al.lag)).collect();
    } else if al.lag < 0.0 {
        cb = cb.iter().map(|c| frac_delay(c, -al.lag)).collect();
    }
    let max = (s.max_s * sr).round() as usize;
    for c in ca.iter_mut().chain(cb.iter_mut()) {
        c.truncate(max);
    }
    drop_tail(&mut ca);
    drop_tail(&mut cb);
    (finish(ca, sr, s), finish(cb, sr, s), Some(al))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms_db_diff(a: &[f64], b: &[f64]) -> f64 {
        let e: f64 = a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum();
        let p: f64 = b.iter().map(|y| y * y).sum();
        10.0 * (e / p).log10()
    }

    /// |H(f)| of an IR at `sr` (dB).
    fn mag_db(h: &[f64], sr: f64, f: f64) -> f64 {
        let (mut re, mut im) = (0.0, 0.0);
        for (k, v) in h.iter().enumerate() {
            let w = -2.0 * PI * f * k as f64 / sr;
            re += v * w.cos();
            im += v * w.sin();
        }
        10.0 * (re * re + im * im).log10()
    }

    /// A synthetic "cab": a spike, then seeded noise through a decaying resonance pair, 30 ms, starting
    /// at `delay_s` (a band-limited fractional start: the same shape at any delay).
    fn synth(sr: f64, delay_s: f64) -> Vec<f64> {
        let n = (0.03 * sr) as usize;
        let d = delay_s * sr;
        let mut seed = 99u64;
        let noise: Vec<f64> = (0..n)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                ((seed >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
            })
            .collect();
        let proto: Vec<f64> = (0..n)
            .map(|k| {
                let t = k as f64 / sr;
                let ring = (-t / 0.004).exp() * ((2.0 * PI * 900.0 * t).sin() + 0.5 * (2.0 * PI * 2500.0 * t).cos());
                let spike = if k == 1 { 1.0 } else if k == 2 { 0.6 } else { 0.0 };
                spike + 0.3 * ring + 0.3 * (-t / 0.003).exp() * noise[k]
            })
            .collect();
        let mut y = frac_delay(&proto, d);
        y.truncate(n);
        y
    }

    #[test]
    fn resampling_response() {
        // sparse random taps over 5 ms: the resampled IR's response equals the source's (its DTFT)
        // through the pass band
        for (from, to, top) in [(44100.0, 48000.0, 20000.0), (48000.0, 44100.0, 19000.0), (96000.0, 48000.0, 20000.0), (48000.0, 192000.0, 20000.0)] {
            let mut x = vec![0.0f64; (0.005 * from) as usize];
            let mut seed = 12345u64;
            for _ in 0..20 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let k = (seed >> 33) as usize % x.len();
                x[k] += ((seed >> 11) % 1000) as f64 / 500.0 - 1.0;
            }
            let (h, lead) = resample_full(&x, from, to);
            let h = &h[lead..];
            // the precursor before time 0 is the interpolator's: count it back in
            let pre: Vec<f64> = resample_full(&x, from, to).0[..lead].to_vec();
            // complex error against the source's response, relative to its RMS level: −45 dB is
            // ±0.05 dB on a flat response
            let dtft = |v: &[f64], sr: f64, f: f64| {
                let (mut re, mut im) = (0.0, 0.0);
                for (k, a) in v.iter().enumerate() {
                    let w = -2.0 * PI * f * k as f64 / sr;
                    re += a * w.cos();
                    im += a * w.sin();
                }
                (re, im)
            };
            let level = (x.iter().map(|v| v * v).sum::<f64>()).sqrt();
            let mut worst = f64::MIN;
            let mut f = 50.0;
            while f <= top {
                let mut a = dtft(h, to, f);
                // add the precursor at negative times
                for (k, v) in pre.iter().enumerate() {
                    let w = 2.0 * PI * f * (lead - k) as f64 / to;
                    a.0 += v * w.cos();
                    a.1 += v * w.sin();
                }
                let b = dtft(&x, from, f);
                let e = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
                worst = worst.max(20.0 * (e / level).log10());
                f *= 1.03;
            }
            eprintln!("ir: resample {from} → {to}: worst error {worst:.1} dB re level to {top} Hz");
            assert!(worst < -45.0, "{from} → {to}: {worst:.1} dB");
        }
    }

    #[test]
    fn resampling_stop_band() {
        // 96 → 48: a tone at 30 kHz (above the new Nyquist) is rejected ≥ 100 dB
        let n = 9600;
        let x: Vec<f64> = (0..n).map(|k| (2.0 * PI * 30000.0 * k as f64 / 96000.0).sin()).collect();
        let y = resample(&x, 96000.0, 48000.0);
        let mid = &y[1000..y.len() - 1000];
        let pk = mid.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        eprintln!("ir: 96 → 48 alias of 30 kHz: {:.1} dB", 20.0 * pk.log10());
        assert!(20.0 * pk.log10() < -100.0);
        // 48 → 192: a 20 kHz tone leaves no image above 24 kHz
        let x: Vec<f64> = (0..4800).map(|k| (2.0 * PI * 20000.0 * k as f64 / 48000.0).sin()).collect();
        let y = resample(&x, 48000.0, 192000.0);
        let mid = &y[4000..y.len() - 4000];
        // Blackman-Harris window for the measurement
        let nm = mid.len() as f64;
        let mid: Vec<f64> = mid
            .iter()
            .enumerate()
            .map(|(k, v)| {
                let a = 2.0 * PI * k as f64 / nm;
                v * (0.35875 - 0.48829 * a.cos() + 0.14128 * (2.0 * a).cos() - 0.01168 * (3.0 * a).cos())
            })
            .collect();
        let mid = &mid[..];
        let img = mag_db(mid, 192000.0, 28000.0) - mag_db(mid, 192000.0, 20000.0);
        eprintln!("ir: 48 → 192 image at 28 kHz: {img:.1} dB re the tone");
        assert!(img < -100.0);
    }

    #[test]
    fn onset_keeps_the_first_peak() {
        let sr = 48000.0;
        let h = synth(sr, 0.004);
        let mut chs = vec![h.clone()];
        let cut = onset_trim(&mut chs, sr);
        let first = h.iter().position(|v| v.abs() >= 0.1 * h.iter().fold(0.0f64, |m, v| m.max(v.abs()))).unwrap();
        assert_eq!(cut, first - 24);
        // bit-equal from 0.25 ms before the −20 dB point
        assert_eq!(&chs[0][12..], &h[cut + 12..]);
        // a file that starts at its onset loses nothing
        let h0 = synth(sr, 0.0001);
        let mut c0 = vec![h0.clone()];
        assert_eq!(onset_trim(&mut c0, sr), 0);
        assert_eq!(c0[0], h0);
    }

    #[test]
    fn polarity_follows_the_first_lobe() {
        let sr = 48000.0;
        let h = synth(sr, 0.0);
        let neg: Vec<f64> = h.iter().map(|v| -v).collect();
        assert_eq!(polarity(&h), polarity(&h));
        assert_eq!(polarity(&neg), -polarity(&h));
        // a first spike, then a larger swing the other way: the spike decides
        let mut s = vec![0.0; 100];
        s[3] = 0.5;
        s[4] = -1.0;
        assert_eq!(polarity(&s), 1.0);
    }

    #[test]
    fn correlation_recovers_lag_and_sign() {
        let sr = 48000.0;
        let a = synth(sr, 0.001);
        let b: Vec<f64> = synth(sr, 0.001 + 0.00137).iter().map(|v| -v).collect();
        let al = align_correlation(&a, &b, sr);
        assert!(al.flip);
        assert!((al.lag / sr - 0.00137).abs() < 0.00001, "lag {} ms", al.lag / sr * 1000.0);
        // the pair aligned: 50 / 50 blend within 0.5 dB of A to 5 kHz
        let s = Settings {
            align: Align::Correlation,
            normalize: Normalize::Off,
            min_phase: false,
            reverse: false,
            length_s: 2.0,
            fade: 0.1,
            max_s: 2.0,
            band: (80.0, 6000.0),
            ref_db: 14.0,
            map: ChannelMap::Cab,
            out_channels: 1,
        };
        let raw = Settings { align: Align::Raw, ..s };
        let (pa, pb, _) = prepare_pair(&Source::mono(sr, a.clone()), &Source::mono(sr, b.clone()), sr, &raw);
        let _ = (pa, pb);
        let (pa, pb, _) = prepare_pair(&Source::mono(sr, a.clone()), &Source::mono(sr, b), sr, &s);
        let n = pa[0].len().max(pb[0].len());
        let blend: Vec<f64> = (0..n).map(|k| 0.5 * (*pa[0].get(k).unwrap_or(&0.0) as f64 + *pb[0].get(k).unwrap_or(&0.0) as f64)).collect();
        let a_only: Vec<f64> = pa[0].iter().map(|v| *v as f64).collect();
        let mut worst = 0.0f64;
        let mut f = 100.0;
        while f < 5000.0 {
            worst = worst.max((mag_db(&blend, sr, f) - mag_db(&a_only, sr, f)).abs());
            f *= 1.1;
        }
        eprintln!("ir: aligned 50/50 blend vs A: {worst:.3} dB to 5 kHz");
        assert!(worst < 0.5);
    }

    #[test]
    fn min_phase_keeps_magnitude_and_front_loads_energy() {
        let sr = 48000.0;
        let h = synth(sr, 0.002);
        let m = min_phase(&h);
        // 1/12-octave smoothed magnitude
        let smooth = |x: &[f64], f: f64| {
            let p: f64 = (0..16).map(|i| 10f64.powf(mag_db(x, sr, f * 2f64.powf((i as f64 - 7.5) / 16.0 / 12.0)) / 10.0)).sum();
            10.0 * (p / 16.0).log10()
        };
        let mut f = 100.0;
        let mut worst = 0.0f64;
        while f < 20000.0 {
            worst = worst.max((smooth(&m, f) - smooth(&h, f)).abs());
            f *= 1.06;
        }
        eprintln!("ir: min phase, 1/12-octave magnitude within {worst:.4} dB");
        assert!(worst < 0.1);
        let total: f64 = h.iter().map(|v| v * v).sum();
        let (mut ea, mut eb, mut deficit) = (0.0, 0.0, 0.0f64);
        for (x, y) in h.iter().zip(&m) {
            ea += x * x;
            eb += y * y;
            deficit = deficit.max((ea - eb) / total);
        }
        eprintln!("ir: min phase, cumulative energy deficit {deficit:.2e} of the total");
        assert!(deficit < 1e-6);
    }

    #[test]
    fn trim_fade_and_tail() {
        let mut chs = vec![vec![1.0; 1000], vec![0.5; 800]];
        trim_fade(&mut chs, 400, 0.25);
        assert_eq!(chs[0].len(), 400);
        assert_eq!(chs[0][299], 1.0);
        assert!(chs[0][399].abs() < 1e-12);
        let mut t = vec![vec![1.0, 0.5, 1e-7, 0.0, 0.0]];
        drop_tail(&mut t);
        assert_eq!(t[0].len(), 2);
    }

    #[test]
    fn loudness_normalises() {
        let sr = 48000.0;
        let h = synth(sr, 0.0);
        let g = loudness_gain(std::slice::from_ref(&h), sr, (80.0, 6000.0));
        let hn: Vec<f64> = h.iter().map(|v| v * g).collect();
        let p = weighted_power(&[hn], sr, (80.0, 6000.0));
        assert!((10.0 * p.log10()).abs() < 1e-9);
        // δ is 0 dB
        assert!((weighted_power(&[vec![1.0]], sr, (60.0, 12000.0)) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn frac_delay_is_a_delay() {
        let sr = 48000.0;
        let h = synth(sr, 0.0);
        let d = frac_delay(&h, 10.5);
        // phase slope: group delay 10.5 samples at 1 kHz
        let ph = |x: &[f64], f: f64| {
            let (mut re, mut im) = (0.0, 0.0);
            for (k, v) in x.iter().enumerate() {
                let w = -2.0 * PI * f * k as f64 / sr;
                re += v * w.cos();
                im += v * w.sin();
            }
            im.atan2(re)
        };
        let dp = ph(&h, 1000.0) - ph(&d, 1000.0);
        let expect = 2.0 * PI * 1000.0 * 10.5 / sr;
        let diff = (dp - expect).rem_euclid(2.0 * PI);
        assert!(diff.min(2.0 * PI - diff) < 1e-3);
        assert!(rms_db_diff(&frac_delay(&h, 3.0)[3..], &h) < -300.0);
    }
}
