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
pub mod blep;
pub mod circuit;
pub mod circuit_nl;
pub mod converter;
pub mod convolve;
pub mod cuts;
pub mod crossover;
pub mod delay;
pub mod detect;
pub mod duck;
pub mod pv_shift;
pub mod effect;
pub mod envelope;
pub mod exp_smooth;
pub mod flanger;
pub mod geq;
pub mod ir;
#[cfg(feature = "ir-host")]
pub mod ir_engine;
#[cfg(feature = "ir-host")]
pub mod ir_host;
pub mod lfo;
pub mod matched;
pub mod onset;
pub mod oversample;
pub mod pickup;
pub mod pitch;
pub mod pot;
pub mod reverb;
pub mod sidechain;
pub mod smooth;
pub mod ssb;
pub mod stereo;
pub mod svf;
pub mod swell;
pub mod track;
pub mod tape;
pub mod transient;
pub mod transport;
pub mod util;
pub mod vibe;
pub mod wah;
pub mod zoom;

pub use effect::{Effect, ParamInfo, ParamKind};
