// SPDX-License-Identifier: GPL-3.0-or-later
//! The slot effects, on one frame's output field (magnitudes, phases, frequencies), within a region.
//!
//! Rules (README §3): levels in dB relative to the frame's peak; level-dependent gains smoothed over
//! 20 ms; rates per second; counts as fractions of the region's bins; at amount 0 every effect is
//! neutral and 1 % is close to it. The Move effects (18 – 20) are applied by the engine's frequency map.

use crate::engine::{wrap, Ctx, Rand, SlotSet};
use std::f32::consts::PI;

pub const EFFECTS: [&str; 30] = [
    "Off",
    "Low Slope",
    "High Slope",
    "Band Keep",
    "Band Cut",
    "Band Lift",
    "Tilt",
    "Quiet Cut",
    "Loud Cut",
    "Sharpen",
    "Thin",
    "Peak Only",
    "Hold",
    "Scatter Hold",
    "Stutter Hold",
    "Attack Hold",
    "Ring Hold",
    "Haze Hold",
    "Spread Region",
    "Squeeze Region",
    "Slide Region",
    "Blur",
    "Crush",
    "Corrupt",
    "Sparkle",
    "Bin Clip",
    "Flip",
    "Phase Swap",
    "Clump",
    "Shuffle",
];

const MAX_PAIRS: usize = 4096;
const MAX_CLUMPS: usize = 32;

#[derive(Clone, Copy, Default)]
struct Clump {
    at: u32,
    span: u32,
    life: f32,
}

/// One slot's memory (per channel).
pub struct SlotState {
    pub fx: u8,
    hm: Vec<f32>,
    hp: Vec<f32>,
    hw: Vec<f32>,
    timer: Vec<f32>,
    gs: Vec<f32>,
    aux: Vec<f32>,
    pairs: Vec<(u32, u32)>,
    clumps: [Clump; MAX_CLUMPS],
    /// region-wide timer (stutter, attack), refresh timer (sparkle, shuffle)
    rt: f32,
    prev_e: f32,
}

impl SlotState {
    pub fn new() -> Self {
        use crate::engine::MAX_BINS;
        Self {
            fx: 0,
            hm: vec![0.0; MAX_BINS],
            hp: vec![0.0; MAX_BINS],
            hw: vec![0.0; MAX_BINS],
            timer: vec![0.0; MAX_BINS],
            gs: vec![1.0; MAX_BINS],
            aux: vec![0.0; MAX_BINS],
            pairs: Vec::with_capacity(MAX_PAIRS),
            clumps: [Clump::default(); MAX_CLUMPS],
            rt: 0.0,
            prev_e: 0.0,
        }
    }

    pub fn clear(&mut self) {
        self.fx = 0;
        self.hm.fill(0.0);
        self.hp.fill(0.0);
        self.hw.fill(0.0);
        self.timer.fill(0.0);
        self.gs.fill(1.0);
        self.aux.fill(0.0);
        self.pairs.clear();
        self.clumps = [Clump::default(); MAX_CLUMPS];
        self.rt = 0.0;
        self.prev_e = 0.0;
    }
}

impl Default for SlotState {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn db(x: f32) -> f32 {
    10f32.powf(x / 20.0)
}

#[inline]
fn level(m: f32, peak: f32) -> f32 {
    20.0 * (m / peak).max(1e-9).log10()
}

#[inline]
fn smoothstep(x: f32) -> f32 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The region's bins, from octaves above 20 Hz (at least one bin).
fn region(s: &SlotSet, ctx: &Ctx) -> (usize, usize) {
    let (lo, hi) = if s.lo <= s.hi { (s.lo, s.hi) } else { (s.hi, s.lo) };
    let n = (ctx.nb - 1) as f32 * 2.0;
    let bin = |o: f32| 20.0 * 2f32.powf(o) * n / ctx.sr;
    let kl = (bin(lo).ceil() as usize).clamp(1, ctx.nb - 1);
    let kh = (bin(hi).floor() as usize).clamp(1, ctx.nb - 1);
    (kl, kh.max(kl))
}

/// Max-hold with a per-bin decay factor: a bin louder than its decayed hold takes over (its phase and
/// frequency); otherwise the hold plays on, its phase advancing at the held frequency.
#[inline]
fn hold_bin(k: usize, a: f32, hop: f32, om: &mut [f32], oph: &mut [f32], ow: &mut [f32], st: &mut SlotState) {
    let held = st.hm[k] * a;
    if om[k] >= held {
        st.hm[k] = om[k];
        st.hp[k] = oph[k];
        st.hw[k] = ow[k];
    } else {
        st.hm[k] = if held > 1e-12 { held } else { 0.0 };
        st.hp[k] = wrap(st.hp[k] + st.hw[k] * hop);
        om[k] = st.hm[k];
        oph[k] = st.hp[k];
        ow[k] = st.hw[k];
    }
}

/// A bin frozen as captured: its phase keeps advancing.
#[inline]
fn play_held(k: usize, hop: f32, om: &mut [f32], oph: &mut [f32], ow: &mut [f32], st: &mut SlotState) {
    st.hp[k] = wrap(st.hp[k] + st.hw[k] * hop);
    om[k] = st.hm[k];
    oph[k] = st.hp[k];
    ow[k] = st.hw[k];
}

#[inline]
fn capture(k: usize, om: &[f32], oph: &[f32], ow: &[f32], st: &mut SlotState) {
    st.hm[k] = om[k];
    st.hp[k] = oph[k];
    st.hw[k] = ow[k];
}

/// T60 → per-frame factor.
#[inline]
fn decay(t60: f32, dt: f32) -> f32 {
    if t60 <= 1e-4 {
        0.0
    } else {
        (-6.907_755 * dt / t60).exp()
    }
}

#[allow(clippy::too_many_arguments)]
pub fn apply(s: SlotSet, ctx: &Ctx, om: &mut [f32], oph: &mut [f32], ow: &mut [f32], st: &mut SlotState, pre: &mut [f64], rand: &mut Rand) {
    if st.fx != s.fx {
        st.clear();
        st.fx = s.fx;
    }
    let nb = ctx.nb;
    let a = s.amt.clamp(0.0, 1.0);
    let (kl, kh) = region(&s, ctx);
    let (lo, hi) = if s.lo <= s.hi { (s.lo, s.hi) } else { (s.hi, s.lo) };
    let peak = ctx.peak;
    let dt = ctx.dt;
    let hop = ctx.hop;
    let gsm = 1.0 - (-dt / 0.02).exp();
    // a level-dependent gain, smoothed in time
    macro_rules! smooth_gain {
        ($k:expr, $g:expr) => {{
            let k = $k;
            st.gs[k] += gsm * ($g - st.gs[k]);
            om[k] *= st.gs[k];
        }};
    }
    // bins outside the region drop any hold (a moving region does not bring stale holds back)
    let clear_outside = |st: &mut SlotState| {
        for k in (0..kl).chain(kh + 1..nb) {
            st.hm[k] = 0.0;
            st.timer[k] = 0.0;
        }
    };
    match s.fx {
        // ---------------- Filter
        1 | 2 => {
            let span = (hi - lo).max(1e-3);
            for k in 1..nb {
                let o = ctx.oct[k];
                let t = if s.fx == 1 { (o - lo) / span } else { (hi - o) / span };
                om[k] *= db(-60.0 * a * t.clamp(0.0, 1.0));
            }
        }
        3 => {
            let g = db(-60.0 * a);
            for k in (0..kl).chain(kh + 1..nb) {
                om[k] *= g;
            }
        }
        4 | 5 => {
            let g = if s.fx == 4 { db(-60.0 * a) } else { db(18.0 * a) };
            for v in &mut om[kl..=kh] {
                *v *= g;
            }
        }
        6 => {
            let centre = 0.5 * (lo + hi);
            for k in 1..nb {
                let o = ctx.oct[k].clamp(lo, hi);
                om[k] *= db(6.0 * a * (o - centre));
            }
        }
        // ---------------- Tame
        7 => {
            let t = -60.0 + 54.0 * a;
            for k in kl..=kh {
                let g = smoothstep((level(om[k], peak) - t + 3.0) / 3.0);
                smooth_gain!(k, g);
            }
        }
        8 => {
            let t = -40.0 * a;
            for k in kl..=kh {
                let g = 1.0 - smoothstep((level(om[k], peak) - t) / 3.0);
                smooth_gain!(k, g);
            }
        }
        9 => {
            let e = 3.0 * a;
            for k in kl..=kh {
                let g = (om[k] / peak).max(1e-9).powf(e);
                smooth_gain!(k, g);
            }
        }
        10 => {
            let depth = 12.0 - 12.0 * a;
            let cut = db(-60.0 * a);
            for k in kl..=kh {
                let l = level(om[k], peak);
                let ln = level(om[k.saturating_sub(1)], peak).min(level(om[(k + 1).min(nb - 1)], peak));
                // how far under both neighbours, past the depth (3 dB knee)
                let u = smoothstep((ln - l - depth) / 3.0);
                let g = 1.0 + (cut - 1.0) * u;
                smooth_gain!(k, g);
            }
        }
        11 => {
            // the loudest peak's region in the slot's region is kept, the rest goes down
            let mut kp = kl;
            for k in kl..=kh {
                if om[k] > om[kp] {
                    kp = k;
                }
            }
            let mut a0 = kp;
            while a0 > kl && om[a0 - 1] <= om[a0] {
                a0 -= 1;
            }
            let mut b0 = kp;
            while b0 < kh && om[b0 + 1] <= om[b0] {
                b0 += 1;
            }
            let cut = db(-60.0 * a);
            for k in kl..=kh {
                let g = if k >= a0 && k <= b0 { 1.0 } else { cut };
                smooth_gain!(k, g);
            }
        }
        // ---------------- Hold
        12 => {
            let f = decay(10.0 * a, dt);
            clear_outside(st);
            for k in kl..=kh {
                hold_bin(k, f, hop, om, oph, ow, st);
            }
        }
        13 => {
            clear_outside(st);
            let p = 20.0 * a * dt;
            for k in kl..=kh {
                let r = rand.unit();
                let len = 0.05 + 0.45 * rand.unit();
                if st.timer[k] > 0.0 {
                    st.timer[k] -= dt;
                    play_held(k, hop, om, oph, ow, st);
                } else {
                    capture(k, om, oph, ow, st);
                    if r < p {
                        st.timer[k] = len;
                    }
                }
            }
        }
        14 | 15 => {
            clear_outside(st);
            let r = rand.unit();
            let len = rand.unit();
            if s.fx == 15 {
                let mut e = 0.0f32;
                for k in kl..=kh {
                    e += om[k] * om[k];
                }
                let rise = db(2.0 * (9.0 - 6.0 * a)); // power ratio
                let floor = peak * peak * 1e-6;
                if st.rt <= 0.0 && e > st.prev_e * rise && e > floor {
                    st.rt = 0.2 + 0.8 * a;
                    for k in kl..=kh {
                        capture(k, om, oph, ow, st);
                    }
                    st.prev_e = e;
                    return;
                }
                st.prev_e = e;
            }
            if st.rt > 0.0 {
                st.rt -= dt;
                for k in kl..=kh {
                    play_held(k, hop, om, oph, ow, st);
                }
            } else {
                for k in kl..=kh {
                    capture(k, om, oph, ow, st);
                }
                if s.fx == 14 && r < 30.0 * a * dt {
                    st.rt = 0.03 + 0.22 * len;
                }
            }
        }
        16 => {
            // each bin's T60 grows with its held level relative to the loudest hold
            clear_outside(st);
            let mut hmax = 0.0f32;
            for k in kl..=kh {
                hmax = hmax.max(st.hm[k]).max(om[k]);
            }
            let hmax = hmax.max(1e-20);
            for k in kl..=kh {
                let rel = (st.hm[k].max(om[k]) / hmax).sqrt();
                let f = decay(0.05 + 6.0 * a * rel, dt);
                hold_bin(k, f, hop, om, oph, ow, st);
            }
        }
        17 => {
            clear_outside(st);
            let f = decay(1.0 + 4.0 * a, dt);
            let step = 0.6 * dt.sqrt();
            for k in kl..=kh {
                hold_bin(k, f, hop, om, oph, ow, st);
                let w = (st.aux[k] + step * rand.bipolar()).clamp(-1.0, 1.0);
                st.aux[k] = w;
                om[k] *= db(12.0 * a * w);
            }
        }
        // ---------------- Texture
        21 => {
            let r = 2f32.powf(300.0 * a / 1200.0) - 1.0;
            pre[0] = 0.0;
            for k in 0..nb {
                pre[k + 1] = pre[k] + om[k] as f64;
            }
            // prefix sum read at a fractional position
            let at = |x: f32, pre: &[f64]| -> f64 {
                let x = x.clamp(0.0, nb as f32);
                let i = (x.floor() as usize).min(nb - 1);
                let f = (x - i as f32) as f64;
                pre[i] + (pre[i + 1] - pre[i]) * f
            };
            for k in kl..=kh {
                let h = k as f32 * r;
                if h < 0.01 {
                    continue;
                }
                let (x0, x1) = (k as f32 + 0.5 - h, k as f32 + 0.5 + h);
                let x0c = x0.max(0.0);
                let x1c = x1.min(nb as f32);
                om[k] = ((at(x1c, pre) - at(x0c, pre)) / (x1c - x0c).max(1e-6) as f64) as f32;
            }
        }
        22 | 23 => {
            let q = 64.0 * (2.0f32 / 64.0).powf(a);
            let step = 60.0 / q;
            let qp = (64.0 * (4.0f32 / 64.0).powf(a)).round().max(4.0);
            let pstep = 2.0 * PI / qp;
            let off = 7.0 * a;
            for k in kl..=kh {
                let l = level(om[k], peak);
                if l < -60.0 {
                    om[k] = 0.0;
                    continue;
                }
                let idx = (l / step).round();
                let lq = if s.fx == 22 {
                    idx * step
                } else {
                    let sign = if (idx as i64) & 1 == 1 { 1.0 } else { -1.0 };
                    ((idx + sign * off) * step).min(0.0)
                };
                om[k] = peak * db(lq);
                let pi = (oph[k] / pstep).round();
                let pq = if s.fx == 22 {
                    pi
                } else {
                    let h = ((pi as i64).rem_euclid(qp as i64) * 7 + 3).rem_euclid(qp as i64) as f32;
                    pi + (a * h).round()
                };
                oph[k] = pq * pstep;
            }
        }
        24 => {
            if st.rt <= 0.0 {
                for k in kl..=kh {
                    st.aux[k] = rand.bipolar();
                }
                st.rt = 0.02;
            }
            st.rt -= dt;
            for k in kl..=kh {
                om[k] *= db(12.0 * a * st.aux[k]);
            }
        }
        25 => {
            let ceil = peak * db(-30.0 * a);
            let knee = ceil * 0.5;
            let mut p0 = 0.0f64;
            let mut p1 = 0.0f64;
            for k in kl..=kh {
                let m = om[k];
                p0 += (m * m) as f64;
                let y = if m <= knee { m } else { knee + (ceil - knee) * ((m - knee) / (ceil - knee)).tanh() };
                om[k] = y;
                p1 += (y * y) as f64;
            }
            if p1 > 0.0 {
                let g = ((p0 / p1).sqrt() as f32).min(16.0);
                for v in &mut om[kl..=kh] {
                    *v *= g;
                }
            }
        }
        26 | 27 => {
            let w = (kh - kl) as f32;
            let edge = a * w * 0.5;
            let mut k = kl;
            while k < kh - (k - kl) {
                let m = kl + kh - k;
                if ((k - kl) as f32) < edge {
                    oph.swap(k, m);
                    if s.fx == 26 {
                        om.swap(k, m);
                        ow.swap(k, m);
                    }
                }
                k += 1;
            }
        }
        28 => {
            if rand.unit() < 30.0 * a * dt {
                let at = kl + (rand.unit() * (kh - kl + 1) as f32) as usize;
                if let Some(c) = st.clumps.iter_mut().find(|c| c.life <= 0.0) {
                    *c = Clump { at: at.min(kh) as u32, span: (1.0 + 15.0 * a).round() as u32, life: 0.1 };
                }
            } else {
                let _ = rand.unit();
            }
            for c in st.clumps.iter_mut() {
                if c.life <= 0.0 {
                    continue;
                }
                c.life -= dt;
                let at = c.at as usize;
                if at > kh {
                    continue;
                }
                for j in 1..=c.span as usize {
                    let d = at + j;
                    if d > kh {
                        break;
                    }
                    om[d] = om[at];
                    oph[d] = oph[at];
                    ow[d] = ow[at];
                }
            }
        }
        29 => {
            if st.rt <= 0.0 {
                st.pairs.clear();
                let nbins = kh - kl + 1;
                let count = ((0.1 * a * nbins as f32) as usize).min(MAX_PAIRS);
                for _ in 0..count {
                    let x = kl + ((rand.unit() * nbins as f32) as usize).min(nbins - 1);
                    let y = kl + ((rand.unit() * nbins as f32) as usize).min(nbins - 1);
                    st.pairs.push((x as u32, y as u32));
                }
                st.rt = 0.02;
            }
            st.rt -= dt;
            for &(x, y) in &st.pairs {
                let (x, y) = (x as usize, y as usize);
                if x <= kh && y <= kh {
                    om.swap(x, y);
                    oph.swap(x, y);
                    ow.swap(x, y);
                }
            }
        }
        _ => {}
    }
}
