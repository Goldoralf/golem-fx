// SPDX-License-Identifier: GPL-3.0-or-later
//! TEMPORARY — gfx-pitch's engine behind a wrapper that reports **0 latency**.
//!
//! An A / B build for golem-rack's REAPER stall test (2026-11-12): run beside gfx-pitch, it tells
//! whether a stall on enable comes from the plugin's activation (both stall) or from REAPER's delay
//! compensation re-sync when a latent plugin is enabled (only the 12 ms build stalls). Delete once settled.
//!
//! The engine source is gfx-pitch's own, included by path (its `plugin` feature is not enabled here, so
//! its VST3 export is not compiled in).

#[path = "../../gfx-pitch/src/lib.rs"]
#[allow(dead_code)]
mod engine;

pub use engine::*;

#[cfg(feature = "wrap")]
mod plugin;
