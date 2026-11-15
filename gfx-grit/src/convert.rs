// SPDX-License-Identifier: GPL-3.0-or-later
//! The one-time conversion of golem's `grit` values (Prisma's laws) to gfx-grit's (README §3.2), for
//! golem's engine table and the tests. Keyed by the slot's module type: `*_v1` / `*_v2` keep their %
//! unit but each type's % now follows a law linear in a real unit (dB, bits, offset).
//!
//! Prisma's laws are restated from the README's formulas (§1.1); no Prisma code is used.

use crate::modules::Kind;

/// Prisma's clip drive value `val` at v (0 – 1): drive = −20 log₁₀ val dB.
pub fn prisma_clip_val(v: f64) -> f64 {
    1.0 - (1.0 - ((1.0 - v).powi(10) + 1.0 - v.powf(0.2)) / 2.0) * 0.99
}

/// Zero cross: (Prisma v, our dead zone T in dBFS), interpolated linearly (README §3.2).
pub const ZC_TABLE: [(f64, f64); 12] = [
    (0.05, -67.0),
    (0.10, -46.0),
    (0.20, -34.0),
    (0.30, -30.0),
    (0.40, -27.0),
    (0.50, -25.0),
    (0.60, -22.5),
    (0.70, -20.0),
    (0.80, -16.0),
    (0.90, -11.0),
    (0.95, -8.5),
    (1.00, -6.0),
];

fn zc_t(v: f64) -> f64 {
    if v <= ZC_TABLE[0].0 {
        return ZC_TABLE[0].1;
    }
    for w in ZC_TABLE.windows(2) {
        let ((v0, t0), (v1, t1)) = (w[0], w[1]);
        if v <= v1 {
            return t0 + (t1 - t0) * (v - v0) / (v1 - v0);
        }
    }
    ZC_TABLE[ZC_TABLE.len() - 1].1
}

/// A module value: golem's old % (Prisma) → gfx-grit's %, for a slot of type `kind`.
pub fn module_value(kind: Kind, old_pct: f64) -> f64 {
    let v = (old_pct / 100.0).clamp(0.0, 1.0);
    let new = match kind {
        Kind::Soft | Kind::Hard => -20.0 * prisma_clip_val(v).log10() / 48.0,
        Kind::Fold => 20.0 * (70.0 * v * v + 1.0).log10() / 48.0,
        Kind::SineFold => (5.0 * v * v).min(1.0) * 20.0 * (70.0 * v * v + 2.0).log10() / 48.0,
        Kind::ZeroCross => (zc_t(v) + 66.0) / 60.0,
        Kind::BitCrush => {
            let bits = (2.0 / (0.99 * v.powi(5) + 0.01)).log2();
            (16.0 - bits) / 15.0
        }
        Kind::Gain => {
            if v <= 0.0 {
                0.0
            } else {
                (40.0 * (v / 0.35).log10() + 24.0) / 48.0
            }
        }
        Kind::Dc => 0.5 + 0.5 * (2.0 * v - 1.0).powi(5),
        Kind::StereoDc => v.powi(5),
        _ => v,
    };
    100.0 * new.clamp(0.0, 1.0)
}

/// A band gain: golem's old % ((2g)², 50 % = 0 dB) → dB, −60 = mute.
pub fn band_gain_db(old_pct: f64) -> f64 {
    let g = old_pct / 100.0;
    if g <= 0.0 {
        -60.0
    } else {
        (40.0 * (2.0 * g).log10()).clamp(-60.0, 12.0)
    }
}
