// SPDX-License-Identifier: GPL-3.0-or-later
//! Potentiometer taper laws: the fraction of the track's resistance between the start and the wiper
//! at a rotation `t` (0 – 1). A pedal knob's percentage is rotation; the law is the part's.
//!
//! - B (linear): t.
//! - A (log, audio): (10^(2t) − 1) / 99, the exponential law gfx-emet uses: ≈ 9 % at half travel.
//! - C (reverse log): the mirror of A, 1 − A(1 − t).
//! - W: the two halves of a linear track seen from both ends (the "W" anti-log / log pair of a balance
//!   pot is out of scope; W here is the symmetric S-law 3t² − 2t³, flat at both ends).

/// A pot's law.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Taper {
    /// B: linear.
    Linear,
    /// A: audio (log).
    Log,
    /// C: reverse audio.
    RevLog,
    /// W: symmetric S-law.
    W,
}

/// The audio taper: the fraction of the resistance at rotation `t` (0 – 1); ≈ 9 % at half.
pub fn a_taper(t: f64) -> f64 {
    (10f64.powf(2.0 * t.clamp(0.0, 1.0)) - 1.0) / 99.0
}

/// The reverse audio taper (C): the mirror of A.
pub fn c_taper(t: f64) -> f64 {
    1.0 - a_taper(1.0 - t)
}

impl Taper {
    /// The fraction (0 – 1) of the track between the start and the wiper at rotation `t` (0 – 1).
    pub fn fraction(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Taper::Linear => t,
            Taper::Log => a_taper(t),
            Taper::RevLog => c_taper(t),
            Taper::W => t * t * (3.0 - 2.0 * t),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn laws_have_their_ends_and_middles() {
        for law in [Taper::Linear, Taper::Log, Taper::RevLog, Taper::W] {
            assert!(law.fraction(0.0).abs() < 1e-12, "{law:?}");
            assert!((law.fraction(1.0) - 1.0).abs() < 1e-12, "{law:?}");
            // monotone
            let mut last = -1.0;
            for k in 0..=100 {
                let f = law.fraction(k as f64 / 100.0);
                assert!(f >= last, "{law:?} at {k}");
                last = f;
            }
        }
        assert!((Taper::Log.fraction(0.5) - 9.0 / 99.0).abs() < 1e-12);
        assert!((Taper::RevLog.fraction(0.5) - 90.0 / 99.0).abs() < 1e-12);
        assert!((Taper::Linear.fraction(0.5) - 0.5).abs() < 1e-12);
        assert!((Taper::W.fraction(0.5) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn a_taper_is_gfx_emets() {
        // gfx-emet keeps its own copy (unchanged); the law is the same
        for k in 0..=10 {
            let t = k as f64 / 10.0;
            assert_eq!(a_taper(t), (10f64.powf(2.0 * t) - 1.0) / 99.0);
        }
    }
}
