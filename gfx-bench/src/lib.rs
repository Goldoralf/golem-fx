// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-bench — offline render and measurement of golem-fx effects (design: README.md).
//!
//! It drives each effect's host-independent processor ([`gfx_dsp::Effect`]); golem's own tools test
//! the VST3 in REAPER and the Golem Engine.

pub mod checks;
pub mod selftest;
pub mod signals;
pub mod wav;

use gfx_dsp::Effect;

/// Every effect the bench knows, by crate name.
pub const EFFECTS: &[&str] = &["gfx-boost", "gfx-gate", "gfx-comp", "gfx-limiter", "gfx-eq", "gfx-deesser", "gfx-wah", "gfx-autowah", "gfx-tremolo"];

pub fn make(name: &str) -> Option<Box<dyn Effect>> {
    match name {
        "gfx-boost" | "boost" => Some(Box::new(gfx_boost::Boost::new())),
        "gfx-gate" | "gate" => Some(Box::new(gfx_gate::Gate::new())),
        "gfx-comp" | "comp" => Some(Box::new(gfx_comp::Comp::new())),
        "gfx-limiter" | "limiter" => Some(Box::new(gfx_limiter::Limiter::new())),
        "gfx-eq" | "eq" => Some(Box::new(gfx_eq::Eq::new())),
        "gfx-deesser" | "deesser" => Some(Box::new(gfx_deesser::Deesser::new())),
        "gfx-wah" | "wah" => Some(Box::new(gfx_wah::Wah::new())),
        "gfx-autowah" | "autowah" => Some(Box::new(gfx_autowah::Autowah::new())),
        "gfx-tremolo" | "tremolo" => Some(Box::new(gfx_tremolo::Tremolo::new())),
        // the bench's own faulty effects (selftest.rs)
        "_hard-gain" => Some(Box::new(selftest::HardGain::default())),
        "_sloppy" => Some(Box::new(selftest::Sloppy::default())),
        _ => None,
    }
}

/// The common checks' options for an effect (bounds its design documents).
pub fn options(name: &str) -> checks::Options {
    let mut o = checks::Options::default();
    if matches!(name, "gfx-wah" | "wah" | "gfx-autowah" | "autowah") {
        o.skip_latency = true;
        // a +20 dB resonance swept heel to toe in 15 ms is a chirp, and `drive` clips that resonance
        // on purpose: both put real energy above 10 kHz that is not a click
        o.click_bound_db = -35.0;
    }
    o
}

/// A fresh, prepared and reset instance.
pub fn fresh(name: &str, sample_rate: f32, channels: usize, max_block: usize) -> Box<dyn Effect> {
    let mut e = make(name).unwrap_or_else(|| panic!("unknown effect {name}"));
    e.prepare(sample_rate, max_block, channels);
    e.reset();
    e
}

/// Sets parameters, settles them (by processing silence is NOT done: settings are applied before
/// reset so they start settled), and renders `input` in blocks of `block`.
pub fn render(effect: &mut dyn Effect, input: &[Vec<f32>], block: usize) -> Vec<Vec<f32>> {
    let mut out: Vec<Vec<f32>> = input.to_vec();
    let n = out.first().map_or(0, |c| c.len());
    let mut start = 0;
    while start < n {
        let end = (start + block).min(n);
        let mut slices: Vec<&mut [f32]> = out.iter_mut().map(|c| &mut c[start..end]).collect();
        effect.process(&mut slices);
        start = end;
    }
    out
}

/// Applies settings and lets them settle: set, then `reset()` (smoothers settle on their targets).
pub fn apply_settings(effect: &mut dyn Effect, settings: &[(String, f32)]) {
    for (id, v) in settings {
        effect.set_param(id, *v);
    }
    effect.reset();
}
