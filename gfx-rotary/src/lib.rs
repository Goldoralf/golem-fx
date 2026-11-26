// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-rotary — golem's rotary speaker (design: README.md).
//!
//! Stands for golem's `rotary` block (ids `speed`, `mic`, `spacing`, `depth` kept). For guitar and keys
//! (golem's organ keeps setBfree's own Leslie).
//!
//! The engine lives in gfx-dsp `rotary` (moved there unchanged, under a bit-identical null of this
//! crate's renders, so that gfx-organ plays the same Leslie); this crate is its plugin.

pub use gfx_dsp::rotary::*;

#[cfg(feature = "plugin")]
pub mod plugin;

#[cfg(test)]
mod tests;
