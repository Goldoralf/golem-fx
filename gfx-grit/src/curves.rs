// SPDX-License-Identifier: GPL-3.0-or-later
//! The memoryless curves of the dirt modules and their antiderivatives (for ADAA1), the Amount laws
//! (README §2.4) and the static level compensation (§2.6). Plain `std` only: build.rs includes this
//! file to generate the compensation tables at build time.
//!
//! Every curve is written from the README's formulas; nothing comes from Prisma's source.

#![allow(dead_code)]

/// Peak of the compensation's reference sine (dBFS): −18 dBFS, a first value (README §2.6).
pub const REF_PEAK_DB: f64 = -18.0;
/// Points of each compensation table, over Amount 0 – 100 %.
pub const COMP_POINTS: usize = 65;
/// Limits of the make-up (dB).
pub const COMP_MIN_DB: f64 = -30.0;
pub const COMP_MAX_DB: f64 = 12.0;
/// Heavy / Asym knee: linear below −40 dBFS.
pub const HEAVY_KNEE: f64 = 0.01;
/// Tube bias, and its tanh (checked in the tests).
pub const TUBE_BIAS: f64 = 0.3;
pub const TANH_TUBE_BIAS: f64 = 0.291_312_612_451_591;
/// Fuzz core: stage gains and biases (gfx-drive's two-stage shape, hard-clip stages).
pub const FUZZ_G1: f64 = 2.0;
pub const FUZZ_B1: f64 = 0.2;
pub const FUZZ_G2: f64 = 3.0;
pub const FUZZ_B2: f64 = 0.1;

#[inline]
pub fn ln_cosh(x: f64) -> f64 {
    // |x| + ln(1 + e^(−2|x|)) − ln 2; ln(1 + e) is cheaper than ln_1p and off by < 2e-16 absolute,
    // and the correction is below an ulp from |x| = 18.5
    let a = x.abs();
    if a > 18.5 {
        a - std::f64::consts::LN_2
    } else {
        a + (1.0 + (-2.0 * a).exp()).ln() - std::f64::consts::LN_2
    }
}

#[inline]
pub fn clip_f(x: f64) -> f64 {
    x.clamp(-1.0, 1.0)
}

#[inline]
pub fn clip_af(x: f64) -> f64 {
    if x.abs() <= 1.0 {
        0.5 * x * x
    } else {
        x.abs() - 0.5
    }
}

// ---- Amount laws (a = effective Amount, 0 – 100) -------------------------------------------------

/// Saturators: drive 0.48 a dB, as a linear gain.
#[inline]
pub fn drive_gain(a: f64) -> f64 {
    10f64.powf(0.024 * a)
}

/// Heavy / Asym exponent (Prisma's law, kept).
#[inline]
pub fn heavy_exp(a: f64) -> f64 {
    1.0 - 0.0099 * a
}

/// Zero cross dead zone T (dBFS): −66 + 0.60 a.
#[inline]
pub fn zc_threshold_db(a: f64) -> f64 {
    -66.0 + 0.6 * a
}

/// Bit crush depth (bits, fractional): 16 − 0.15 a.
#[inline]
pub fn crush_bits(a: f64) -> f64 {
    16.0 - 0.15 * a
}

/// Sample divide hold rate (Hz): 40 · 1000^(1 − a / 100), Prisma's law kept.
#[inline]
pub fn hold_rate(a: f64) -> f64 {
    40.0 * 1000f64.powf(1.0 - a / 100.0)
}

/// Octave-linear frequency 20 · 1000^(a / 100) Hz (Highpass, Peak, Ring mod, crossovers).
#[inline]
pub fn freq_up(a: f64) -> f64 {
    20.0 * 1000f64.powf(a / 100.0)
}

/// Lowpass cut-off 20 · 1000^(1 − a / 100) Hz.
#[inline]
pub fn freq_down(a: f64) -> f64 {
    20.0 * 1000f64.powf(1.0 - a / 100.0)
}

/// Gain module (dB): −24 + 0.48 a.
#[inline]
pub fn gain_db(a: f64) -> f64 {
    -24.0 + 0.48 * a
}

// ---- curves ----------------------------------------------------------------------------------------

/// A memoryless curve with its parameters, f(x) and F(x) = ∫₀ˣ f.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    /// y = x.
    Lin,
    /// tanh(g x).
    Tanh(f64),
    /// clip(g x, ±1).
    Clip(f64),
    /// sgn x · |x|^e, linear below the knee (value-continuous).
    Heavy(f64),
    /// x ≥ 0: x^e (knee as Heavy); −1 ≤ x < 0: (x + 1)^e − 1; x < −1: x.
    Asym(f64),
    /// (1 − r) x + r s |x| (s = +1, or −1 for Stereo rectify's right channel).
    Rectify(f64, f64),
    /// Triangle fold of g x, period 4.
    Fold(f64),
    /// sin(π/2 · g x).
    SineFold(f64),
    /// sgn x · max(|x| − T, 0) with a quadratic knee of ±w around T.
    ZeroCross(f64),
    /// Mid-tread quantiser of step q: q · round(x / q).
    Crush(f64),
    /// tanh(g x + b) − tanh(b).
    Tube(f64),
    /// clip(g x + b) − clip(b) (one Fuzz stage).
    BiasClip(f64, f64),
}

/// Half-width of Zero cross's knee, relative to T: from T / √2 to (2 − 1/√2) T (about 6 dB wide).
const ZC_KNEE: f64 = 1.0 - std::f64::consts::FRAC_1_SQRT_2;

#[inline]
fn heavy_pos(a: f64, e: f64) -> f64 {
    if a <= HEAVY_KNEE {
        a * HEAVY_KNEE.powf(e - 1.0)
    } else {
        a.powf(e)
    }
}

#[inline]
fn heavy_pos_af(a: f64, e: f64) -> f64 {
    let k = HEAVY_KNEE;
    if a <= k {
        0.5 * a * a * k.powf(e - 1.0)
    } else {
        0.5 * k * k.powf(e) + (a.powf(e + 1.0) - k.powf(e + 1.0)) / (e + 1.0)
    }
}

#[inline]
fn tri(u: f64) -> f64 {
    let p = (u + 1.0).rem_euclid(4.0) - 1.0;
    if p <= 1.0 {
        p
    } else {
        2.0 - p
    }
}

#[inline]
fn tri_af(u: f64) -> f64 {
    let p = (u + 1.0).rem_euclid(4.0) - 1.0;
    if p <= 1.0 {
        0.5 * p * p
    } else {
        1.0 - 0.5 * (p - 2.0) * (p - 2.0)
    }
}

impl Curve {
    #[inline]
    pub fn f(&self, x: f64) -> f64 {
        match *self {
            Curve::Lin => x,
            Curve::Tanh(g) => (g * x).tanh(),
            Curve::Clip(g) => clip_f(g * x),
            Curve::Heavy(e) => x.signum() * heavy_pos(x.abs(), e),
            Curve::Asym(e) => {
                if x >= 0.0 {
                    heavy_pos(x, e)
                } else if x >= -1.0 {
                    (x + 1.0).powf(e) - 1.0
                } else {
                    x
                }
            }
            Curve::Rectify(r, s) => (1.0 - r) * x + r * s * x.abs(),
            Curve::Fold(g) => tri(g * x),
            Curve::SineFold(g) => (std::f64::consts::FRAC_PI_2 * g * x).sin(),
            Curve::ZeroCross(t) => {
                let a = x.abs();
                let w = ZC_KNEE * t;
                let y = if a <= t - w {
                    0.0
                } else if a <= t + w {
                    let d = a - (t - w);
                    d * d / (4.0 * w)
                } else {
                    a - t
                };
                x.signum() * y
            }
            Curve::Crush(q) => q * (x / q).round(),
            Curve::Tube(g) => (g * x + TUBE_BIAS).tanh() - TANH_TUBE_BIAS,
            Curve::BiasClip(g, b) => clip_f(g * x + b) - clip_f(b),
        }
    }

    #[inline]
    pub fn af(&self, x: f64) -> f64 {
        match *self {
            Curve::Lin => 0.5 * x * x,
            Curve::Tanh(g) => ln_cosh(g * x) / g,
            Curve::Clip(g) => clip_af(g * x) / g,
            Curve::Heavy(e) => heavy_pos_af(x.abs(), e),
            Curve::Asym(e) => {
                if x >= 0.0 {
                    heavy_pos_af(x, e)
                } else if x >= -1.0 {
                    ((x + 1.0).powf(e + 1.0) - 1.0) / (e + 1.0) - x
                } else {
                    // F(−1) = 1 − 1 / (e + 1), then ∫ s ds
                    1.0 - 1.0 / (e + 1.0) + 0.5 * (x * x - 1.0)
                }
            }
            Curve::Rectify(r, s) => 0.5 * (1.0 - r) * x * x + 0.5 * r * s * x * x.abs(),
            Curve::Fold(g) => tri_af(g * x) / g,
            Curve::SineFold(g) => {
                let k = std::f64::consts::FRAC_PI_2 * g;
                -(k * x).cos() / k
            }
            Curve::ZeroCross(t) => {
                let a = x.abs();
                let w = ZC_KNEE * t;
                if a <= t - w {
                    0.0
                } else if a <= t + w {
                    let d = a - (t - w);
                    d * d * d / (12.0 * w)
                } else {
                    let u = a - t;
                    2.0 * w * w / 3.0 + 0.5 * (u * u - w * w)
                }
            }
            Curve::Crush(q) => {
                let n = (x / q).round();
                q * n * x - 0.5 * q * q * n * n
            }
            Curve::Tube(g) => (ln_cosh(g * x + TUBE_BIAS) - g * x * TANH_TUBE_BIAS) / g,
            Curve::BiasClip(g, b) => clip_af(g * x + b) / g - x * clip_f(b),
        }
    }
}

// ---- level compensation ----------------------------------------------------------------------------

/// The compensated module families, in table order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comp {
    Soft,
    Hard,
    Heavy,
    Asym,
    Rectify,
    Fold,
    SineFold,
    ZeroCross,
    Tube,
    Fuzz,
}

pub const COMPS: [Comp; 10] = [Comp::Soft, Comp::Hard, Comp::Heavy, Comp::Asym, Comp::Rectify, Comp::Fold, Comp::SineFold, Comp::ZeroCross, Comp::Tube, Comp::Fuzz];

/// The curve of a compensated family at Amount `a` (Fuzz: its first stage).
pub fn comp_curve(c: Comp, a: f64) -> Curve {
    match c {
        Comp::Soft => Curve::Tanh(drive_gain(a)),
        Comp::Hard => Curve::Clip(drive_gain(a)),
        Comp::Heavy => Curve::Heavy(heavy_exp(a)),
        Comp::Asym => Curve::Asym(heavy_exp(a)),
        Comp::Rectify => Curve::Rectify(a / 100.0, 1.0),
        Comp::Fold => Curve::Fold(drive_gain(a)),
        Comp::SineFold => Curve::SineFold(drive_gain(a)),
        Comp::ZeroCross => Curve::ZeroCross(10f64.powf(zc_threshold_db(a) / 20.0)),
        Comp::Tube => Curve::Tube(drive_gain(a)),
        Comp::Fuzz => Curve::BiasClip(FUZZ_G1 * drive_gain(a), FUZZ_B1),
    }
}

fn remove_mean(v: &mut [f64]) {
    let m = v.iter().sum::<f64>() / v.len() as f64;
    v.iter_mut().for_each(|s| *s -= m);
}

/// Harmonics of the reference that the make-up counts: those of a 1 kHz sine below 20 kHz (what
/// leaves the pedal; a folder at full drive puts most of its energy above, which the downsampler
/// removes).
pub const COMP_HARMONICS: usize = 20;

/// The static make-up of a family at Amount `a` (dB): RMS ratio in / out of one period of a sine
/// peaking at the reference level, the output counted on its first [`COMP_HARMONICS`] harmonics (no
/// DC: the band's DC block removes it), limited to [`COMP_MIN_DB`] – [`COMP_MAX_DB`]. Fuzz: the two
/// stages, the inter-stage high-pass taken as a mean removal (it sits at 40 Hz, the reference at 1 kHz).
pub fn makeup_db(c: Comp, a: f64) -> f64 {
    const N: usize = 4096;
    let amp = 10f64.powf(REF_PEAK_DB / 20.0);
    let x: Vec<f64> = (0..N).map(|i| amp * (std::f64::consts::TAU * (i as f64 + 0.5) / N as f64).sin()).collect();
    let curve = comp_curve(c, a);
    let mut y: Vec<f64> = x.iter().map(|&v| curve.f(v)).collect();
    if c == Comp::Fuzz {
        remove_mean(&mut y);
        let s2 = Curve::BiasClip(FUZZ_G2, FUZZ_B2);
        y.iter_mut().for_each(|v| *v = s2.f(*v));
    }
    let (cs, sn): (Vec<f64>, Vec<f64>) = (0..N).map(|i| {
        let w = std::f64::consts::TAU * i as f64 / N as f64;
        (w.cos(), w.sin())
    }).unzip();
    let mut power = 0.0;
    for h in 1..=COMP_HARMONICS {
        let (mut re, mut im) = (0.0, 0.0);
        for (n, &v) in y.iter().enumerate() {
            let k = (h * n) % N;
            re += v * cs[k];
            im += v * sn[k];
        }
        let (re, im) = (2.0 * re / N as f64, 2.0 * im / N as f64);
        power += 0.5 * (re * re + im * im);
    }
    let out = power.sqrt();
    let db = if out < 1e-12 { COMP_MAX_DB } else { 20.0 * (amp / std::f64::consts::SQRT_2 / out).log10() };
    db.clamp(COMP_MIN_DB, COMP_MAX_DB)
}

/// The table of one family: [`COMP_POINTS`] make-ups (dB) at a = 100 k / (COMP_POINTS − 1).
pub fn comp_table(c: Comp) -> [f64; COMP_POINTS] {
    std::array::from_fn(|k| makeup_db(c, 100.0 * k as f64 / (COMP_POINTS - 1) as f64))
}
