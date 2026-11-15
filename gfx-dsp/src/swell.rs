// SPDX-License-Identifier: GPL-3.0-or-later
//! The swell's gain engine (design: gfx-swell/README.md §2.5): a normalised gain u ∈ [0, 1] that the
//! caller maps to g = s₀ + (1 − s₀) · u (s₀ = 1 − depth, so `depth` can move while a swell plays).
//!
//! - **States**: Closed / Spent (u = 0) → Rise → Hold (u = 1) → [Decay] → Spent; Release (a reset)
//!   → Closed.
//! - **Curves**: Square t², Linear, S-Curve, dB (48 dB), Pedal (an A-taper pot), each with its inverse,
//!   so a retrigger continues the rise from where the gain is: nothing jumps.
//! - **Retrigger**: a raised-cosine dip to u · (1 − dip), then the rise from c⁻¹ of that, the remaining
//!   time proportional. A dip can be scheduled `delay` samples ahead (lookahead placement).
//! - Times are latched at each trigger (an EXP move sets the next swell). f64 throughout.

/// Rise law (enum order, final: append only).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Curve {
    Square,
    Linear,
    SCurve,
    Db,
    Pedal,
}

const DB_A: f64 = 0.003_981_071_705_534_973; // 10^−2.4

impl Curve {
    pub const ALL: [Curve; 5] = [Curve::Square, Curve::Linear, Curve::SCurve, Curve::Db, Curve::Pedal];

    pub fn from_index(i: usize) -> Curve {
        *Self::ALL.get(i).unwrap_or(&Curve::Square)
    }

    /// c(t), t ∈ [0, 1] → [0, 1].
    #[inline]
    pub fn c(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Curve::Square => t * t,
            Curve::Linear => t,
            Curve::SCurve => 0.5 - 0.5 * (std::f64::consts::PI * t).cos(),
            Curve::Db => (10f64.powf(-2.4 * (1.0 - t)) - DB_A) / (1.0 - DB_A),
            Curve::Pedal => (32f64.powf(t) - 1.0) / 31.0,
        }
    }

    /// c⁻¹(u).
    #[inline]
    pub fn inv(self, u: f64) -> f64 {
        let u = u.clamp(0.0, 1.0);
        match self {
            Curve::Square => u.sqrt(),
            Curve::Linear => u,
            Curve::SCurve => (1.0 - 2.0 * u).clamp(-1.0, 1.0).acos() / std::f64::consts::PI,
            Curve::Db => (1.0 + (u * (1.0 - DB_A) + DB_A).log10() / 2.4).clamp(0.0, 1.0),
            Curve::Pedal => (31.0 * u + 1.0).ln() / 32f64.ln(),
        }
    }
}

/// What the meters report (`state`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle = 0,
    Rise = 1,
    Hold = 2,
    Decay = 3,
}

/// A swell's shape, latched at its trigger (samples).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub curve: Curve,
    pub rise: f64,
    /// 0 = no decay (hold)
    pub decay: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Seg {
    Closed,
    Spent,
    Rise { t: f64 },
    Hold,
    Decay { t: f64 },
    Release { t: f64, dt: f64 },
    Dip { from: f64, to: f64, k: f64, len: f64 },
}

/// A trigger waiting for its scheduled time.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pending {
    wait: u32,
    dip: f64,
    dip_len: f64,
    shape: Shape,
}

#[derive(Clone, Debug)]
pub struct Engine {
    seg: Seg,
    u: f64,
    shape: Shape,
    pending: Option<Pending>,
    curve: Curve,
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            seg: Seg::Closed,
            u: 0.0,
            shape: Shape { curve: Curve::Square, rise: 1.0, decay: 0.0 },
            pending: None,
            curve: Curve::Square,
        }
    }
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Closed, u = 0, nothing pending.
    pub fn reset(&mut self) {
        self.seg = Seg::Closed;
        self.u = 0.0;
        self.pending = None;
    }

    /// The normalised gain.
    pub fn u(&self) -> f64 {
        self.u
    }

    /// Closed: the level gate may open a swell (a releasing note counts as closed).
    pub fn is_closed(&self) -> bool {
        matches!(self.seg, Seg::Closed | Seg::Release { .. }) && self.pending.is_none()
    }

    /// Spent: a bow has ended while the note rings (only detected onsets restart it).
    pub fn is_spent(&self) -> bool {
        self.seg == Seg::Spent
    }

    /// Rise, Hold or Decay (a dip counts as a rise): the note may be reset.
    pub fn is_playing(&self) -> bool {
        matches!(self.seg, Seg::Rise { .. } | Seg::Hold | Seg::Decay { .. } | Seg::Dip { .. }) || self.pending.is_some()
    }

    pub fn state(&self) -> State {
        match self.seg {
            Seg::Rise { .. } | Seg::Dip { .. } => State::Rise,
            Seg::Hold => State::Hold,
            Seg::Decay { .. } => State::Decay,
            _ => State::Idle,
        }
    }

    /// A new swell. Over a ringing note (u > 0, not releasing) it dips to u · (1 − `dip`) over
    /// `dip_len` samples first; `dip` 0 then ignores it. `delay` schedules the dip (or, from Closed, the
    /// rise) that many samples ahead.
    pub fn trigger(&mut self, shape: Shape, dip: f64, dip_len: f64, delay: u32) {
        let ringing = matches!(self.seg, Seg::Rise { .. } | Seg::Hold | Seg::Decay { .. } | Seg::Dip { .. });
        if ringing && dip <= 0.0 {
            return;
        }
        let p = Pending { wait: delay, dip: dip.clamp(0.0, 1.0), dip_len: dip_len.max(1.0), shape };
        if delay == 0 {
            self.start(p);
        } else {
            self.pending = Some(p);
        }
    }

    fn start(&mut self, p: Pending) {
        self.shape = p.shape;
        self.curve = p.shape.curve;
        match self.seg {
            Seg::Closed | Seg::Spent => self.seg = Seg::Rise { t: 0.0 },
            Seg::Release { .. } => self.seg = Seg::Rise { t: self.curve.inv(self.u) },
            _ => {
                let to = self.u * (1.0 - p.dip);
                self.seg = Seg::Dip { from: self.u, to, k: 0.0, len: p.dip_len };
            }
        }
    }

    /// The note has reset: back to u = 0 over `release` samples (the curve played backwards), then
    /// Closed. Spent becomes Closed at once.
    pub fn release(&mut self, release: f64) {
        self.pending = None;
        match self.seg {
            Seg::Spent => self.seg = Seg::Closed,
            Seg::Rise { .. } | Seg::Hold | Seg::Decay { .. } | Seg::Dip { .. } => {
                if self.u <= 0.0 {
                    self.seg = Seg::Closed;
                } else {
                    self.seg = Seg::Release { t: self.curve.inv(self.u), dt: 1.0 / release.max(1.0) };
                }
            }
            _ => {}
        }
    }

    /// One sample: the normalised gain.
    #[inline]
    pub fn next(&mut self) -> f64 {
        if let Some(p) = self.pending.as_mut() {
            if p.wait <= 1 {
                let p = *p;
                self.pending = None;
                self.start(p);
            } else {
                p.wait -= 1;
            }
        }
        match self.seg {
            Seg::Closed | Seg::Spent => self.u = 0.0,
            Seg::Hold => self.u = 1.0,
            Seg::Rise { t } => {
                let t = t + 1.0 / self.shape.rise.max(1.0);
                if t >= 1.0 {
                    self.u = 1.0;
                    self.seg = if self.shape.decay > 0.0 { Seg::Decay { t: 1.0 } } else { Seg::Hold };
                } else {
                    self.u = self.curve.c(t);
                    self.seg = Seg::Rise { t };
                }
            }
            Seg::Decay { t } => {
                let t = t - 1.0 / self.shape.decay.max(1.0);
                if t <= 0.0 {
                    self.u = 0.0;
                    self.seg = Seg::Spent;
                } else {
                    self.u = self.curve.c(t);
                    self.seg = Seg::Decay { t };
                }
            }
            Seg::Release { t, dt } => {
                let t = t - dt;
                if t <= 0.0 {
                    self.u = 0.0;
                    self.seg = Seg::Closed;
                } else {
                    self.u = self.curve.c(t);
                    self.seg = Seg::Release { t, dt };
                }
            }
            Seg::Dip { from, to, k, len } => {
                let k = k + 1.0;
                if k >= len {
                    self.u = to;
                    self.seg = Seg::Rise { t: self.curve.inv(to) };
                } else {
                    self.u = to + (from - to) * (0.5 + 0.5 * (std::f64::consts::PI * k / len).cos());
                    self.seg = Seg::Dip { from, to, k, len };
                }
            }
        }
        self.u
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_and_inverses() {
        for c in Curve::ALL {
            assert!(c.c(0.0).abs() < 1e-12 && (c.c(1.0) - 1.0).abs() < 1e-12, "{c:?}");
            for k in 0..=100 {
                let t = k as f64 / 100.0;
                assert!((c.inv(c.c(t)) - t).abs() < 1e-9, "{c:?} {t}");
            }
        }
        // the README's half-rise levels
        let half = |c: Curve| 20.0 * c.c(0.5).log10();
        assert!((half(Curve::Square) + 12.04).abs() < 0.05);
        assert!((half(Curve::Linear) + 6.02).abs() < 0.05);
        assert!((half(Curve::SCurve) + 6.02).abs() < 0.05);
        assert!((half(Curve::Db) + 24.0).abs() < 0.6, "{}", half(Curve::Db));
        assert!((half(Curve::Pedal) + 16.0).abs() < 0.5, "{}", half(Curve::Pedal));
    }

    #[test]
    fn rise_hold_decay_spent() {
        let mut e = Engine::new();
        e.trigger(Shape { curve: Curve::Linear, rise: 100.0, decay: 50.0 }, 1.0, 24.0, 0);
        let v: Vec<f64> = (0..200).map(|_| e.next()).collect();
        assert!((v[49] - 0.5).abs() < 1e-9);
        assert_eq!(v[99], 1.0);
        assert!((v[124] - 0.5).abs() < 0.02);
        assert_eq!(v[150], 0.0);
        assert!(e.is_spent());
        e.release(10.0);
        assert!(e.is_closed());
    }

    #[test]
    fn retrigger_dips_then_continues_without_steps() {
        let mut e = Engine::new();
        let sh = Shape { curve: Curve::Square, rise: 1000.0, decay: 0.0 };
        e.trigger(sh, 1.0, 1.0, 0);
        for _ in 0..2000 {
            e.next();
        }
        assert_eq!(e.state(), State::Hold);
        e.trigger(sh, 0.7, 384.0, 0);
        let mut prev = e.u();
        let mut max_step = 0.0f64;
        let mut low = 1.0f64;
        for _ in 0..2000 {
            let u = e.next();
            max_step = max_step.max((u - prev).abs());
            low = low.min(u);
            prev = u;
        }
        assert!((low - 0.3).abs() < 1e-9, "{low}");
        // a raised cosine of 384 samples over 0.7 moves at most 0.7 · π / 2 / 384 per sample
        assert!(max_step <= 0.7 * std::f64::consts::FRAC_PI_2 / 384.0 + 1e-9, "{max_step}");
        assert_eq!(e.state(), State::Hold);
        // dip 0 ignores retriggers
        e.trigger(sh, 0.0, 384.0, 0);
        assert_eq!(e.next(), 1.0);
    }

    #[test]
    fn release_plays_the_curve_backwards_and_closes() {
        let mut e = Engine::new();
        e.trigger(Shape { curve: Curve::Square, rise: 10.0, decay: 0.0 }, 1.0, 1.0, 0);
        for _ in 0..20 {
            e.next();
        }
        e.release(100.0);
        let v: Vec<f64> = (0..120).map(|_| e.next()).collect();
        assert!((v[49] - 0.25).abs() < 0.01, "{}", v[49]);
        assert!(e.is_closed());
        assert_eq!(e.state(), State::Idle);
    }

    #[test]
    fn scheduled_trigger_waits() {
        let mut e = Engine::new();
        e.trigger(Shape { curve: Curve::Linear, rise: 10.0, decay: 0.0 }, 1.0, 1.0, 5);
        let v: Vec<f64> = (0..8).map(|_| e.next()).collect();
        assert_eq!(&v[..4], &[0.0; 4]);
        assert!(v[4] > 0.0);
    }
}
