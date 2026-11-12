// SPDX-License-Identifier: GPL-3.0-or-later
//! The bench catches the faults it claims to, and every registered effect passes its common checks
//! at 48 kHz (the full four-rate run is `gfx-bench check <effect>`).

use gfx_bench::checks::{self, Options};

#[test]
fn catches_a_click() {
    let r = checks::param_steps("_hard-gain", 48000.0, &Options::default());
    assert!(!r.pass, "{}", r.detail);
}

#[test]
fn catches_unreported_latency_and_dirty_reset() {
    assert!(!checks::latency("_sloppy", 48000.0).pass);
    assert!(!checks::determinism("_sloppy", 48000.0).pass);
}

#[test]
fn every_effect_passes_at_48k() {
    for name in gfx_bench::EFFECTS {
        let opts = Options { sample_rates: vec![48000.0], ..gfx_bench::options(name) };
        for r in checks::run_all(name, &opts) {
            assert!(r.pass, "{name}: {} — {}", r.name, r.detail);
        }
    }
}
