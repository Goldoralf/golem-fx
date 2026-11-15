// SPDX-License-Identifier: GPL-3.0-or-later
//! The models' anti-alias and reconstruction filters (README §2.4): analog prototypes normalised to a
//! 1 rad/s edge (elliptic, Butterworth, a 4-pole ladder), bilinear-transformed with the edge
//! prewarped, realised as TPT SVF sections whose coefficients ramp between control updates.

use gfx_dsp::matched::Biquad;
use gfx_dsp::svf::Svf;
use num_complex::Complex64 as C;
use std::f64::consts::PI;

/// Most second-order sections a prototype holds (a 9th-order elliptic: four, plus one real pole).
pub const MAX_SECTIONS: usize = 4;

/// An analog low-pass prototype: second-order sections n(s) / d(s) and an optional real pole −σ.
#[derive(Clone, Copy, Debug, Default)]
pub struct Proto {
    pub secs: [([f64; 3], [f64; 3]); MAX_SECTIONS],
    pub n: usize,
    /// the real pole's σ (0: none)
    pub real: f64,
}

impl Proto {
    fn push(&mut self, num: [f64; 3], den: [f64; 3]) {
        self.secs[self.n] = (num, den);
        self.n += 1;
    }

    /// |H(jΩ)| (Ω relative to the edge).
    pub fn mag(&self, w: f64) -> f64 {
        let mut m = 1.0;
        for (num, den) in &self.secs[..self.n] {
            let a = C::new(num[0] - num[2] * w * w, num[1] * w).norm();
            let b = C::new(den[0] - den[2] * w * w, den[1] * w).norm();
            m *= a / b;
        }
        if self.real > 0.0 {
            m *= self.real / C::new(self.real, w).norm();
        }
        m
    }

    /// Butterworth low-pass of an even order (2, 4, 6, 8).
    pub fn butterworth(order: usize) -> Proto {
        let mut p = Proto::default();
        for k in 0..order / 2 {
            let theta = PI * (2 * k + 1) as f64 / (2 * order) as f64;
            p.push([1.0, 0.0, 0.0], [1.0, 2.0 * theta.sin(), 1.0]);
        }
        p
    }

    /// A 4-pole transistor ladder with feedback `k` (0 – 4), unity at DC: 1 / ((1 + s)⁴ + k) · (1 + k).
    pub fn ladder(k: f64) -> Proto {
        let mut p = Proto::default();
        let r = k.max(0.0).powf(0.25);
        for a in [PI / 4.0, 3.0 * PI / 4.0] {
            let pole = C::new(-1.0 + r * a.cos(), r * a.sin());
            let m2 = pole.norm_sqr();
            p.push([m2, 0.0, 0.0], [m2, -2.0 * pole.re, 1.0]);
        }
        p
    }

    /// Elliptic (Cauer) low-pass of odd order `order`, pass-band ripple `ap` dB to Ω = 1, stop band
    /// `as_db` down from Ω = 1 / k (returned with the prototype). Orfanidis, "Lecture Notes on
    /// Elliptic Filter Design" (2006): Landen-transformation forms of cd, sn and their inverses.
    pub fn elliptic(order: usize, ap: f64, as_db: f64) -> (Proto, f64) {
        let ep = (10f64.powf(ap / 10.0) - 1.0).sqrt();
        let es = (10f64.powf(as_db / 10.0) - 1.0).sqrt();
        let k1 = ep / es;
        let k = ellipdeg(order, k1);
        let l = order / 2;
        let v0 = (-C::i() * asne(C::i() / ep, k1) / order as f64).re;
        let mut p = Proto::default();
        for i in 1..=l {
            let u = (2 * i - 1) as f64 / order as f64;
            let zeta = cde(C::new(u, 0.0), k).re;
            let z = 1.0 / (k * zeta); // zero at ±jz
            let pole = C::i() * cde(C::new(u, -v0), k);
            let m2 = pole.norm_sqr();
            // unity at DC
            let g = m2 / (z * z);
            p.push([g * z * z, 0.0, g], [m2, -2.0 * pole.re, 1.0]);
        }
        if order % 2 == 1 {
            let p0 = C::i() * sne(C::new(0.0, v0), k);
            p.real = -p0.re;
        }
        (p, 1.0 / k)
    }
}

const LANDEN: usize = 8;

fn landen(k: f64) -> [f64; LANDEN] {
    let mut v = [0.0; LANDEN];
    let mut k = k;
    for x in v.iter_mut() {
        k = (k / (1.0 + (1.0 - k * k).sqrt())).powi(2);
        *x = k;
    }
    v
}

fn cde(u: C, k: f64) -> C {
    let v = landen(k);
    let mut w = (u * PI / 2.0).cos();
    for n in (0..LANDEN).rev() {
        w = (1.0 + v[n]) * w / (1.0 + v[n] * w * w);
    }
    w
}

fn sne(u: C, k: f64) -> C {
    let v = landen(k);
    let mut w = (u * PI / 2.0).sin();
    for n in (0..LANDEN).rev() {
        w = (1.0 + v[n]) * w / (1.0 + v[n] * w * w);
    }
    w
}

fn acde(w: C, k: f64) -> C {
    let v = landen(k);
    let mut w = w;
    for n in 0..LANDEN {
        let v1 = if n == 0 { k } else { v[n - 1] };
        w = w / (1.0 + (1.0 - w * w * v1 * v1).sqrt()) * 2.0 / (1.0 + v[n]);
    }
    w.acos() * 2.0 / PI
}

fn asne(w: C, k: f64) -> C {
    1.0 - acde(w, k)
}

/// The selectivity k = Ωp / Ωs an order-`n` elliptic reaches with discrimination `k1`.
fn ellipdeg(n: usize, k1: f64) -> f64 {
    let kp1 = (1.0 - k1 * k1).sqrt();
    let mut prod = 1.0;
    for i in 1..=n / 2 {
        let u = (2 * i - 1) as f64 / n as f64;
        prod *= sne(C::new(u, 0.0), kp1).re;
    }
    let kp = kp1.powi(n as i32) * prod.powi(4);
    (1.0 - kp * kp).sqrt()
}

/// Bilinear transform of n(s) / d(s), the prototype's Ω = 1 mapped to `fc`.
pub fn bilinear(num: &[f64; 3], den: &[f64; 3], fc: f64, sr: f64) -> Biquad {
    let k = 1.0 / (PI * fc / sr).tan();
    let k2 = k * k;
    let b = [num[0] + num[1] * k + num[2] * k2, 2.0 * num[0] - 2.0 * num[2] * k2, num[0] - num[1] * k + num[2] * k2];
    let a = [den[0] + den[1] * k + den[2] * k2, 2.0 * den[0] - 2.0 * den[2] * k2, den[0] - den[1] * k + den[2] * k2];
    Biquad { b0: b[0] / a[0], b1: b[1] / a[0], b2: b[2] / a[0], a1: a[1] / a[0], a2: a[2] / a[0] }
}

/// A first-order low-pass σ / (s + σ), bilinear, its coefficients ramped like the SVF's.
#[derive(Clone, Copy, Debug, Default)]
struct RealPole {
    b: f64,
    a: f64,
    db: f64,
    da: f64,
    left: u32,
    x1: f64,
    y1: f64,
}

impl RealPole {
    fn set(&mut self, sigma: f64, fc: f64, sr: f64, ramp: u32) {
        let k = 1.0 / (PI * fc / sr).tan();
        let b = sigma / (k + sigma);
        let a = (sigma - k) / (k + sigma);
        if ramp <= 1 {
            self.b = b;
            self.a = a;
            self.left = 0;
        } else {
            self.db = (b - self.b) / ramp as f64;
            self.da = (a - self.a) / ramp as f64;
            self.left = ramp;
        }
    }

    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        if self.left > 0 {
            self.left -= 1;
            self.b += self.db;
            self.a += self.da;
        }
        let y = self.b * (x + self.x1) - self.a * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

/// A prototype realised at the host rate.
#[derive(Clone, Debug, Default)]
pub struct Cascade {
    svf: [Svf; MAX_SECTIONS],
    n: usize,
    real: Option<RealPole>,
}

impl Cascade {
    pub fn reset(&mut self) {
        for s in self.svf.iter_mut() {
            s.reset();
        }
        if let Some(r) = self.real.as_mut() {
            r.x1 = 0.0;
            r.y1 = 0.0;
        }
    }

    /// No filtering.
    pub fn clear(&mut self) {
        self.n = 0;
        self.real = None;
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0 && self.real.is_none()
    }

    /// The prototype's edge at `fc`, reached over `ramp` samples (0: at once, and the state cleared
    /// when the structure changes).
    pub fn design(&mut self, proto: &Proto, fc: f64, sr: f64, ramp: u32) {
        let fc = fc.clamp(10.0, 0.45 * sr);
        if proto.n != self.n || (proto.real > 0.0) != self.real.is_some() {
            self.n = proto.n;
            self.real = if proto.real > 0.0 { Some(RealPole::default()) } else { None };
            self.reset();
        }
        for (s, (num, den)) in self.svf.iter_mut().zip(proto.secs.iter()).take(proto.n) {
            let b = bilinear(num, den, fc, sr);
            if ramp > 1 {
                s.set_biquad_ramped(&b, ramp);
            } else {
                s.set_biquad(&b);
            }
        }
        if let Some(r) = self.real.as_mut() {
            r.set(proto.real, fc, sr, ramp);
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let mut y = x;
        if let Some(r) = self.real.as_mut() {
            y = r.process(y as f64) as f32;
        }
        for s in self.svf[..self.n].iter_mut() {
            y = s.process(y);
        }
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elliptic_meets_its_edges() {
        let (p, ws) = Proto::elliptic(9, 0.1, 70.0);
        assert!(ws <= 0.55 / 0.45, "stop edge {ws}");
        let mut ripple: f64 = 0.0;
        for i in 0..=1000 {
            ripple = ripple.max((20.0 * p.mag(i as f64 / 1000.0).log10()).abs());
        }
        assert!(ripple <= 0.1 + 1e-6, "{ripple}");
        let mut stop = f64::MIN;
        for i in 0..2000 {
            stop = stop.max(20.0 * p.mag(ws * (1.0 + i as f64 * 0.01)).log10());
        }
        assert!(stop <= -70.0 + 1e-6, "{stop}");
        assert!((p.mag(0.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn digital_sections_follow_the_prototype() {
        let sr = 48000.0;
        let (p, _) = Proto::elliptic(9, 0.1, 70.0);
        let mut c = Cascade::default();
        c.design(&p, 3600.0, sr, 0);
        // flat to the edge, ≥ 70 dB from 0.55 / 0.45 of it
        for f in [100.0, 1000.0, 3000.0, 3590.0] {
            let g = gfx_dsp::svf::measure_gain_db(|x| c.process(x), f, sr as f32);
            assert!(g.abs() < 0.15, "{f}: {g}");
        }
        for f in [4400.0, 5000.0, 8000.0, 15000.0] {
            let g = gfx_dsp::svf::measure_gain_db(|x| c.process(x), f, sr as f32);
            assert!(g < -69.0, "{f}: {g}");
        }
        let b = Proto::butterworth(6);
        c.design(&b, 3000.0, sr, 0);
        let g = gfx_dsp::svf::measure_gain_db(|x| c.process(x), 3000.0, sr as f32);
        assert!((g + 3.01).abs() < 0.1, "{g}");
    }
}
