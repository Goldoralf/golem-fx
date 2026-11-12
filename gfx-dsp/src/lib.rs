// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-dsp — the shared DSP library of golem-fx (see README.md).
//!
//! Rules every block follows:
//! - no plugin dependency: plain buffers of `f32`;
//! - real-time safe: memory is sized in `prepare`, never on the audio path;
//! - zero latency unless the block's name and docs say otherwise;
//! - deterministic: every LFO and random source starts from a fixed state on `reset`;
//! - audio in `f32`, recursive state and phase accumulators in `f64`.

#![allow(clippy::should_implement_trait, clippy::needless_range_loop)]

pub mod adaa;
pub mod bbd;
pub mod delay;
pub mod effect;
pub mod envelope;
pub mod exp_smooth;
pub mod lfo;
pub mod matched;
pub mod oversample;
pub mod smooth;
pub mod svf;
pub mod transport;
pub mod util;
pub mod wah;

pub use effect::{Effect, ParamInfo, ParamKind};
