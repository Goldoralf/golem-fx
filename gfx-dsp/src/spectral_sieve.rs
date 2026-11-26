// SPDX-License-Identifier: GPL-3.0-or-later
//! The root sieve of the Spectral core (gfx-organ README §2.3). Placeholder until the sieve is coded.

use crate::spectral::Bank;

/// The sieve's constants.
#[derive(Clone, Copy, Debug, Default)]
pub struct SieveParams {}

/// What the sieve measured (tests, meters).
#[derive(Clone, Copy, Debug, Default)]
pub struct SieveStats {
    pub roots: usize,
}

/// The root sieve's state per clean band.
#[derive(Clone, Debug, Default)]
pub struct Sieve {
    root: Vec<bool>,
}

impl Sieve {
    pub fn prepare(&mut self, clean: &Bank, _tick_s: f64) {
        self.root = vec![false; clean.n];
    }

    pub fn reset(&mut self) {
        self.root.iter_mut().for_each(|r| *r = false);
    }

    pub fn tick(&mut self, _clean: &Bank, _picked: bool, _now: u64) {}

    /// Band b is a confirmed root.
    #[inline]
    pub fn holds_root(&self, b: usize) -> bool {
        self.root[b]
    }
}
