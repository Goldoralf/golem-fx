// SPDX-License-Identifier: GPL-3.0-or-later
//! The objects of the Box stage (README §2.7), from textbook acoustics, no recording: the Rust port of
//! `proto/objects.py`, with the review's fixed mode set (r6). Formulas [K]:
//! - closed-open tube (can, bucket, jar): axial f = (2n − 1) c / (4 L′), L′ = L + 0.6 a;
//! - open-open tube (pipe, cone): f = n c / (2 L′), L′ = L + 1.2 a;
//! - closed cylinder transverse modes: f = c / 2π · √((j′_mn / a)² + k_z²), j′ the zeros of J′_m;
//! - rectangular cavity: f = c / 2 · √((l / Lx)² + (m / Ly)² + (n / Lz)²);
//! - Helmholtz resonator: f = c / 2π · √(A / (V L_eff));
//! - thin-ring flexural wall modes: f_n = h / (2π a² √12) · √(E / ρ) · n (n² − 1) / √(n² + 1);
//! - clamped circular plate (a lid): f = λ² / (2π a²) · √(D / (ρ h)), D = E h³ / (12 (1 − ν²)).
//!
//! Q comes from the material, capped per object by a T60 (a held can does not ring for 7 s). Each
//! mode's gain is its shape at the source times at the listening point `position` along the main
//! axis: g(pos) = a + b cos(k π pos). Every object keeps the same ≤ 48 modes at every setting
//! (chosen once by the mean over `position` of g² / f), so `position` only moves gains and `size` only
//! moves frequencies (the edges fade in gfx-dsp `modal`).
//!
//! build.rs includes this file to generate the level, peak and voice tables.

#![allow(dead_code, clippy::needless_range_loop)]

use gfx_dsp::modal::{Mode, MAX_MODES};
use std::f64::consts::PI;

const C: f64 = 343.0;
/// Zeros of J′_m: (value, m); the (0, 0) plane wave is the axial series.
const JP: [(f64, usize); 9] = [(1.841, 1), (3.054, 2), (3.832, 0), (4.201, 3), (5.318, 1), (5.331, 4), (6.416, 5), (6.706, 2), (7.016, 0)];
/// Clamped-plate eigenvalues λ².
const PLATE: [f64; 8] = [10.22, 21.26, 34.88, 39.77, 51.04, 60.82, 69.67, 84.58];

/// Option labels of `object` (enum order, final; append only). `object_b` is "Same" then these.
pub const OBJECT_NAMES: [&str; 10] = ["None", "Tin Can", "Bucket", "Pipe", "Bottle", "Cardboard Box", "Oil Drum", "Glass Jar", "Cone", "Hands"];
pub const OBJECTS: usize = 10;
/// T60 caps (s), per object (voicing [I]).
const T60_MAX: [f64; OBJECTS] = [0.0, 0.6, 0.4, 0.5, 1.0, 0.1, 1.5, 1.2, 0.3, 0.05];
/// No mode rings longer than this at any `decay` (s).
pub const T60_LIMIT: f64 = 4.0;

/// A candidate mode: f (Hz, size 100 %), Q, and its gain law a + b cos(k π pos).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cand {
    pub f: f64,
    pub q: f64,
    pub a: f64,
    pub b: f64,
    pub k: f64,
}

impl Cand {
    #[inline]
    pub fn gain(&self, pos: f64) -> f64 {
        if self.b == 0.0 {
            self.a
        } else {
            self.a + self.b * (self.k * PI * pos).cos()
        }
    }
}

/// An object: its fixed modes and its skirt (2nd-order high-pass and low-pass corners, Hz at size 100
/// %, and the non-resonant path's level in dB).
#[derive(Clone, Copy, Debug)]
pub struct Object {
    pub modes: [Cand; MAX_MODES],
    pub n: usize,
    pub hp: f64,
    pub lp: f64,
    pub skirt_db: f64,
}

fn ring_modes(a: f64, h: f64, e: f64, rho: f64) -> Vec<f64> {
    let k = h / (a * a * 12f64.sqrt()) * (e / rho).sqrt() / (2.0 * PI);
    (2..=9).map(|n| n as f64).map(|n| k * n * (n * n - 1.0) / (n * n + 1.0).sqrt()).collect()
}

fn closed_open(l: f64, a: f64, q_air: f64, fmax: f64) -> Vec<Cand> {
    let lp = l + 0.6 * a;
    let mut out = Vec::new();
    let mut n = 1;
    loop {
        let f = (2 * n - 1) as f64 * C / (4.0 * lp);
        if f > fmax {
            break;
        }
        // the mouth drives volume velocity at the opening (shape ±1); the listening point reads the
        // pressure shape cos((2n − 1) π x / 2)
        let src = ((2 * n - 1) as f64 * PI / 2.0).sin();
        let q = q_air * (1.0 + 0.15 * (n - 1) as f64).powf(-0.5);
        out.push(Cand { f, q, a: src * 0.35, b: src * 0.65, k: (2 * n - 1) as f64 / 2.0 });
        n += 1;
    }
    for &(jv, m) in JP.iter() {
        for q in 0..3 {
            let kz = (2 * q + 1) as f64 * PI / (2.0 * lp);
            let f = C / (2.0 * PI) * ((jv / a).powi(2) + kz * kz).sqrt();
            if f < fmax {
                let w = if m != 0 { 0.6 } else { 1.0 };
                out.push(Cand { f, q: q_air * 0.8, a: 0.0, b: 0.5 * w, k: (2 * q + 1) as f64 / 2.0 });
            }
        }
    }
    out
}

fn open_open(l: f64, a: f64, q_air: f64, fmax: f64) -> Vec<Cand> {
    let lp = l + 1.2 * a;
    let mut out = Vec::new();
    let mut n = 1;
    while n as f64 * C / (2.0 * lp) < fmax {
        let f = n as f64 * C / (2.0 * lp);
        // radiation lowers Q with n
        out.push(Cand { f, q: q_air * (n as f64).powf(-0.35), a: 0.4, b: 0.6, k: n as f64 });
        n += 1;
    }
    out
}

fn rect(lx: f64, ly: f64, lz: f64, q: f64, fmax: f64) -> Vec<Cand> {
    let mut out = Vec::new();
    for l in 0..12 {
        for m in 0..10 {
            for n in 0..10 {
                if l == 0 && m == 0 && n == 0 {
                    continue;
                }
                let f = C / 2.0 * ((l as f64 / lx).powi(2) + (m as f64 / ly).powi(2) + (n as f64 / lz).powi(2)).sqrt();
                if f < fmax {
                    // source in a corner of the open face (every cosine 1), listening along x
                    let w = if m > 0 { 0.5 } else { 1.0 } * if n > 0 { 0.5 } else { 1.0 };
                    out.push(Cand { f, q, a: 0.0, b: w, k: l as f64 });
                }
            }
        }
    }
    out
}

fn helmholtz(area: f64, vol: f64, leff: f64) -> f64 {
    C / (2.0 * PI) * (area / (vol * leff)).sqrt()
}

fn constant(f: f64, q: f64, g: f64) -> Cand {
    Cand { f, q, a: g, b: 0.0, k: 0.0 }
}

/// The object's candidate modes and skirt (hp, lp, skirt dB), size 100 %, before the T60 cap.
fn candidates(obj: usize) -> (Vec<Cand>, f64, f64, f64) {
    match obj {
        1 => {
            // Tin Can: closed-open 11 cm × ⌀ 7.4 cm, steel wall rings (0.22 mm)
            let (a, l) = (0.037, 0.11);
            let mut m = closed_open(l, a, 28.0, 8000.0);
            m.extend(ring_modes(a, 0.00022, 200e9, 7850.0).into_iter().map(|f| constant(f, 260.0, 0.12)));
            (m, 450.0, 6500.0, -14.0)
        }
        2 => {
            // Bucket: closed-open 27 cm × ⌀ 26 cm, plastic wall
            let (a, l) = (0.13, 0.27);
            let mut m = closed_open(l, a, 14.0, 3000.0);
            m.extend(ring_modes(a, 0.0018, 1.5e9, 950.0).into_iter().map(|f| constant(f, 35.0, 0.15)));
            (m, 150.0, 3500.0, -12.0)
        }
        3 => (open_open(0.60, 0.025, 45.0, 6000.0), 200.0, 7000.0, -16.0),
        4 => {
            // Bottle: Helmholtz (0.75 L, 9.5 mm neck) + body (its first axial mode is the Helmholtz) +
            // glass rings
            let fh = helmholtz(PI * 0.0095 * 0.0095, 7.5e-4, 0.08 + 1.7 * 0.0095);
            let mut m = vec![constant(fh, 40.0, 1.4)];
            m.extend(closed_open(0.22, 0.04, 30.0, 7000.0).into_iter().skip(1));
            m.extend(ring_modes(0.04, 0.003, 70e9, 2500.0).into_iter().filter(|&f| f < 9000.0).map(|f| constant(f, 700.0, 0.10)));
            (m, 90.0, 5000.0, -16.0)
        }
        5 => (rect(0.40, 0.30, 0.30, 7.0, 2500.0), 120.0, 2200.0, -6.0),
        6 => {
            // Oil Drum: closed cylinder 88 cm × ⌀ 58 cm, steel lid modes
            let (a, l) = (0.29, 0.88);
            let mut m = Vec::new();
            for n in 1..30 {
                let f = n as f64 * C / (2.0 * l);
                if f > 2500.0 {
                    break;
                }
                m.push(Cand { f, q: 60.0 * (n as f64).powf(-0.2), a: 0.0, b: 1.0, k: n as f64 });
            }
            for &(jv, _) in JP[..6].iter() {
                for q in 0..3 {
                    let f = C / (2.0 * PI) * ((jv / a).powi(2) + (q as f64 * PI / l).powi(2)).sqrt();
                    if f < 2500.0 {
                        m.push(Cand { f, q: 50.0, a: 0.0, b: 0.4, k: q as f64 });
                    }
                }
            }
            let s = ((200e9 * 0.001f64.powi(3) / (12.0 * 0.91)) / (7850.0 * 0.001)).sqrt();
            m.extend(PLATE.iter().map(|lam| constant(lam * s / (2.0 * PI * a * a), 120.0, 0.5)));
            (m, 40.0, 2500.0, -10.0)
        }
        7 => {
            // Glass Jar: closed-open 15 cm × ⌀ 9 cm, glass wall rings (3 mm)
            let (a, l) = (0.045, 0.15);
            let mut m = closed_open(l, a, 32.0, 8000.0);
            m.extend(ring_modes(a, 0.003, 70e9, 2500.0).into_iter().filter(|&f| f < 12000.0).map(|f| constant(f, 900.0, 0.10)));
            (m, 350.0, 7000.0, -14.0)
        }
        // Cone: a truncated cone 62 cm, the mouth at the narrow end: the complete-cone series
        8 => (open_open(0.62, 0.15, 14.0, 6000.0), 280.0, 7500.0, -4.0),
        9 => {
            // Hands: Helmholtz 150 cm³, 2 cm² opening, and one overtone
            let fh = helmholtz(2.0e-4, 1.5e-4, 0.015);
            (vec![constant(fh, 3.0, 1.0), constant(2.6 * fh, 4.0, 0.35)], 180.0, 3200.0, -3.0)
        }
        _ => (Vec::new(), 20.0, 20000.0, 0.0),
    }
}

/// Points over `position` of the fixed-set selection (and of the level table).
pub const POS_POINTS: usize = 65;

impl Object {
    /// The object `obj` (0 = None: no mode, a flat skirt at 0 dB).
    pub fn new(obj: usize) -> Self {
        let (cands, hp, lp, skirt_db) = candidates(obj);
        let t60 = T60_MAX[obj.min(OBJECTS - 1)];
        // the T60 cap (Q ≤ π f T60 / 6.91) and the low limit of the generator (60 Hz at size 100 %)
        let mut m: Vec<Cand> = cands.into_iter().filter(|c| c.f >= 60.0).map(|c| Cand { q: c.q.min(PI * c.f * t60 / 6.91), ..c }).collect();
        if m.len() > MAX_MODES {
            // the fixed set: the strongest by the mean over position of g² / f (low, strong modes first)
            let score = |c: &Cand| (0..POS_POINTS).map(|i| c.gain(i as f64 / (POS_POINTS - 1) as f64).powi(2)).sum::<f64>() / c.f;
            m.sort_by(|x, y| score(y).total_cmp(&score(x)));
            m.truncate(MAX_MODES);
        }
        m.sort_by(|x, y| x.f.total_cmp(&y.f));
        let mut modes = [Cand::default(); MAX_MODES];
        modes[..m.len()].copy_from_slice(&m);
        Self { modes, n: m.len(), hp, lp, skirt_db }
    }

    /// The modes at `pos` (0 – 1), `size` and `decay` (ratios, 1 = 100 %), before the edge fades:
    /// f / size, Q × decay capped at a 4 s T60, the gain at the listening point times `scale`.
    pub fn modes(&self, pos: f64, size: f64, decay: f64, scale: f64, out: &mut [Mode; MAX_MODES]) -> usize {
        for (o, c) in out.iter_mut().zip(self.modes[..self.n].iter()) {
            let f = c.f / size;
            let q = (c.q * decay).min(PI * f * T60_LIMIT / 6.91);
            *o = Mode { f, q, g: c.gain(pos) * scale };
        }
        self.n
    }

    /// The skirt corners at `size` (Hz).
    pub fn skirt(&self, size: f64) -> (f64, f64) {
        (self.hp / size, self.lp / size)
    }
}

// ---- the build-time tables (README §2.7) -----------------------------------------------------------

/// The level table's grid: `position` 0 – 1 (65 linear), `decay` 25 – 400 % (5, octaves), `size`
/// 25 – 400 % (9, half octaves).
pub const DECAY_POINTS: usize = 5;
pub const SIZE_POINTS: usize = 9;
/// The rate the tables are generated at.
pub const TABLE_FS: f64 = 48000.0;

pub fn decay_at(i: usize) -> f64 {
    0.25 * 2f64.powi(i as i32)
}

pub fn size_at(i: usize) -> f64 {
    0.25 * 2f64.powf(0.5 * i as f64)
}

/// The analysis grid: 2,048 log points 20 Hz – 20 kHz (the prototype's), and its 100 Hz – 8 kHz part
/// (the pink weighting: equal weight per log step).
pub fn grid() -> Vec<f64> {
    (0..2048).map(|i| 20.0 * 1000f64.powf(i as f64 / 2047.0)).collect()
}

/// A 2nd-order Butterworth corner's complex response (high-pass or low-pass), bilinear at `fs`.
fn butter(freq: f64, fc: f64, fs: f64, high: bool) -> (f64, f64) {
    let w = (PI * freq / fs).tan() / (PI * fc.min(0.49 * fs) / fs).tan();
    // LP: 1 / (1 − w² + j√2 w); HP: −w² / (1 − w² + j√2 w)
    let (dr, di) = (1.0 - w * w, std::f64::consts::SQRT_2 * w);
    let d2 = dr * dr + di * di;
    let (nr, ni) = (dr / d2, -di / d2);
    if high {
        (-w * w * nr, -w * w * ni)
    } else {
        (nr, ni)
    }
}

/// The skirt's complex response: gain · HP(hp) · LP(lp).
pub fn skirt_response(freq: f64, hp: f64, lp: f64, gain: f64, fs: f64) -> (f64, f64) {
    let (ar, ai) = butter(freq, hp, fs, true);
    let (br, bi) = butter(freq, lp, fs, false);
    (gain * (ar * br - ai * bi), gain * (ar * bi + ai * br))
}

/// The complex responses of each mode (gain 1, edge-faded) and of the skirt on `freqs`, for one size
/// and decay: rows 0 .. n the modes, row n the skirt. The gains at a position are then a quadratic
/// form over these rows.
fn rows(o: &Object, size: f64, decay: f64, freqs: &[f64], fs: f64) -> (Vec<Vec<(f64, f64)>>, Vec<f64>) {
    let mut ms = [Mode::default(); MAX_MODES];
    let n = o.modes(0.5, size, decay, 1.0, &mut ms);
    let mut out = Vec::with_capacity(n + 1);
    let mut fade = Vec::with_capacity(n);
    for m in ms[..n].iter() {
        let e = gfx_dsp::modal::edge_fade(Mode { g: 1.0, ..*m }, fs);
        fade.push(e.g);
        out.push(freqs.iter().map(|&f| gfx_dsp::modal::response(&[Mode { g: 1.0, ..e }], f, fs)).collect());
    }
    let (hp, lp) = o.skirt(size);
    let s = 10f64.powf(o.skirt_db / 20.0);
    out.push(freqs.iter().map(|&f| skirt_response(f, hp, lp, s, fs)).collect());
    (out, fade)
}

/// The mode gains at `pos` (edge fades in), then 1 for the skirt.
fn weights(o: &Object, pos: f64, fade: &[f64]) -> Vec<f64> {
    let mut w: Vec<f64> = o.modes[..o.n].iter().zip(fade).map(|(c, f)| c.gain(pos) * f).collect();
    w.push(1.0);
    w
}

/// The level tables of one object: for each `position` (65) × `decay` (5) × `size` (9), the gain
/// (dB) that brings its pink-weighted power over 100 Hz – 8 kHz to 0 dB; and for each `decay` ×
/// `size`, its largest peak (dB) after that normalisation, over nine positions.
pub fn level_tables(obj: usize) -> (Vec<f64>, Vec<f64>) {
    let o = Object::new(obj);
    let fs = TABLE_FS;
    let g = grid();
    let band: Vec<f64> = g.iter().copied().filter(|&f| (100.0..=8000.0).contains(&f)).collect();
    let mut level = vec![0.0; POS_POINTS * DECAY_POINTS * SIZE_POINTS];
    let mut peak = vec![0.0; DECAY_POINTS * SIZE_POINTS];
    if obj == 0 {
        return (level, peak);
    }
    // the peak search: 512 log points and every mode's own frequency
    let coarse: Vec<f64> = (0..512).map(|i| 20.0 * 1000f64.powf(i as f64 / 511.0)).collect();
    for di in 0..DECAY_POINTS {
        for si in 0..SIZE_POINTS {
            let (decay, size) = (decay_at(di), size_at(si));
            let (r, fade) = rows(&o, size, decay, &band, fs);
            let n = r.len();
            // Gram matrix: mean over the band of Re(r_i conj r_j)
            let mut gm = vec![0.0; n * n];
            for i in 0..n {
                for j in i..n {
                    let s: f64 = r[i].iter().zip(&r[j]).map(|(a, b)| a.0 * b.0 + a.1 * b.1).sum::<f64>() / band.len() as f64;
                    gm[i * n + j] = s;
                    gm[j * n + i] = s;
                }
            }
            for pi in 0..POS_POINTS {
                let w = weights(&o, pi as f64 / (POS_POINTS - 1) as f64, &fade);
                let mut p = 0.0;
                for i in 0..n {
                    let mut s = 0.0;
                    for j in 0..n {
                        s += gm[i * n + j] * w[j];
                    }
                    p += w[i] * s;
                }
                level[(pi * DECAY_POINTS + di) * SIZE_POINTS + si] = -10.0 * p.max(1e-30).log10();
            }
            // the largest peak, normalised, over nine positions
            let mut ms = [Mode::default(); MAX_MODES];
            let nm = o.modes(0.5, size, decay, 1.0, &mut ms);
            let mut freqs = coarse.clone();
            freqs.extend(ms[..nm].iter().map(|m| m.f).filter(|&f| f < 20000.0));
            let (r2, fade2) = rows(&o, size, decay, &freqs, fs);
            let mut best = f64::NEG_INFINITY;
            for k in 0..9 {
                let pos = k as f64 / 8.0;
                let w = weights(&o, pos, &fade2);
                let kk = level_at(&level, pos, decay, size);
                for fi in 0..freqs.len() {
                    let (mut re, mut im) = (0.0, 0.0);
                    for (row, wi) in r2.iter().zip(&w) {
                        re += wi * row[fi].0;
                        im += wi * row[fi].1;
                    }
                    best = best.max(10.0 * (re * re + im * im).max(1e-30).log10() + kk);
                }
            }
            peak[di * SIZE_POINTS + si] = best;
        }
    }
    (level, peak)
}

/// Trilinear read of a level table (position linear, decay and size in log2).
pub fn level_at<T: Copy + Into<f64>>(t: &[T], pos: f64, decay: f64, size: f64) -> f64 {
    let p = pos.clamp(0.0, 1.0) * (POS_POINTS - 1) as f64;
    let d = ((decay / 0.25).max(1e-9).log2()).clamp(0.0, (DECAY_POINTS - 1) as f64);
    let s = (2.0 * (size / 0.25).max(1e-9).log2()).clamp(0.0, (SIZE_POINTS - 1) as f64);
    let (p0, d0, s0) = ((p as usize).min(POS_POINTS - 2), (d as usize).min(DECAY_POINTS - 2), (s as usize).min(SIZE_POINTS - 2));
    let (fp, fd, fsz) = (p - p0 as f64, d - d0 as f64, s - s0 as f64);
    let at = |pi: usize, di: usize, si: usize| -> f64 { t[(pi * DECAY_POINTS + di) * SIZE_POINTS + si].into() };
    let mut v = 0.0;
    for (pi, wp) in [(p0, 1.0 - fp), (p0 + 1, fp)] {
        for (di, wd) in [(d0, 1.0 - fd), (d0 + 1, fd)] {
            for (si, ws) in [(s0, 1.0 - fsz), (s0 + 1, fsz)] {
                v += wp * wd * ws * at(pi, di, si);
            }
        }
    }
    v
}

/// Bilinear read of a peak table (decay and size in log2).
pub fn peak_at<T: Copy + Into<f64>>(t: &[T], decay: f64, size: f64) -> f64 {
    let d = ((decay / 0.25).max(1e-9).log2()).clamp(0.0, (DECAY_POINTS - 1) as f64);
    let s = (2.0 * (size / 0.25).max(1e-9).log2()).clamp(0.0, (SIZE_POINTS - 1) as f64);
    let (d0, s0) = ((d as usize).min(DECAY_POINTS - 2), (s as usize).min(SIZE_POINTS - 2));
    let (fd, fsz) = (d - d0 as f64, s - s0 as f64);
    let at = |di: usize, si: usize| -> f64 { t[di * SIZE_POINTS + si].into() };
    (1.0 - fd) * ((1.0 - fsz) * at(d0, s0) + fsz * at(d0, s0 + 1)) + fd * ((1.0 - fsz) * at(d0 + 1, s0) + fsz * at(d0 + 1, s0 + 1))
}

/// The pink-normalised object's level on a sung vowel at the defaults (`position` 50 %, `size` and
/// `decay` 100 %), dB re bypass: the power mean over 98 – 392 Hz in semitones, 'a' and 'o' (steady
/// state). The voice constant is its opposite.
pub fn voice_level(obj: usize, level: &[f64]) -> f64 {
    if obj == 0 {
        return 0.0;
    }
    let o = Object::new(obj);
    let fs = TABLE_FS;
    let k = 10f64.powf(level_at(level, 0.5, 1.0, 1.0) / 20.0);
    let mut ms = [Mode::default(); MAX_MODES];
    let n = o.modes(0.5, 1.0, 1.0, k, &mut ms);
    let faded: Vec<Mode> = ms[..n].iter().map(|m| gfx_dsp::modal::edge_fade(*m, fs)).collect();
    let (hp, lp) = o.skirt(1.0);
    let s = k * 10f64.powf(o.skirt_db / 20.0);
    let h2 = |f: f64| {
        let (a, b) = gfx_dsp::modal::response(&faded, f, fs);
        let (c, d) = skirt_response(f, hp, lp, s, fs);
        (a + c) * (a + c) + (b + d) * (b + d)
    };
    let mut acc = 0.0;
    let mut cnt = 0.0;
    for i in 0..25 {
        let f0 = 98.0 * 2f64.powf(i as f64 / 12.0);
        for v in [crate::voice::VOWEL_A, crate::voice::VOWEL_O] {
            acc += 10f64.powf(crate::voice::vowel_gain_db(f0, v, fs, h2) / 10.0);
            cnt += 1.0;
        }
    }
    10.0 * (acc / cnt).log10()
}

/// The pink-weighted power (dB re bypass, 100 Hz – 8 kHz on the analysis grid) of object `obj` at any
/// setting with its level table and voice constant applied, computed directly (tests: the table's
/// interpolation error off its points).
pub fn pink_power_db(obj: usize, level: &[f64], pos: f64, size: f64, decay: f64, fs: f64) -> f64 {
    let o = Object::new(obj);
    let k = 10f64.powf(level_at(level, pos, decay, size) / 20.0);
    let mut ms = [Mode::default(); MAX_MODES];
    let n = o.modes(pos, size, decay, k, &mut ms);
    let faded: Vec<Mode> = ms[..n].iter().map(|m| gfx_dsp::modal::edge_fade(*m, fs)).collect();
    let (hp, lp) = o.skirt(size);
    let s = k * 10f64.powf(o.skirt_db / 20.0);
    let band: Vec<f64> = grid().into_iter().filter(|&f| (100.0..=8000.0).contains(&f)).collect();
    let p = band
        .iter()
        .map(|&f| {
            let (a, b) = gfx_dsp::modal::response(&faded, f, fs);
            let (c, d) = skirt_response(f, hp, lp.min(0.45 * fs), s, fs);
            (a + c) * (a + c) + (b + d) * (b + d)
        })
        .sum::<f64>()
        / band.len() as f64;
    10.0 * p.log10()
}
