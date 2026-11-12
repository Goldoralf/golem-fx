// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-flanger — golem's flanger (design: README.md).
//!
//! Stands for golem's `flanger-js` block (ids `length`, `feedback` in dB, `wet`, `dry`, `rate` kept). The
//! engine lives in gfx-dsp (`gfx_dsp::flanger`), shared with gfx-flanger-calf (golem's `flanger` block,
//! Calf's ids); this crate is its VST3 wrapper and its bench suite.

pub use gfx_dsp::flanger::*;

#[cfg(feature = "plugin")]
mod plugin;

#[cfg(test)]
mod tests;
