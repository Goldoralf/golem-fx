// SPDX-License-Identifier: GPL-3.0-or-later
//! The transport (README §2.3): the speed deviation ε(t), generated at the control rate as each
//! model's rotating parts (two passes, record and playback, each a [`Wobble`]) plus random motion
//! (drift, random wow, random flutter), snags, and the stop / spin-up machine; and the `wf_out` meter
//! (IEC 386 / AES6 weighting fitted to the README's table, RMS over 2 s).

use crate::models::{Comp, Model, WOBBLE_H2};
use gfx_dsp::lfo::Rng;
use gfx_dsp::tape::{BandNoise, Shelf1, Wobble};
use std::f64::consts::PI;

const MAX_COMPS: usize = 3;

/// A seeded process designed at its nominal frequencies, stepped at a variable clock (rate 1 =
/// nominal) with linear interpolation: `wow_rate` and the tape speed scale it without a redesign.
#[derive(Clone, Debug)]
struct Clocked {
    gen: BandNoise,
    prev: f64,
    cur: f64,
    ph: f64,
}

impl Clocked {
    fn new(seed: u64) -> Self {
        Self { gen: BandNoise::new(seed), prev: 0.0, cur: 0.0, ph: 0.0 }
    }

    fn reset(&mut self) {
        self.gen.reset();
        self.prev = 0.0;
        self.cur = 0.0;
        self.ph = 0.0;
    }

    #[inline]
    fn next(&mut self, rate: f64) -> f64 {
        self.ph += rate.clamp(0.0, 8.0);
        while self.ph >= 1.0 {
            self.ph -= 1.0;
            self.prev = self.cur;
            self.cur = self.gen.next();
        }
        self.prev + (self.cur - self.prev) * self.ph
    }
}

/// A rotating part: its two passes and its frequency law.
#[derive(Clone, Debug)]
struct Part {
    w: [Wobble; 2],
    comp: Comp,
    /// share of the band's periodic amplitude (Σ = 1)
    share: f64,
    /// glide phase offset
    off: f64,
}

/// Random motion of one band, common and the right channel's own (`spread`).
#[derive(Clone, Debug)]
struct Random {
    a: [Clocked; 2],
    b: [Clocked; 2],
}

impl Random {
    fn reset(&mut self) {
        for c in self.a.iter_mut().chain(self.b.iter_mut()) {
            c.reset();
        }
    }
}

/// A snag: ε dips by `depth` over t1 (raised cosine), then overshoots for 3 t1 with equal area.
#[derive(Clone, Copy, Debug, Default)]
struct Snag {
    t: f64,
    t1: f64,
    depth: f64,
    on: bool,
}

impl Snag {
    #[inline]
    fn value(&self) -> f64 {
        if !self.on {
            return 0.0;
        }
        if self.t < self.t1 {
            -self.depth * (PI * self.t / self.t1).sin().powi(2)
        } else {
            let u = (self.t - self.t1) / (3.0 * self.t1);
            self.depth / 3.0 * (PI * u).sin().powi(2)
        }
    }
}

/// The knobs the generator reads each control tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct Knobs {
    /// peak deviations as fractions (`wow` / 100, `flutter` / 100)
    pub wow: f64,
    pub flutter: f64,
    /// random shares (0 … 1) after `wow_rand` and `wear`
    pub rho_wow: f64,
    pub rho_flutter: f64,
    /// `wow_rate` / 100
    pub wow_rate: f64,
    /// 0 … 1
    pub spread: f64,
    /// snags per second
    pub snag_rate: f64,
}

/// n(ρ): the periodic and random parts are independent, their peaks add in quadrature.
#[inline]
fn mix_norm(rho: f64) -> f64 {
    ((1.0 - rho).powi(2) + rho * rho).sqrt().max(1e-9)
}

#[derive(Clone, Debug)]
pub struct Transport {
    fc: f64,
    /// speed ratio v / v_model (tape), 1 on discs
    rv: f64,
    wow: [Option<Part>; MAX_COMPS],
    flutter: [Option<Part>; MAX_COMPS],
    /// drift (OU, 0.05 Hz) + random wow (0.2 – 3 Hz), random flutter (40 – 200 Hz)
    drift: Random,
    rwow: Random,
    rflut: Random,
    snag: Snag,
    rng: Rng,
    /// glide clock (s)
    time: f64,
    weight: Weighting,
}

impl Default for Transport {
    fn default() -> Self {
        Self::new()
    }
}

fn random(seed: u64) -> Random {
    Random { a: [Clocked::new(seed), Clocked::new(seed + 1)], b: [Clocked::new(seed + 2), Clocked::new(seed + 3)] }
}

impl Transport {
    pub fn new() -> Self {
        Self {
            fc: 3000.0,
            rv: 1.0,
            wow: [None, None, None],
            flutter: [None, None, None],
            drift: random(101),
            rwow: random(111),
            rflut: random(121),
            snag: Snag::default(),
            rng: Rng::new(131),
            time: 0.0,
            weight: Weighting::new(),
        }
    }

    /// Control rate `fc` (Hz). Designs the random processes at their nominal frequencies.
    pub fn prepare(&mut self, fc: f64) {
        self.fc = fc;
        for c in self.drift.a.iter_mut().chain(self.drift.b.iter_mut()) {
            c.gen.design(0.0, 0.05, fc);
        }
        for c in self.rwow.a.iter_mut().chain(self.rwow.b.iter_mut()) {
            c.gen.design(0.2, 3.0, fc);
        }
        for c in self.rflut.a.iter_mut().chain(self.rflut.b.iter_mut()) {
            c.gen.design(40.0, 200.0, fc);
        }
        self.weight.prepare(fc);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.drift.reset();
        self.rwow.reset();
        self.rflut.reset();
        self.snag = Snag::default();
        self.rng.reset();
        self.time = 0.0;
        for p in self.wow.iter_mut().chain(self.flutter.iter_mut()).flatten() {
            p.w[0].reset();
            p.w[1].reset();
        }
        self.weight.reset();
    }

    /// The model's rotating parts at speed ratio `rv` (a model change resets the parts).
    pub fn set_model(&mut self, m: &Model, rv: f64) {
        self.rv = rv;
        let build = |comps: &[Comp], seed: u64| {
            let total: f64 = comps.iter().map(|c| c.amp).sum::<f64>().max(1e-9);
            let mut out: [Option<Part>; MAX_COMPS] = [None, None, None];
            for (k, c) in comps.iter().take(MAX_COMPS).enumerate() {
                let s = seed + 10 * k as u64;
                out[k] = Some(Part { w: [Wobble::new(s, WOBBLE_H2), Wobble::new(s + 1, WOBBLE_H2)], comp: *c, share: c.amp / total, off: k as f64 * 0.37 });
            }
            out
        };
        self.wow = build(m.wow, 1000);
        self.flutter = build(m.flutter, 2000);
    }

    /// The parts' current frequency (Hz): reels glide over their range in about four minutes.
    fn part_hz(p: &Part, time: f64, scale: f64) -> f64 {
        let c = &p.comp;
        let f = if c.hi > c.lo {
            let t = 0.5 + 0.5 * (2.0 * PI * (time / 240.0 + p.off)).sin();
            c.lo * (c.hi / c.lo).powf(t)
        } else {
            c.f
        };
        f * scale
    }

    /// Peak excursion of the read head (s) the knobs can produce: Σ ε_k / 2π f_k for the periodic
    /// parts, 3σ / 2π f for the random ones, + 6 ms when snags can happen (README §2.3, D0).
    pub fn excursion_s(&self, k: &Knobs) -> f64 {
        let sw = self.rv * k.wow_rate;
        let sf = self.rv;
        let band = |parts: &[Option<Part>; MAX_COMPS], amt: f64, rho: f64, scale: f64, f_rand: f64| {
            let n = mix_norm(rho);
            let per: f64 = parts.iter().flatten().map(|p| p.share / (2.0 * PI * p.comp.lo.max(0.05) * scale)).sum();
            amt * ((1.0 - rho) / n * per + rho / n / (2.0 * PI * f_rand * scale.min(8.0)))
        };
        let mut e = band(&self.wow, k.wow, k.rho_wow, sw, 0.15) + band(&self.flutter, k.flutter, k.rho_flutter, sf, 40.0);
        if k.snag_rate > 0.0 {
            e += 0.006;
        }
        e
    }

    /// One control tick: ε for the left and right channels (fraction of speed).
    pub fn tick(&mut self, k: &Knobs) -> [f64; 2] {
        let dt = 1.0 / self.fc;
        self.time += dt;
        let (sw, sf) = (self.rv * k.wow_rate, self.rv);
        let periodic = |parts: &mut [Option<Part>; MAX_COMPS], scale: f64, bw: f64, time: f64, fc: f64| {
            let mut s = 0.0;
            for p in parts.iter_mut().flatten() {
                let hz = Self::part_hz(p, time, scale);
                let mut v = 0.0;
                for w in p.w.iter_mut() {
                    w.set(hz, bw, fc);
                    v += 0.5 * w.next();
                }
                s += p.share * v;
            }
            s
        };
        let pw = periodic(&mut self.wow, sw, 0.1, self.time, self.fc);
        let pf = periodic(&mut self.flutter, sf, 0.5, self.time, self.fc);
        let a = 1.0 - k.spread;
        let b = (1.0 - a * a).max(0.0).sqrt();
        let rand2 = |r: &mut Random, rate: f64| -> [f64; 2] {
            let c = (r.a[0].next(rate) + r.a[1].next(rate)) * std::f64::consts::FRAC_1_SQRT_2;
            if b > 0.0 {
                let o = (r.b[0].next(rate) + r.b[1].next(rate)) * std::f64::consts::FRAC_1_SQRT_2;
                [c, a * c + b * o]
            } else {
                [c, c]
            }
        };
        let d = rand2(&mut self.drift, sw);
        let rw = rand2(&mut self.rwow, sw);
        let rf = rand2(&mut self.rflut, sf);
        // snags (shared: one tape)
        if self.snag.on {
            self.snag.t += dt;
            if self.snag.t >= 4.0 * self.snag.t1 {
                self.snag.on = false;
            }
        } else if k.snag_rate > 0.0 && self.rng.next_f64() < k.snag_rate * dt {
            self.snag = Snag { t: 0.0, t1: 0.010 + 0.030 * self.rng.next_f64(), depth: 0.1 + 0.2 * self.rng.next_f64(), on: true };
        }
        let sn = self.snag.value();
        let (nw, nf) = (mix_norm(k.rho_wow), mix_norm(k.rho_flutter));
        let mut out = [0.0; 2];
        for ch in 0..2 {
            // random wow: drift and band noise in equal power, unit σ; "peak" = 3σ
            let rwv = (d[ch] + rw[ch]) * std::f64::consts::FRAC_1_SQRT_2 / 3.0;
            let ew = k.wow * ((1.0 - k.rho_wow) * pw + k.rho_wow * rwv) / nw;
            let ef = k.flutter * ((1.0 - k.rho_flutter) * pf + k.rho_flutter * rf[ch] / 3.0) / nf;
            out[ch] = ew + ef + sn;
        }
        self.weight.push(out[0]);
        out
    }

    /// The weighted W&F of the left channel's ε (%, WRMS over 2 s).
    pub fn wf_percent(&self) -> f64 {
        self.weight.wrms() * 100.0
    }
}

/// IEC 386 / AES6 weighting [U: fitted to the README's table, recalled by the reviewer — check
/// against AES6-2008 before the meter is trusted]: a 2nd-order high-pass (0.446 Hz, Q 0.668), a
/// first-order high-pass (1.453 Hz), a first-order low-pass (9.62 Hz), a shelf (zero 45.5 Hz, pole
/// 60.2 Hz), gain 1.133: within 0.3 dB of the table from 0.2 to 200 Hz. Then RMS over 2 s.
#[derive(Clone, Debug)]
pub struct Weighting {
    fc: f64,
    // 2nd-order TPT high-pass
    a1: f64,
    a2: f64,
    a3: f64,
    k: f64,
    ic1: f64,
    ic2: f64,
    hp1: Shelf1,
    lp1: Shelf1,
    shelf: Shelf1,
    ring: Vec<f64>,
    pos: usize,
    sum: f64,
}

pub const WEIGHT_FIT: [f64; 7] = [0.446_083, 0.667_756, 1.453_266, 9.619_071, 45.517_72, 60.199_53, 1.132_886];

/// The README's weighting table (Hz, dB) [U].
pub const WEIGHT_TABLE: [(f64, f64); 16] = [
    (0.2, -30.6),
    (0.315, -19.7),
    (0.4, -15.0),
    (0.63, -8.4),
    (0.8, -6.0),
    (1.0, -4.2),
    (1.6, -1.8),
    (2.0, -0.9),
    (4.0, 0.0),
    (6.3, -0.9),
    (10.0, -2.1),
    (20.0, -5.9),
    (40.0, -10.4),
    (63.0, -14.2),
    (100.0, -17.3),
    (200.0, -23.0),
];

impl Weighting {
    pub fn new() -> Self {
        Self { fc: 3000.0, a1: 0.0, a2: 0.0, a3: 0.0, k: 1.0, ic1: 0.0, ic2: 0.0, hp1: Shelf1::identity(), lp1: Shelf1::identity(), shelf: Shelf1::identity(), ring: Vec::new(), pos: 0, sum: 0.0 }
    }

    pub fn prepare(&mut self, fc: f64) {
        self.fc = fc;
        let [fa, qa, fb, flp, z, p, _] = WEIGHT_FIT;
        let g = (PI * fa / fc).tan();
        self.k = 1.0 / qa;
        self.a1 = 1.0 / (1.0 + g * (g + self.k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
        // a first-order high-pass is a shelf with its zero at 0 (k = 0 on the low-pass part): built
        // here as a shelf from ~0 to 1 (zero at fb / 1e6)
        self.hp1 = Shelf1::design(fb * 1e-6, fb, fc);
        self.lp1 = Shelf1::design(flp * 1e6, flp, fc);
        self.shelf = Shelf1::design(z, p, fc);
        self.ring = vec![0.0; (2.0 * fc).round().max(1.0) as usize];
        self.reset();
    }

    pub fn reset(&mut self) {
        self.ic1 = 0.0;
        self.ic2 = 0.0;
        self.hp1.reset();
        self.lp1.reset();
        self.shelf.reset();
        self.ring.iter_mut().for_each(|v| *v = 0.0);
        self.pos = 0;
        self.sum = 0.0;
    }

    /// The weighting filter alone (one sample).
    #[inline]
    pub fn filter(&mut self, x: f64) -> f64 {
        let v3 = x - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        let hp = x - self.k * v1 - v2;
        // the high-pass shelf runs from 1e-6 to 1: scale its HF asymptote back to 1
        let y = self.hp1.process(hp) * 1e-6;
        let y = self.lp1.process(y);
        WEIGHT_FIT[6] * self.shelf.process(y)
    }

    #[inline]
    pub fn push(&mut self, x: f64) {
        if self.ring.is_empty() {
            return;
        }
        let y = self.filter(x);
        let e = y * y;
        self.sum += e - self.ring[self.pos];
        self.ring[self.pos] = e;
        self.pos += 1;
        if self.pos == self.ring.len() {
            self.pos = 0;
            // no drift of the running sum
            self.sum = self.ring.iter().sum();
        }
    }

    pub fn wrms(&self) -> f64 {
        if self.ring.is_empty() {
            return 0.0;
        }
        (self.sum.max(0.0) / self.ring.len() as f64).sqrt()
    }
}

impl Default for Weighting {
    fn default() -> Self {
        Self::new()
    }
}

/// The stop / spin-up machine (README §2.3): speed s, playback amplitude a (∝ speed: the head reads
/// dΦ/dt), and the lag the read head has accumulated (samples).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stage {
    Run,
    Stopping,
    Stopped,
    /// released during a stop: fade to silence (10 ms), then spin up
    Release,
    SpinUp,
    /// 30 ms crossfade from the lagging head to the D0 head
    Catch,
}

#[derive(Clone, Copy, Debug)]
pub struct Stop {
    pub stage: Stage,
    t: f64,
    len: f64,
    s0: f64,
    a0: f64,
    pub speed: f64,
    pub amp: f64,
    pub lag: f64,
    /// crossfade position in Catch (0 → 1)
    pub x: f64,
    tau: f64,
    tail: f64,
}

impl Default for Stop {
    fn default() -> Self {
        Self { stage: Stage::Run, t: 0.0, len: 1.0, s0: 1.0, a0: 1.0, speed: 1.0, amp: 1.0, lag: 0.0, x: 0.0, tau: 1.0, tail: 0.0 }
    }
}

impl Stop {
    /// Settled at `stopped` (reset with `stop` on starts silent).
    pub fn settle(&mut self, stopped: bool) {
        *self = Self::default();
        if stopped {
            self.stage = Stage::Stopped;
            self.speed = 0.0;
            self.amp = 0.0;
        }
    }

    pub fn idle(&self) -> bool {
        self.stage == Stage::Run
    }

    fn start_stop(&mut self, time_s: f64) {
        self.stage = Stage::Stopping;
        self.t = 0.0;
        self.len = time_s.max(0.01);
        self.s0 = self.speed;
        self.a0 = self.amp;
        self.tau = self.len / 2.5;
        self.tail = (-self.len / self.tau).exp();
        if self.x > 0.0 {
            // stopped during the catch-up: keep the head where the crossfade had it
            self.lag *= 1.0 - self.x;
            self.x = 0.0;
        }
    }

    /// One sample. `stop` the switch, `time_s` = `stop_time`.
    #[inline]
    pub fn tick(&mut self, stop: bool, time_s: f64, dt: f64) {
        match self.stage {
            Stage::Run => {
                if stop {
                    self.start_stop(time_s);
                }
            }
            Stage::Stopping => {
                if !stop {
                    self.stage = Stage::Release;
                    self.t = 0.0;
                    self.a0 = self.amp;
                }
            }
            Stage::Stopped => {
                if !stop {
                    self.stage = Stage::SpinUp;
                    self.t = 0.0;
                    self.len = time_s.max(0.01) * 0.25;
                    self.lag = 0.0;
                }
            }
            Stage::Release => {}
            Stage::SpinUp | Stage::Catch => {
                if stop {
                    self.start_stop(time_s);
                }
            }
        }
        self.t += dt;
        match self.stage {
            Stage::Run => {}
            Stage::Stopping | Stage::Release => {
                let u = (self.t.min(self.len) / self.len).min(1.0);
                let curve = ((-self.t.min(self.len) / self.tau).exp() - u * self.tail).max(0.0);
                if self.stage == Stage::Stopping {
                    self.speed = self.s0 * curve;
                    self.amp = self.a0 * curve;
                    if self.t >= self.len {
                        self.stage = Stage::Stopped;
                        self.speed = 0.0;
                        self.amp = 0.0;
                    }
                } else {
                    self.speed = self.s0 * curve;
                    let f = (1.0 - self.t / 0.010).max(0.0);
                    self.amp = self.a0 * f;
                    if f <= 0.0 {
                        self.stage = Stage::SpinUp;
                        self.t = 0.0;
                        self.len = time_s.max(0.01) * 0.25;
                        self.lag = 0.0;
                    }
                }
                self.lag += 1.0 - self.speed;
            }
            Stage::Stopped => {
                self.speed = 0.0;
                self.amp = 0.0;
            }
            Stage::SpinUp => {
                let u = (self.t / self.len).min(1.0);
                self.speed = 0.3 + 0.7 * u;
                self.amp = u;
                self.lag += 1.0 - self.speed;
                if u >= 1.0 {
                    self.stage = Stage::Catch;
                    self.t = 0.0;
                    self.x = 0.0;
                }
            }
            Stage::Catch => {
                self.speed = 1.0;
                self.amp = 1.0;
                self.x = (self.t / 0.030).min(1.0);
                if self.x >= 1.0 {
                    self.stage = Stage::Run;
                    self.lag = 0.0;
                    self.x = 0.0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighting_fits_table() {
        let fc = 3000.0;
        for &(f, db) in WEIGHT_TABLE.iter() {
            let mut w = Weighting::new();
            w.prepare(fc);
            let n = (fc * (20.0 / f).max(4.0)) as usize;
            let mut e = 0.0;
            for i in 0..2 * n {
                let y = w.filter((2.0 * PI * f * i as f64 / fc).sin());
                if i >= n {
                    e += y * y;
                }
            }
            let g = 10.0 * (2.0 * e / n as f64).log10();
            assert!((g - db).abs() < 0.5, "{f} Hz: {g:.2} vs {db}");
        }
    }

    #[test]
    fn stop_reaches_zero_on_time() {
        let mut s = Stop::default();
        let fs = 48000.0;
        let mut n = 0;
        while s.stage != Stage::Stopped {
            s.tick(true, 0.6, 1.0 / fs);
            n += 1;
            assert!(n < 100_000);
        }
        assert!(((n as f64 / fs) - 0.6).abs() < 0.03, "{}", n as f64 / fs);
        // released: spin-up a quarter of it, then the catch, then run
        let mut n = 0;
        while s.stage != Stage::Run {
            s.tick(false, 0.6, 1.0 / fs);
            n += 1;
        }
        assert!(((n as f64 / fs) - 0.18).abs() < 0.002, "{}", n as f64 / fs);
    }
}
