// SPDX-License-Identifier: GPL-3.0-or-later
//! The medium's faults and noises (README §2.4), the companders (§2.6), the ALC (§2.5) and the disc
//! core (§2.2, Vinyl and Shellac). Every random source is seeded (golem contract).

use gfx_dsp::adaa::{Adaa1, Shaper};
use gfx_dsp::envelope::SlidingMax;
use gfx_dsp::lfo::Rng;
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::tape::{BandNoise, Shelf1};
use std::f64::consts::PI;

/// A standard normal draw (Box-Muller).
#[inline]
pub fn normal(rng: &mut Rng) -> f64 {
    let u1 = 1.0 - rng.next_f64();
    let u2 = rng.next_f64();
    (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos()
}

/// One dropout: a spacing excursion with a raised-cosine rise and fall, its depth per channel
/// (log-depths correlated ρ = 0.7: the debris spans part of the tape's width).
#[derive(Clone, Copy, Debug, Default)]
struct Drop {
    t: f64,
    rise: f64,
    hold: f64,
    fall: f64,
    d: [f64; 2],
    on: bool,
}

impl Drop {
    #[inline]
    fn env(&self) -> f64 {
        let t = self.t;
        if t < self.rise {
            0.5 - 0.5 * (PI * t / self.rise).cos()
        } else if t < self.rise + self.hold {
            1.0
        } else {
            let u = (t - self.rise - self.hold) / self.fall;
            if u >= 1.0 {
                0.0
            } else {
                0.5 + 0.5 * (PI * u).cos()
            }
        }
    }
}

const POOL: usize = 6;
/// Dropout depth: lognormal, median 3 µm, 1 % beyond 15 µm.
const DEPTH_MED: f64 = 3e-6;
const DEPTH_SIGMA: f64 = 0.69;

/// A lane of spacing events (dropouts and crinkle bursts) for one or two tracks.
#[derive(Clone, Debug)]
pub struct Lane {
    ev: [Drop; POOL],
    rng: Rng,
    crink_n: BandNoise,
    crink_am: BandNoise,
    crink_t: f64,
    crink_len: f64,
    crink_on: bool,
    fs: f64,
    /// events started (for the tests)
    pub count: u64,
}

impl Lane {
    pub fn new(seed: u64) -> Self {
        Self { ev: [Drop::default(); POOL], rng: Rng::new(seed), crink_n: BandNoise::new(seed + 7), crink_am: BandNoise::new(seed + 8), crink_t: 0.0, crink_len: 0.0, crink_on: false, fs: 48000.0, count: 0 }
    }

    pub fn prepare(&mut self, fs: f64) {
        self.fs = fs;
        self.crink_n.design(20.0, 80.0, fs);
        self.crink_am.design(5.0, 40.0, fs);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.ev = [Drop::default(); POOL];
        self.rng.reset();
        self.crink_n.reset();
        self.crink_am.reset();
        self.crink_on = false;
        self.count = 0;
    }

    /// A uniform draw from the lane's own source (the grid's dice).
    pub fn rng_draw(&mut self) -> f64 {
        self.rng.next_f64()
    }

    /// Starts a dropout now.
    pub fn trigger(&mut self) {
        let r = &mut self.rng;
        let rise = 0.002 + 0.008 * r.next_f64();
        let hold = (0.040 * (0.5 * normal(r)).exp()).clamp(0.005, 0.4);
        let fall = 0.010 + 0.070 * r.next_f64();
        let z0 = normal(r);
        let z1 = normal(r);
        let d0 = (DEPTH_MED * (DEPTH_SIGMA * z0).exp()).min(30e-6);
        let d1 = (DEPTH_MED * (DEPTH_SIGMA * (0.7 * z0 + 0.714 * z1)).exp()).min(30e-6);
        let e = Drop { t: 0.0, rise, hold, fall, d: [d0, d1], on: true };
        if let Some(slot) = self.ev.iter_mut().find(|x| !x.on) {
            *slot = e;
        } else {
            // the oldest gives way
            let k = (0..POOL).max_by(|a, b| self.ev[*a].t.total_cmp(&self.ev[*b].t)).unwrap_or(0);
            self.ev[k] = e;
        }
        self.count += 1;
    }

    /// One sample: spacing (m) per track and the level dip (dB) per track. `rate` dropouts per second
    /// (Poisson; 0 when synced), `crinkle` 0 … 1 and its burst rate per second.
    #[inline]
    pub fn next(&mut self, rate: f64, crinkle: f64, crinkle_rate: f64) -> ([f64; 2], [f64; 2]) {
        let dt = 1.0 / self.fs;
        if rate > 0.0 && self.rng.next_f64() < rate * dt {
            self.trigger();
        }
        let mut d = [0.0f64; 2];
        for e in self.ev.iter_mut().filter(|e| e.on) {
            let g = e.env();
            d[0] += g * e.d[0];
            d[1] += g * e.d[1];
            e.t += dt;
            if e.t >= e.rise + e.hold + e.fall {
                e.on = false;
            }
        }
        // deep loss of contact: −0.3 dB per µm beyond 5 µm
        let mut dip = [-0.3e6 * (d[0] - 5e-6).max(0.0), -0.3e6 * (d[1] - 5e-6).max(0.0)];
        // crinkle bursts
        if !self.crink_on && crinkle_rate > 0.0 && self.rng.next_f64() < crinkle_rate * dt {
            self.crink_on = true;
            self.crink_t = 0.0;
            self.crink_len = 0.1 + 0.5 * self.rng.next_f64();
        }
        if self.crink_on {
            let u = self.crink_t / self.crink_len;
            let edge = 0.1;
            let env = if u < edge {
                0.5 - 0.5 * (PI * u / edge).cos()
            } else if u > 1.0 - edge {
                0.5 + 0.5 * (PI * (u - 1.0 + edge) / edge).cos()
            } else {
                1.0
            };
            let n = self.crink_n.next().abs().min(3.0) / 3.0;
            let am = self.crink_am.next().clamp(-1.0, 1.0);
            let dd = 4e-6 * crinkle * env * n;
            d[0] += dd;
            d[1] += dd;
            dip[0] += crinkle * env * am;
            dip[1] += crinkle * env * am;
            self.crink_t += dt;
            if self.crink_t >= self.crink_len {
                self.crink_on = false;
            }
        }
        (d, dip)
    }
}

/// Clicks and scratches (README §2.4 Crackle): Pareto amplitudes (α 1.5), each impulse shaped by a
/// stylus resonance (10 – 14 kHz) and the disc's de-emphasis, or duller splice ticks on tape; on
/// discs some repeat once per rotation, drifting slowly.
#[derive(Clone, Debug)]
pub struct Crackle {
    rng: Rng,
    res: Svf,
    deemph: Shelf1,
    scratch: [(f64, f64); 6],
    rot: f64,
    fs: f64,
}

impl Crackle {
    pub fn new(seed: u64) -> Self {
        Self { rng: Rng::new(seed), res: Svf::new(), deemph: Shelf1::identity(), scratch: [(0.0, 0.0); 6], rot: 0.0, fs: 48000.0 }
    }

    pub fn prepare(&mut self, fs: f64) {
        self.fs = fs;
        self.deemph = Shelf1::design(1e9, 2122.0, fs);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.rng.reset();
        self.res.reset();
        self.deemph.reset();
        for s in self.scratch.iter_mut() {
            *s = (self.rng.next_f64(), 2.0 + 4.0 * self.rng.next_f64());
        }
        self.rot = 0.0;
    }

    fn pareto(&mut self) -> f64 {
        let u = 1.0 - self.rng.next_f64();
        u.powf(-1.0 / 1.5).min(30.0)
    }

    /// One sample. `rate` clicks per second, `amp` the median click (peak), `scratches` active
    /// slots, `rot_hz` the rotation (0 on tape), `disc` the stylus voicing.
    #[inline]
    pub fn next(&mut self, rate: f64, amp: f64, scratches: usize, rot_hz: f64, disc: bool) -> f32 {
        let mut imp = 0.0;
        let mut fire = |s: &mut Self, a: f64| {
            let f = if disc { 10_000.0 + 4000.0 * s.rng.next_f64() } else { 2500.0 + 1500.0 * s.rng.next_f64() };
            let q = if disc { 3.0 } else { 1.5 };
            s.res.set(SvfMode::BandPass, f as f32, q, 0.0, s.fs as f32);
            imp += a * s.fs / 48000.0 * if s.rng.next_f64() < 0.5 { 1.0 } else { -1.0 };
        };
        if rate > 0.0 && self.rng.next_f64() < rate / self.fs {
            let a = amp * self.pareto();
            fire(self, a);
        }
        if rot_hz > 0.0 {
            let prev = self.rot;
            self.rot += rot_hz / self.fs;
            let wrap = self.rot >= 1.0;
            if wrap {
                self.rot -= 1.0;
            }
            for k in 0..scratches.min(6) {
                let (pos, a) = self.scratch[k];
                let hit = if wrap { pos > prev || pos <= self.rot } else { pos > prev && pos <= self.rot };
                if hit {
                    fire(self, amp * a);
                }
                if wrap {
                    let p = (self.scratch[k].0 + 0.002 * self.rng.bipolar() as f64).rem_euclid(1.0);
                    self.scratch[k].0 = p;
                }
            }
        }
        let y = self.res.process(imp as f32) as f64;
        (if disc { self.deemph.process(y) } else { y }) as f32
    }
}

/// One track's hiss, mechanical noise and modulation noise generators (independent per track).
#[derive(Clone, Debug)]
pub struct TrackNoise {
    rng: Rng,
    pub hiss_shape: Shelf1,
    motor_c: f64,
    motor_s: f64,
    motor_rot: (f64, f64),
    motor_norm: f64,
    mech_band: BandNoise,
    pub modn: BandNoise,
    fs: f64,
}

impl TrackNoise {
    pub fn new(seed: u64) -> Self {
        Self { rng: Rng::new(seed), hiss_shape: Shelf1::identity(), motor_c: 1.0, motor_s: 0.0, motor_rot: (1.0, 0.0), motor_norm: 1.0, mech_band: BandNoise::new(seed + 1), modn: BandNoise::new(seed + 2), fs: 48000.0 }
    }

    pub fn prepare(&mut self, fs: f64) {
        self.fs = fs;
        self.modn.design(20.0, 5000.0, fs);
        // 6 harmonics at 1 / k: rms² = Σ 1 / 2k²
        self.motor_norm = 1.0 / (0.5 * (1..=6).map(|k| 1.0 / (k * k) as f64).sum::<f64>()).sqrt();
    }

    pub fn reset(&mut self) {
        self.rng.reset();
        self.hiss_shape.reset();
        self.motor_c = 1.0;
        self.motor_s = 0.0;
        self.mech_band.reset();
        self.modn.reset();
    }

    /// The hiss's spectrum: the replay boost √(1 + (2πfτ)²) up to 20 kHz (tape), or the disc's
    /// de-emphasised surface noise.
    pub fn set_hiss(&mut self, shape: Shelf1) {
        self.hiss_shape.retune(&shape);
    }

    pub fn set_mech(&mut self, motor_hz: f64, band: (f64, f64)) {
        let d = 2.0 * PI * motor_hz / self.fs;
        self.motor_rot = (d.cos(), d.sin());
        self.mech_band.design(band.0, band.1, self.fs);
    }

    /// White noise through the hiss shape (unit-variance input).
    #[inline]
    pub fn hiss(&mut self) -> f64 {
        let w = self.rng.bipolar() as f64 * 3f64.sqrt();
        self.hiss_shape.process(w)
    }

    /// Motor series and band, in power shares (unit variance overall).
    #[inline]
    pub fn mech(&mut self, motor_share: f64) -> f64 {
        let mut m = 0.0;
        if motor_share > 0.0 && self.motor_rot.1 != 0.0 {
            // a rotating phasor, renormalised each sample
            let (rc, rs) = self.motor_rot;
            let (c, s) = (self.motor_c * rc - self.motor_s * rs, self.motor_s * rc + self.motor_c * rs);
            let g = 1.5 - 0.5 * (c * c + s * s);
            self.motor_c = c * g;
            self.motor_s = s * g;
            // sin kφ by the Chebyshev recursion
            let (s1, c1) = (self.motor_s, self.motor_c);
            let (mut a, mut b) = (0.0, s1);
            let mut acc = s1;
            for k in 2..=6 {
                let s = 2.0 * c1 * b - a;
                a = b;
                b = s;
                acc += s / k as f64;
            }
            m = acc * self.motor_norm * motor_share.sqrt();
        }
        m + self.mech_band.next() * (1.0 - motor_share).max(0.0).sqrt()
    }
}

/// The additive noises' gain law (README §2.4): Follow, Constant, Duck, from the input before
/// `input` (a 25 ms sliding maximum, release linear in dB at 120 dB/s, exactly 0 below −90 dBFS).
/// Kept in the linear domain: a dB-linear release is a constant factor per sample.
#[derive(Clone, Debug)]
pub struct NoiseLaw {
    max: SlidingMax,
    env: f64,
    rel: f64,
    w: [f64; 3],
    cw: f64,
}

pub const FOLLOW_REF_DB: f64 = -24.0;
pub const FOLLOW_FLOOR_DB: f64 = -90.0;
/// The same, linear.
const FOLLOW_REF: f64 = 0.063_095_734_448_019_33;
const FOLLOW_FLOOR: f64 = 3.162_277_660_168_379_5e-5;

impl NoiseLaw {
    pub fn new() -> Self {
        Self { max: SlidingMax::new(), env: 0.0, rel: 1.0, w: [1.0, 0.0, 0.0], cw: 0.0 }
    }

    pub fn prepare(&mut self, fs: f64) {
        let win = (0.025 * fs).round() as usize;
        self.max.prepare(win);
        self.max.set_window(win);
        self.rel = 10f64.powf(-120.0 / 20.0 / fs);
        self.cw = gfx_dsp::util::one_pole_coef(0.050 / 3.0 * fs);
    }

    pub fn reset(&mut self, mode: usize) {
        self.max.reset();
        self.env = 0.0;
        self.w = [0.0; 3];
        self.w[mode.min(2)] = 1.0;
    }

    /// The detector (dBFS).
    pub fn env_db(&self) -> f64 {
        20.0 * self.env.max(1e-12).log10()
    }

    /// One sample: `level` the largest |x| over the channels (before `input`).
    #[inline]
    pub fn next(&mut self, level: f32, mode: usize) -> f64 {
        let m = self.max.next(level) as f64;
        self.env = if m > self.env { m } else { (self.env * self.rel).max(m) };
        let follow = if self.env < FOLLOW_FLOOR { 0.0 } else { (self.env * (1.0 / FOLLOW_REF)).min(1.0) };
        let g = [follow, 1.0, 1.0 - 0.9 * follow];
        let mut out = 0.0;
        for k in 0..3 {
            let t = if k == mode { 1.0 } else { 0.0 };
            if self.w[k] != t {
                self.w[k] = t + (self.w[k] - t) * self.cw;
                if (self.w[k] - t).abs() < 1e-6 {
                    self.w[k] = t;
                }
            }
            if self.w[k] != 0.0 {
                out += self.w[k] * g[k];
            }
        }
        out
    }
}

impl Default for NoiseLaw {
    fn default() -> Self {
        Self::new()
    }
}

/// The recorder's automatic level control: attack 5 ms, release 1.5 s, −20 dB range, holding the
/// instrument's peaks at −6 dB re the reference sine's peak (before `input`).
#[derive(Clone, Debug, Default)]
pub struct Alc {
    env: f64,
    ca: f64,
    cr: f64,
}

pub const REF_PEAK: f64 = 0.178_04;

impl Alc {
    pub fn prepare(&mut self, fs: f64) {
        self.ca = gfx_dsp::util::one_pole_coef(0.005 * fs);
        self.cr = gfx_dsp::util::one_pole_coef(1.5 * fs);
        self.env = 0.0;
    }

    pub fn reset(&mut self) {
        self.env = 0.0;
    }

    #[inline]
    pub fn next(&mut self, level: f32) -> f64 {
        let a = level as f64;
        let c = if a > self.env { self.ca } else { self.cr };
        self.env = a + (self.env - a) * c;
        let target = REF_PEAK * 10f64.powf(-6.0 / 20.0);
        (target / self.env.max(1e-9)).clamp(0.1, 1.0)
    }
}

/// A 2:1 RMS compander with 10 dB pre-emphasis (README §2.6): both detectors read the encoded
/// signal — the encoder in feedback, the decoder in feed-forward — with the same 10 ms detector, the
/// same one-sample convention and unity at the reference: decode is the exact inverse of encode on a
/// perfect channel, and mistracks as real ones do on tape.
#[derive(Clone, Debug)]
pub struct Compander {
    pre: Shelf1,
    post: Shelf1,
    enc: f64,
    dec: f64,
    c: f64,
    r_ref: f64,
}

/// Gain range of the encoder (dB): +40 in the gaps, −24 at the loudest.
pub const COMP_MAX_DB: f64 = 40.0;
pub const COMP_MIN_DB: f64 = -24.0;

impl Compander {
    pub fn new() -> Self {
        Self { pre: Shelf1::identity(), post: Shelf1::identity(), enc: 0.0, dec: 0.0, c: 0.0, r_ref: 0.125_89 }
    }

    pub fn prepare(&mut self, fs: f64) {
        let pre = Shelf1::design(1000.0, 1000.0 * 10f64.powf(10.0 / 20.0), fs);
        self.post = pre.inverse();
        self.pre = pre;
        self.c = gfx_dsp::util::one_pole_coef(0.010 * fs);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.pre.reset();
        self.post.reset();
        // settled at the reference: unity gain
        self.enc = self.r_ref * self.r_ref;
        self.dec = self.enc;
    }

    #[inline]
    fn gain(&self, ms: f64) -> f64 {
        let (lo, hi) = (10f64.powf(COMP_MIN_DB / 20.0), 10f64.powf(COMP_MAX_DB / 20.0));
        (self.r_ref / ms.max(1e-30).sqrt()).clamp(lo, hi)
    }

    #[inline]
    pub fn encode(&mut self, x: f64) -> f64 {
        let u = self.pre.process(x);
        let y = u * self.gain(self.enc);
        self.enc = y * y + (self.enc - y * y) * self.c;
        y
    }

    #[inline]
    pub fn decode(&mut self, y: f64) -> f64 {
        let z = y / self.gain(self.dec);
        self.dec = y * y + (self.dec - y * y) * self.c;
        self.post.process(z)
    }
}

impl Default for Compander {
    fn default() -> Self {
        Self::new()
    }
}

/// The disc core at the internal rate: record pre-emphasis (RIAA with its 3.18 µs pole, or the 78
/// curve's bass turnover only), the second-order tracing term y = v − 2κ v dv/dt, a soft cutter
/// limit (tanh, ADAA1), and the exact inverse de-emphasis. Unity at low level and 1 kHz.
#[derive(Clone, Debug, Default)]
pub struct Disc {
    s1: Shelf1,
    s2: Shelf1,
    p1: Shelf1,
    p2: Shelf1,
    norm: f64,
    kappa: f64,
    limit: f64,
    adaa: Adaa1,
    v1: f64,
    fs: f64,
    shellac: bool,
}

/// The reference sine's peak inside the disc core (cutter limit at 1: +12 dB of headroom).
pub const DISC_REF: f64 = 0.25;

impl Disc {
    /// `wear` moves the stylus inward (outer 146 mm → inner 60 mm): tracing × (146 / r)².
    pub fn set(&mut self, shellac: bool, fs: f64, wear: f64) {
        let fresh = self.fs != fs || self.shellac != shellac;
        self.fs = fs;
        self.shellac = shellac;
        let t1 = 3180e-6;
        let t2 = 318e-6;
        let s1 = Shelf1::design(1.0 / (2.0 * PI * t1), 1.0 / (2.0 * PI * t2), fs);
        let s2 = if shellac { Shelf1::identity() } else { Shelf1::design(1.0 / (2.0 * PI * 75e-6), 1.0 / (2.0 * PI * 3.18e-6), fs) };
        if fresh {
            self.s1 = s1;
            self.s2 = s2;
            self.p1 = s1.inverse();
            self.p2 = s2.inverse();
            self.adaa.reset(Shaper::Tanh);
        }
        self.norm = 1.0 / (s1.gain_at(1000.0, fs) * s2.gain_at(1000.0, fs));
        let r = 146.0 - 86.0 * wear.clamp(0.0, 1.0);
        // 2nd harmonic −40 dB at the reference, 1 kHz, outer groove (κ A ω = 0.01) [I]
        let k0 = 0.01 / (DISC_REF * 2.0 * PI * 1000.0);
        self.kappa = k0 * (146.0 / r).powi(2) * if shellac { 3.0 } else { 1.0 };
        self.limit = if shellac { 0.7 } else { 1.0 };
    }

    pub fn reset(&mut self) {
        self.s1.reset();
        self.s2.reset();
        self.p1.reset();
        self.p2.reset();
        self.adaa.reset(Shaper::Tanh);
        self.v1 = 0.0;
    }

    /// `x` in input units (the reference sine has peak REF_PEAK).
    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let u = x * (DISC_REF / REF_PEAK);
        let v = self.s2.process(self.s1.process(u)) * self.norm;
        let dv = (v - self.v1) * self.fs;
        self.v1 = v;
        let y = v - 2.0 * self.kappa * v * dv;
        let l = self.limit;
        let y = l * self.adaa.process(Shaper::Tanh, (y / l) as f32) as f64;
        self.p1.process(self.p2.process(y / self.norm)) * (REF_PEAK / DISC_REF)
    }
}

/// Level a 1 kHz sine of peak `a` (disc-core units) loses in the cutter limit (dB, ≤ 0).
pub fn disc_loss_db(a: f64, limit: f64) -> f64 {
    let n = 256;
    let mut s = 0.0;
    for i in 0..n {
        let ph = 2.0 * PI * (i as f64 + 0.5) / n as f64;
        s += limit * (a * ph.sin() / limit).tanh() * ph.sin();
    }
    20.0 * ((2.0 * s / n as f64) / a).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compander_inverts() {
        let fs = 48000.0;
        let mut c = Compander::new();
        c.prepare(fs);
        let mut worst = 0.0f64;
        let mut r = Rng::new(9);
        for i in 0..96000 {
            // bursts from −60 to 0 dBFS
            let lvl = 10f64.powf(-3.0 * ((i / 4800) % 20) as f64 / 20.0);
            let x = lvl * r.bipolar() as f64;
            let e = c.encode(x);
            let y = c.decode(e);
            if i > 1000 {
                worst = worst.max((y - x).abs() / lvl);
            }
        }
        assert!(worst < 1e-9, "{worst}");
    }

    #[test]
    fn follow_falls_at_120_db_per_s() {
        let fs = 48000.0;
        let mut l = NoiseLaw::new();
        l.prepare(fs);
        l.reset(0);
        for _ in 0..4800 {
            l.next(0.5, 0);
        }
        let mut n0 = None;
        for i in 0..48000 {
            let g = l.next(0.0, 0);
            if g == 0.0 && n0.is_none() {
                n0 = Some(i);
            }
        }
        let t = n0.unwrap() as f64 / fs;
        // −6 dBFS to −90 dBFS at 120 dB/s, after the 25 ms window
        assert!((t - (84.0 / 120.0 + 0.025)).abs() < 0.01, "{t}");
    }
}
