// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-flanger — golem's flanger (design: README.md).
//!
//! Stands for golem's `flanger-js` block (ids `length`, `feedback` in dB, `wet`, `dry`, `rate` kept). The
//! engine lives in gfx-dsp (`gfx_dsp::flanger`); this crate is its VST3 wrapper and its bench suite. golem's
//! single flanger since catalog v40 (the Calf `flanger` block and gfx-flanger-calf were retired 2026-11-13).

pub use gfx_dsp::flanger::*;

#[cfg(feature = "plugin")]
mod plugin;

#[cfg(test)]
mod tests;
