// SPDX-License-Identifier: GPL-3.0-or-later
//! A virtual converter: an ADC → DAC pair running at its own clock and word format, simulated on its
//! discrete sample sequence (gfx-crusher; gfx-radio runs its cell codec between the two halves).
//!
//! - [`Clock`]: a fractional f64 phase accumulator at any rate up to the host's, with seeded jitter.
//!   It yields the instants falling in each host sample in [`crate::blep`]'s convention (d = n − t).
//! - [`Adc`]: clip at full scale, TPDF dither in the coded domain, quantiser (format and shape). Its
//!   output is a [`Word`]: the decoded level plus the integer code the DAC's ladder needs.
//! - [`Dac`]: the R-2R ladder's bit-weight errors and the power-law transfer curve, both memoryless
//!   maps of the held levels.
//! - [`BoxSampler`]: integrate-and-dump sampling (the input averaged over each virtual period).
//! - [`tail_gain`]: the level window that fades the quantisation out below 1 – 2 steps.
//!
//! Sampling at an instant and rendering the held staircase belong to [`crate::blep`]. Everything here
//! acts on the virtual samples, a discrete sequence: its aliasing is the virtual converter's own.

use crate::blep::{Kernel, Reader};
use crate::lfo::Rng;

/// Word format (append only: ADPCM and Delta come after these).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Uniform PCM, step 2^(1 − bits); fractional bits allowed.
    Linear,
    /// Continuous µ = 255 companding around a uniform quantiser of `bits`.
    MuLaw,
    /// 3-bit exponent (8 octaves of gain ranging) and a `bits` mantissa.
    Float,
}

impl Format {
    pub fn from_index(i: usize) -> Format {
        match i {
            1 => Format::MuLaw,
            2 => Format::Float,
            _ => Format::Linear,
        }
    }
}

/// Where zero sits in the quantiser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// Mid-tread (round): zero is a level, silence stays silent.
    Tread,
    /// Mid-riser: zero between two levels (±½ step buzz on silence; 1 bit = a square).
    Riser,
    /// Truncation toward zero: a 2-step dead zone.
    Truncate,
}

impl Shape {
    pub fn from_index(i: usize) -> Shape {
        match i {
            1 => Shape::Riser,
            2 => Shape::Truncate,
            _ => Shape::Tread,
        }
    }
}

pub const MU: f64 = 255.0;
/// Float: exponents 0 … −7.
pub const FLOAT_OCTAVES: i32 = 8;
/// The DAC curve's level-neutral point (−18 dBFS).
pub const CURVE_REF: f64 = 0.125_892_541_179_416_72;

#[inline]
fn mu_compress(x: f64) -> f64 {
    x.signum() * (MU * x.abs()).ln_1p() / MU.ln_1p()
}

#[inline]
fn mu_expand(c: f64) -> f64 {
    c.signum() * ((c.abs() * MU.ln_1p()).exp() - 1.0) / MU
}

/// A uniform quantiser of step `step` on `u` (in the coded domain, ±1 full scale): the integer code
/// and its level. Codes are clamped to the word's range.
#[inline]
fn quantise(u: f64, step: f64, shape: Shape) -> (i32, f64) {
    let v = u / step;
    let top = (1.0 / step + 1e-9).floor().max(1.0);
    match shape {
        Shape::Tread => {
            let k = v.round().clamp(-top, top);
            (k as i32, k * step)
        }
        Shape::Truncate => {
            let k = v.trunc().clamp(-top, top);
            (k as i32, k * step)
        }
        Shape::Riser => {
            let n = (1.0 / step).round().max(1.0);
            let k = v.floor().clamp(-n, n - 1.0);
            (k as i32, (k + 0.5) * step)
        }
    }
}

/// One converted sample.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Word {
    /// The decoded level (full scale ±1).
    pub level: f64,
    /// The integer code, for the DAC ladder (0 = the zero code).
    pub code: i32,
    /// The level of one code step, for the ladder's errors (0 = no ladder: µ-law).
    pub lsb: f64,
    /// The word width the ladder sees.
    pub width: u32,
}

/// The ADC half: clip, dither, quantise.
#[derive(Clone, Debug)]
pub struct Adc {
    pub format: Format,
    pub shape: Shape,
    bits: f64,
    /// 2^(1 − bits), cached
    step: f64,
    /// TPDF dither, 1 = ±1 step.
    dither: f64,
    rng: Rng,
}

impl Default for Adc {
    fn default() -> Self {
        Self { format: Format::Linear, shape: Shape::Tread, bits: 8.0, step: 2f64.powi(-7), dither: 0.0, rng: Rng::new(0xC0DE_D17E) }
    }
}

impl Adc {
    pub fn reset(&mut self) {
        self.rng.reset();
    }

    /// Word size in bits (1 – 24, fractional allowed) and dither (0 – 1).
    pub fn set(&mut self, format: Format, shape: Shape, bits: f64, dither: f64) {
        self.format = format;
        self.shape = shape;
        let bits = bits.clamp(1.0, 24.0);
        if bits != self.bits {
            self.bits = bits;
            self.step = 2f64.powf(1.0 - bits);
        }
        self.dither = dither.clamp(0.0, 1.0);
    }

    pub fn bits(&self) -> f64 {
        self.bits
    }

    /// The step at zero, as a level: what the tails window is referenced to (0 for Float, whose grain
    /// scales with the level).
    pub fn zero_step(&self) -> f64 {
        let step = self.step;
        match self.format {
            Format::Linear => step,
            Format::MuLaw => mu_expand(step),
            Format::Float => 0.0,
        }
    }

    #[inline]
    fn tpdf(&mut self) -> f64 {
        if self.dither <= 0.0 {
            return 0.0;
        }
        (self.rng.next_f64() + self.rng.next_f64() - 1.0) * self.dither
    }

    /// Converts one virtual sample (clipped at full scale first).
    #[inline]
    pub fn convert(&mut self, x: f64) -> Word {
        let x = x.clamp(-1.0, 1.0);
        let step = self.step;
        let width = self.bits.ceil() as u32;
        match self.format {
            Format::Linear => {
                let u = x + self.tpdf() * step;
                let (code, level) = quantise(u, step, self.shape);
                Word { level, code, lsb: step, width }
            }
            Format::MuLaw => {
                let u = mu_compress(x) + self.tpdf() * step;
                let (code, c) = quantise(u, step, self.shape);
                Word { level: mu_expand(c), code, lsb: 0.0, width }
            }
            Format::Float => {
                // the exponent from the input's magnitude: |x| ∈ (2^(e−1), 2^e], e ∈ 0 … −7
                let a = x.abs();
                let e = if a > 0.0 { (a.log2().ceil() as i32).clamp(1 - FLOAT_OCTAVES, 0) } else { 1 - FLOAT_OCTAVES };
                let scale = 2f64.powi(e);
                let u = x / scale + self.tpdf() * step;
                let (code, m) = quantise(u, step, self.shape);
                Word { level: m * scale, code, lsb: step * scale, width }
            }
        }
    }
}

/// Bits a ladder models (codes are clamped to this width).
const LADDER_BITS: usize = 24;

/// The DAC half: ladder errors and the transfer curve.
#[derive(Clone, Debug)]
pub struct Dac {
    /// fixed seeded bit-weight errors, ±1
    weights: [f64; LADDER_BITS],
    ladder: f64,
    e_pos: f64,
    e_neg: f64,
}

impl Default for Dac {
    fn default() -> Self {
        let mut rng = Rng::new(0x1ADD_E2DA);
        let mut weights = [0.0; LADDER_BITS];
        for w in weights.iter_mut() {
            *w = rng.next_f64() * 2.0 - 1.0;
        }
        // the MSB's error is the largest of the set (the mid-scale step), its sign kept
        weights[0] = weights[0].signum();
        Self { weights, ladder: 0.0, e_pos: 1.0, e_neg: 1.0 }
    }
}

impl Dac {
    /// `ladder` 0 – 1 (1: the MSB off by one step, each lower bit half its upper neighbour's error);
    /// `nonlin` 0 – 1 (exponent 10^(−0.15 · nonlin)); `asym` 0 – 1 (share of the curve kept off the
    /// negative half).
    pub fn set(&mut self, ladder: f64, nonlin: f64, asym: f64) {
        self.ladder = ladder.clamp(0.0, 1.0);
        let e = 10f64.powf(-0.15 * nonlin.clamp(0.0, 1.0));
        self.e_pos = e;
        self.e_neg = 10f64.powf(-0.15 * nonlin.clamp(0.0, 1.0) * (1.0 - asym.clamp(0.0, 1.0)));
    }

    /// The ladder error of `code` in steps, relative to the zero code (offset binary, `width` bits).
    #[inline]
    fn ladder_error(&self, code: i32, width: u32) -> f64 {
        let w = (width as usize).clamp(1, LADDER_BITS);
        let half = 1i64 << (w - 1);
        let c = (code as i64 + half).clamp(0, 2 * half - 1) as u64;
        let z = half as u64;
        let mut e = 0.0;
        // bit w−1 (the MSB) weighs ±1 step at full error, bit w−1−j weighs 2^−j
        let mut scale = 1.0;
        for j in 0..w {
            let bit = w - 1 - j;
            let d = ((c >> bit) & 1) as f64 - ((z >> bit) & 1) as f64;
            e += d * self.weights[j] * scale;
            scale *= 0.5;
        }
        e
    }

    /// The word's level through the ladder.
    #[inline]
    pub fn decode(&self, w: Word) -> f64 {
        if self.ladder > 0.0 && w.lsb > 0.0 {
            w.level + self.ladder * w.lsb * self.ladder_error(w.code, w.width)
        } else {
            w.level
        }
    }

    /// Whether the transfer curve is the identity.
    pub fn is_linear(&self) -> bool {
        self.e_pos == 1.0 && self.e_neg == 1.0
    }

    /// The power-law transfer curve y = sign(v) · r · (|v| / r)^e, r = −18 dBFS.
    #[inline]
    pub fn curve(&self, v: f64) -> f64 {
        if v > 0.0 {
            if self.e_pos == 1.0 {
                v
            } else {
                CURVE_REF * (v / CURVE_REF).powf(self.e_pos)
            }
        } else if v < 0.0 {
            if self.e_neg == 1.0 {
                v
            } else {
                -CURVE_REF * (-v / CURVE_REF).powf(self.e_neg)
            }
        } else {
            0.0
        }
    }
}

/// The tails window: 1 (unquantised) below one step at zero, 0 (fully converted) above two, a
/// smoothstep in dB between. `step` 0 (Float) → 0.
#[inline]
pub fn tail_gain(level: f64, step: f64) -> f64 {
    if step <= 0.0 {
        return 0.0;
    }
    if level <= step {
        return 1.0;
    }
    if level >= 2.0 * step {
        return 0.0;
    }
    let t = (level / step).log2();
    1.0 - t * t * (3.0 - 2.0 * t)
}

/// The virtual clock: a phase accumulator in f64 with seeded jitter.
#[derive(Clone, Debug)]
pub struct Clock {
    phase: f64,
    /// a scheduled instant, relative to the current host sample (t − n)
    pending: Option<f64>,
    /// the low-passed jitter walk, ±1
    walk: f64,
    rng: Rng,
}

/// The instants inside one host sample, earliest first, as d = n − t ∈ (0, 1].
#[derive(Clone, Copy, Debug, Default)]
pub struct Instants {
    pub n: usize,
    pub d: [f64; 2],
}

impl Default for Clock {
    fn default() -> Self {
        Self { phase: 0.0, pending: None, walk: 0.0, rng: Rng::new(0x1177_E2C1) }
    }
}

impl Clock {
    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.pending = None;
        self.walk = 0.0;
        self.rng.reset();
    }

    /// Advances one host sample. `inc` = R / fs (0 < inc ≤ 1); `jitter` 0 – 1: each instant is late
    /// by T · jitter · (1 + w) / 4, w a seeded walk in ±1 (so ±25 % of a period around a fixed
    /// lateness, and consecutive instants never cross).
    #[inline]
    pub fn tick(&mut self, inc: f64, jitter: f64) -> Instants {
        let mut out = Instants::default();
        if let Some(r) = self.pending {
            let r = r - 1.0;
            if r < 0.0 {
                out.d[out.n] = (-r).min(1.0);
                out.n += 1;
                self.pending = None;
            } else {
                self.pending = Some(r);
            }
        }
        let inc = inc.clamp(1e-9, 1.0);
        self.phase += inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            // the nominal instant: d_nom ∈ [0, 1) before this sample
            let d_nom = self.phase / inc;
            let mut late = 0.0;
            if jitter > 0.0 {
                late = jitter * 0.25 * (1.0 + self.walk) / inc;
                // the walk wanders over a few periods
                let u = self.rng.next_f64() * 2.0 - 1.0;
                self.walk += 0.3 * (u - self.walk);
            }
            let r = late - d_nom;
            if self.pending.take().is_some() && out.n < 2 {
                // cannot happen with a steady clock (lateness ≤ T / 2): fire it now, on the sample
                out.d[out.n] = 1e-9;
                out.n += 1;
            }
            if r < 0.0 {
                if out.n < 2 {
                    out.d[out.n] = (-r).min(1.0);
                    out.n += 1;
                }
            } else {
                self.pending = Some(r);
            }
        }
        out
    }
}

/// Integrate-and-dump sampling: the mean of the band-limited input over each virtual period. The
/// running integral at an instant is the plain sum of the samples before it plus the kernel's step
/// table on the last 32 ([`Reader::integral`]): exact for the band-limited signal, so the period's
/// edges fold nothing a real integrating converter would not (a trapezoid over the host samples left
/// its images at −20 dB for high partials). Delayed by the kernel's group delay, like a point sample.
#[derive(Clone, Debug, Default)]
pub struct BoxSampler {
    /// the samples pushed since the last instant, summed
    run: f64,
    since: f64,
    d_prev: f64,
    j_prev: f64,
}

impl BoxSampler {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The mean since the previous instant, up to t = n − d (`reader` holds the history to n − 1).
    #[inline]
    pub fn sample(&mut self, reader: &Reader, k: &Kernel, d: f64) -> f32 {
        let j = reader.integral(k, d) as f64;
        let span = self.since + self.d_prev - d;
        let y = if span > 1e-6 { (self.run + j - self.j_prev) / span } else { reader.read(k, d) as f64 };
        self.run = 0.0;
        self.since = 0.0;
        self.d_prev = d;
        self.j_prev = j;
        y as f32
    }

    /// Host sample `x[n]`, pushed after its instants (as to the [`Reader`]).
    #[inline]
    pub fn push(&mut self, x: f32) {
        self.run += x as f64;
        self.since += 1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blep::TAPS;

    fn levels(adc: &mut Adc) -> std::collections::BTreeSet<i64> {
        let mut s = std::collections::BTreeSet::new();
        for i in 0..=200_000 {
            let x = -1.2 + 2.4 * i as f64 / 200_000.0;
            s.insert((adc.convert(x).level * 1e9).round() as i64);
        }
        s
    }

    #[test]
    fn quantiser_shapes() {
        for bits in [1.0, 3.0, 8.0] {
            let n = 2f64.powf(bits) as usize;
            let mut a = Adc::default();
            a.set(Format::Linear, Shape::Tread, bits, 0.0);
            assert_eq!(a.convert(0.0).level, 0.0);
            assert_eq!(levels(&mut a).len(), n + 1, "tread {bits}");
            a.set(Format::Linear, Shape::Riser, bits, 0.0);
            let step = 2f64.powf(1.0 - bits);
            assert_eq!(a.convert(0.0).level, 0.5 * step);
            assert_eq!(levels(&mut a).len(), n, "riser {bits}");
            a.set(Format::Linear, Shape::Truncate, bits, 0.0);
            // dead zone: |x| < one step → 0 (2 steps wide)
            assert_eq!(a.convert(0.99 * step).level, 0.0);
            assert_eq!(a.convert(-0.99 * step).level, 0.0);
            assert_eq!(a.convert(1.01 * step).level, step);
            assert_eq!(levels(&mut a).len(), n + 1, "truncate {bits}");
        }
        // 1-bit riser: a square at ±½ (−6 dBFS) whatever the input
        let mut a = Adc::default();
        a.set(Format::Linear, Shape::Riser, 1.0, 0.0);
        assert_eq!(a.convert(1e-6).level, 0.5);
        assert_eq!(a.convert(-0.9).level, -0.5);
    }

    /// G.711 µ-law (the segmented tables), encode then decode, on a 14-bit-scale input (±8159).
    fn g711(x: f64) -> f64 {
        let s = (x * 8159.0).round().clamp(-8159.0, 8159.0) as i32;
        let sign = s < 0;
        let mag = (s.abs() + 33).min(8191);
        let mut seg = 0;
        while seg < 7 && mag >= (64 << seg) {
            seg += 1;
        }
        let mant = (mag >> (seg + 1)) & 0x0F;
        // decode: the middle of the interval
        let dec = (((mant << 1) + 33) << seg) - 33;
        let v = dec as f64 / 8159.0;
        if sign {
            -v
        } else {
            v
        }
    }

    #[test]
    fn mu_law_follows_the_curve_and_g711() {
        let mut a = Adc::default();
        a.set(Format::MuLaw, Shape::Tread, 8.0, 0.0);
        let mut worst: f64 = 0.0;
        for i in 1..4000 {
            let x = (i as f64 / 4000.0).powi(3);
            let ours = a.convert(x).level;
            let theirs = g711(x);
            // one step of G.711 at that level
            let c = mu_compress(x);
            let local = mu_expand(c + 1.0 / 128.0) - mu_expand(c);
            worst = worst.max((ours - theirs).abs() / local);
            // on the continuous curve within half a code step
            let cq = mu_compress(ours);
            assert!((cq - c).abs() <= 0.5 / 128.0 + 1e-9);
        }
        // the continuous curve departs from G.711's chords by up to 1.4 codes in the low segments, and
        // each rounds to its own grid: 1.39 local steps measured
        assert!(worst <= 1.5, "µ-law vs G.711: {worst} steps");
        // the companded step at zero for 8 bits: ≈ −75 dBFS
        let s = a.zero_step();
        assert!((20.0 * s.log10() + 75.2).abs() < 0.5, "{}", 20.0 * s.log10());
    }

    #[test]
    fn float_snr_is_constant() {
        let mut snrs = Vec::new();
        // every 0.25 dB: each sample's grain follows its own octave, so the SNR of a sine ripples with
        // where its peaks sit in their octave (3.5 dB peak to peak at 8 bits); a linear word loses 36 dB
        for k in 0..=144 {
            let db = -6.0 - 0.25 * k as f64;
            let amp = 10f64.powf(db / 20.0);
            let mut a = Adc::default();
            a.set(Format::Float, Shape::Tread, 8.0, 0.0);
            let (mut es, mut en) = (0.0, 0.0);
            for i in 0..48000 {
                let x = amp * (2.0 * std::f64::consts::PI * 997.0 * i as f64 / 48000.0).sin();
                let y = a.convert(x).level;
                es += x * x;
                en += (y - x) * (y - x);
            }
            snrs.push(10.0 * (es / en).log10());
        }
        let lo = snrs.iter().cloned().fold(f64::MAX, f64::min);
        let hi = snrs.iter().cloned().fold(f64::MIN, f64::max);
        assert!(hi - lo <= 4.0, "{lo} – {hi} dB");
    }

    #[test]
    fn ladder_errs_at_mid_scale_only_by_its_weights() {
        let mut d = Dac::default();
        d.set(1.0, 0.0, 0.0);
        let mut a = Adc::default();
        a.set(Format::Linear, Shape::Tread, 8.0, 0.0);
        let step = 2f64.powf(-7.0);
        // zero stays zero; the step across mid-scale (code −1 → 0) is wrong-sized
        assert_eq!(d.decode(a.convert(0.0)), 0.0);
        let below = d.decode(a.convert(-step));
        let err = (below + step).abs() / step;
        assert!(err > 0.05 && err <= 2.0, "{err}");
        // at most ±2 steps anywhere (MSB 1 + ½ + ¼ …)
        for i in -128..128 {
            let w = a.convert(i as f64 * step);
            assert!((d.decode(w) - w.level).abs() <= 2.0 * step + 1e-12);
        }
    }

    #[test]
    fn curve_is_level_neutral_at_minus_18() {
        let mut d = Dac::default();
        d.set(0.0, 1.0, 0.0);
        assert!((d.curve(CURVE_REF) - CURVE_REF).abs() < 1e-12);
        assert!((d.curve(-CURVE_REF) + CURVE_REF).abs() < 1e-12);
        // exponent 0.708 at 100 %
        let e = (d.curve(0.5) / CURVE_REF).ln() / (0.5 / CURVE_REF).ln();
        assert!((e - 0.7079).abs() < 1e-3, "{e}");
        // asym 100 %: the negative half untouched
        d.set(0.0, 1.0, 1.0);
        assert_eq!(d.curve(-0.3), -0.3);
        assert!(d.curve(0.3) < 0.3);
    }

    #[test]
    fn tail_window() {
        let s = 2f64.powi(-7);
        assert_eq!(tail_gain(0.9 * s, s), 1.0);
        assert_eq!(tail_gain(2.1 * s, s), 0.0);
        let mid = tail_gain(s * 2f64.sqrt(), s);
        assert!((mid - 0.5).abs() < 1e-12);
        assert_eq!(tail_gain(1.0, 0.0), 0.0);
    }

    #[test]
    fn clock_is_fractional_and_jitter_keeps_order() {
        // 1,000 instants at R = fs / 7.3: their spacing is exactly 7.3 samples
        let mut c = Clock::default();
        let inc = 1.0 / 7.3;
        let mut times = Vec::new();
        for n in 0..7301 {
            let ins = c.tick(inc, 0.0);
            for k in 0..ins.n {
                times.push(n as f64 - ins.d[k]);
            }
        }
        assert_eq!(times.len(), 1000);
        for w in times.windows(2) {
            assert!((w[1] - w[0] - 7.3).abs() < 1e-6);
        }
        // jitter: ordered, within a period's half of the nominal grid, and deterministic
        for inc in [0.9, 0.5, 0.01] {
            let run = || {
                let mut c = Clock::default();
                let mut t = Vec::new();
                for n in 0..20000 {
                    let ins = c.tick(inc, 1.0);
                    for k in 0..ins.n {
                        assert!(ins.d[k] > 0.0 && ins.d[k] <= 1.0);
                        t.push(n as f64 - ins.d[k]);
                    }
                }
                t
            };
            let t = run();
            assert_eq!(t, run());
            for w in t.windows(2) {
                assert!(w[1] > w[0], "{inc}: {} then {}", w[0], w[1]);
            }
            let period = 1.0 / inc;
            for (k, v) in t.iter().enumerate() {
                let nominal = (k + 1) as f64 * period - 1.0;
                assert!(v - nominal >= -1e-6 && v - nominal <= 0.5 * period + 1e-6, "{inc}: {k}");
            }
        }
    }

    #[test]
    fn box_sampler_averages_each_period() {
        // a ramp x[n] = n / 100: band-limited, it is (t − D) / 100 (D the kernel's delay), and its mean
        // over [t_{k−1}, t_k] the period's midpoint, less D
        let k = Kernel::get();
        let mut b = BoxSampler::default();
        let mut r = Reader::default();
        let mut c = Clock::default();
        let inc = 1.0 / 3.7;
        let mut last_t = 0.0;
        for n in 0..400usize {
            let ins = c.tick(inc, 0.0);
            for j in 0..ins.n {
                let t = n as f64 - ins.d[j];
                let y = b.sample(&r, k, ins.d[j]) as f64;
                if n > 2 * TAPS {
                    let want = (0.5 * (t + last_t) - k.delay) / 100.0;
                    assert!((y - want).abs() < 1e-4, "{n}: {y} vs {want}");
                }
                last_t = t;
            }
            let x = n as f32 / 100.0;
            r.push(x);
            b.push(x);
        }
    }
}
