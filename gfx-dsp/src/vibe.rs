// SPDX-License-Identifier: GPL-3.0-or-later
//! Uni-Vibe core (Darabundit, Wedelich & Bischoff, DAFx 2019; guitarix vibe.cc): shared by gfx-phaser's
//! Vibe and gfx-vibrato's Vibe model.
//!
//! - Four phase-shift stages with **unequal capacitors** (15 nF, 220 nF, 470 pF, 4.7 nF), all lit by one
//!   lamp. Their corners sit decades apart, so only about two notches are ever audible.
//! - Each stage's coupling capacitor makes it a low shelf whose corner moves with the light. That is
//!   the "throb": a level swing in the lows.
//! - Lamp: a turn-on curve, then thermal lag. LDR: fast when the light rises, slow when it falls.
//! - Each stage clips softly and asymmetrically (a biased tanh with unity slope at rest).
//!
//! Vibrato = the last stage alone. Chorus = half dry, half wet.

use crate::util::one_pole_coef;

/// Stage capacitors (F).
pub const CAPS: [f64; 4] = [15e-9, 220e-9, 470e-12, 4.7e-9];
/// Coupling capacitor of the shelf (F) and the level it leaves in the lows.
const C_SHELF: f64 = 100e-9;
const SHELF_LOW: f32 = 0.7;
/// LDR resistance range (Ω): lit … dark.
pub const R_LIT: f64 = 4.0e3;
pub const R_DARK: f64 = 150.0e3;

#[derive(Clone, Copy, Debug, Default)]
struct Stage {
    x1: f32,
    y1: f32,
    shelf: f32,
}

/// One channel's four stages.
#[derive(Clone, Debug, Default)]
pub struct Vibe {
    st: [Stage; 4],
}

#[inline]
fn soft(x: f32) -> f32 {
    // biased tanh, unity slope and zero output at rest
    const A: f32 = 0.5;
    const B: f32 = 0.3;
    let tb = B.tanh();
    ((A * x + B).tanh() - tb) / (A * (1.0 - tb * tb))
}

impl Vibe {
    pub fn reset(&mut self) {
        self.st = Default::default();
    }

    /// One sample through the four stages at LDR resistance `r` (Ω).
    #[inline]
    pub fn process(&mut self, x: f32, r: f64, sample_rate: f32) -> f32 {
        let sr = sample_rate as f64;
        let mut v = x;
        for (k, s) in self.st.iter_mut().enumerate() {
            let fc = (1.0 / (std::f64::consts::TAU * r * CAPS[k])).min(0.45 * sr);
            let t = (std::f64::consts::PI * fc / sr).tan();
            let a = ((t - 1.0) / (t + 1.0)) as f32;
            let y = a * v + s.x1 - a * s.y1;
            s.x1 = v;
            s.y1 = y;
            // the coupling capacitor: a low shelf whose corner follows the light
            let fb = (1.0 / (std::f64::consts::TAU * r * C_SHELF)).min(0.45 * sr);
            let g = (1.0 - (-std::f64::consts::TAU * fb / sr).exp()) as f32;
            s.shelf += g * (y - s.shelf);
            v = soft(y - (1.0 - SHELF_LOW) * s.shelf);
        }
        v
    }
}

/// The lamp and its LDR, shared by the channels it lights.
#[derive(Clone, Debug, Default)]
pub struct Lamp {
    lamp: f64,
    ldr: f64,
}

impl Lamp {
    pub fn reset(&mut self) {
        self.lamp = 0.0;
        self.ldr = 0.0;
    }

    /// Drive `u` (0 … 1 from the LFO) scaled by `depth`; lag in ms. Returns the LDR resistance (Ω).
    #[inline]
    pub fn next(&mut self, u: f32, depth: f32, lag_ms: f32, sample_rate: f32) -> f64 {
        let sr = sample_rate as f64;
        // filament turn-on: little light at low drive
        let drive = (u.clamp(0.0, 1.0) as f64).powf(1.5);
        let k = one_pole_coef(lag_ms.max(1.0) as f64 * 0.001 * sr);
        self.lamp = drive + (self.lamp - drive) * k;
        let kl = if self.lamp > self.ldr { one_pole_coef(0.008 * sr) } else { one_pole_coef(0.05 * sr) };
        self.ldr = self.lamp + (self.ldr - self.lamp) * kl;
        // light swings the resistance on a log curve from dark toward lit, as far as `depth` reaches
        let l = self.ldr * depth.clamp(0.0, 1.0) as f64;
        R_DARK * (R_LIT / R_DARK).powf(l)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_lowers_the_resistance_with_lag() {
        let mut l = Lamp::default();
        let mut r = R_DARK;
        for _ in 0..4800 {
            r = l.next(1.0, 1.0, 30.0, 48000.0);
        }
        assert!(r < 0.2 * R_DARK, "{r}");
        let r_lit = r;
        for _ in 0..480 {
            r = l.next(0.0, 1.0, 30.0, 48000.0);
        }
        // 10 ms after the drive goes away the LDR is still mostly lit
        assert!(r < 0.5 * R_DARK && r > r_lit, "{r}");
    }

    #[test]
    fn stages_pass_level_above_the_shelf() {
        let mut v = Vibe::default();
        let sr = 48000.0;
        let x: Vec<f32> = (0..48000).map(|i| 0.1 * (std::f32::consts::TAU * 1000.0 * i as f32 / sr).sin()).collect();
        let y: Vec<f32> = x.iter().map(|s| v.process(*s, 20e3, sr)).collect();
        let peak = y[24000..].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!((peak / 0.1 - 1.0).abs() < 0.1, "{peak}");
        assert!(y.iter().all(|s| s.is_finite()));
    }
}
