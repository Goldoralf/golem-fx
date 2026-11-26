// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-octafuzz — golem's octave fuzz: the transformer octave (Tycobrahe-type) solved as a circuit,
//! the guitar in front of it included. A thin pedal over gfx-fuzz's engine (design: gfx-fuzz's README
//! §2.3 "Octave Up", §2.7, §2.8; this crate's README: its parameters and measurements).
//!
//! - The circuit is gfx-fuzz's `circuits::octave_up`, run by gfx-fuzz's processor in its Octave Up
//!   mode ([`gfx_fuzz::Fuzz::octafuzz`]): 8× oversampling, the guitar's network, `bias` on R2,
//!   `voltage` / `sag` / `temp`, linked mono, the tone tilt, `mix` / `bass_keep`, the 20 Hz second-order
//!   DC guard after the `octave` crossfade.
//! - `octave` (on the face): a linear crossfade from the fuzz tap (a · v(winding)) to the rectified
//!   output, at the oversampled rate before the decimation (one decimator for both).
//! - Ids keep gfx-fuzz's meanings, ranges and defaults; `model`, `stack`, `mids`, `leakage` are not
//!   taken.

use gfx_dsp::{Effect, ParamInfo};
use gfx_fuzz::Fuzz;

#[cfg(feature = "plugin")]
pub mod plugin;

/// Parameter indices (PARAMS order).
pub mod p {
    pub const FUZZ: usize = 0;
    pub const OCTAVE: usize = 1;
    pub const TONE: usize = 2;
    pub const LEVEL: usize = 3;
    pub const GUITAR_VOL: usize = 4;
    pub const BUFFER: usize = 5;
    pub const PICKUP: usize = 6;
    pub const BIAS: usize = 7;
    pub const TEMP: usize = 8;
    pub const VOLTAGE: usize = 9;
    pub const SAG: usize = 10;
    pub const INPUT: usize = 11;
    pub const MIX: usize = 12;
    pub const BASS_KEEP: usize = 13;
    pub const BYPASS: usize = 14;
}

pub const NP: usize = 15;

pub use gfx_fuzz::PICKUPS;

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::float("fuzz", "Fuzz", 0.0, 100.0, 70.0).with_exp(),
    ParamInfo::float("octave", "Octave", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::float("tone", "Tone", -100.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("level", "Level", -24.0, 24.0, 0.0).with_exp(),
    ParamInfo::float("guitar_vol", "Guitar Vol", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::boolean("buffer", "Buffer", false),
    ParamInfo::enumeration("pickup", "Pickup", 4, 0),
    ParamInfo::float("bias", "Bias", -100.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("temp", "Temperature", 0.0, 50.0, 25.0),
    ParamInfo::float("voltage", "Voltage", 3.0, 18.0, 9.0).with_exp(),
    ParamInfo::float("sag", "Sag", 0.0, 100.0, 0.0),
    ParamInfo::float("input", "Input", -12.0, 12.0, 0.0),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::float("bass_keep", "Bass Keep", 0.0, 500.0, 0.0),
    ParamInfo::boolean("bypass", "Bypass", false),
];

/// The pedal: gfx-fuzz's processor on Octave Up.
pub struct Octafuzz {
    dsp: Fuzz,
}

impl Octafuzz {
    pub fn new() -> Self {
        Self {
            dsp: Fuzz::octafuzz(),
        }
    }

    /// Q3's collector (the transformer's driver), 50 ms average: the `bias_v` meter.
    pub fn bias_v(&self) -> f32 {
        self.dsp.bias_v()
    }

    /// The circuit's supply now (sag included): the `supply_v` meter.
    pub fn supply_v(&self) -> f32 {
        self.dsp.supply_v()
    }

    /// Solver statistics of the running circuit of channel 0 (tests, bench).
    pub fn stats(&self) -> gfx_dsp::circuit_nl::Stats {
        self.dsp.stats()
    }

    /// The engine (tests: true stereo, forced oversampling).
    #[doc(hidden)]
    pub fn engine(&mut self) -> &mut Fuzz {
        &mut self.dsp
    }
}

impl Default for Octafuzz {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Octafuzz {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, max_block: usize, channels: usize) {
        self.dsp.prepare(sample_rate, max_block, channels);
    }

    fn reset(&mut self) {
        self.dsp.reset();
    }

    fn set_param(&mut self, id: &str, value: f32) {
        let Some(q) = PARAMS.iter().find(|q| q.id == id) else {
            return;
        };
        let v = value.clamp(q.min, q.max);
        if id == "octave" {
            self.dsp.set_octave(v);
        } else {
            self.dsp.set_param(id, v);
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.dsp.process(buffers);
    }
}

#[cfg(test)]
mod tests;
