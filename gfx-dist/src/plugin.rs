// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second (two independent channels).
//! Linear ranges only (golem contract). Reported latency 0 (minimum-phase oversampling, a few
//! samples of group delay, the gfx-drive / gfx-emet precedent). Generated from PARAMS (same order
//! and ids).

use crate::{Dist, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

const NP: usize = 11;

/// `circuit`: the shipped circuits, in the README's order (the Discrete OD is appended later).
#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum CircuitP {
    #[id = "opamp-dist"]
    #[name = "Op-Amp Dist"]
    OpAmpDist,
    #[id = "booster-dist"]
    #[name = "Booster Dist"]
    BoosterDist,
    #[id = "blend-od"]
    #[name = "Blend OD"]
    BlendOd,
    #[id = "mosfet-od"]
    #[name = "MOSFET OD"]
    MosfetOd,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum DiodesP {
    #[id = "circuit"]
    #[name = "Circuit"]
    Circuit,
    #[id = "silicon"]
    #[name = "Silicon"]
    Silicon,
    #[id = "germanium"]
    #[name = "Germanium"]
    Germanium,
    #[id = "led"]
    #[name = "LED"]
    Led,
    #[id = "mosfet"]
    #[name = "MOSFET"]
    Mosfet,
    #[id = "mosfet-ge"]
    #[name = "MOSFET + Ge"]
    MosfetGe,
    #[id = "none"]
    #[name = "None"]
    None,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum OpAmpP {
    #[id = "circuit"]
    #[name = "Circuit"]
    Circuit,
    #[id = "lm308"]
    #[name = "LM308"]
    Lm308,
    #[id = "lm358"]
    #[name = "LM358"]
    Lm358,
    #[id = "4558"]
    #[name = "4558"]
    Rc4558,
    #[id = "tl072"]
    #[name = "TL072"]
    Tl072,
    #[id = "ideal"]
    #[name = "Ideal"]
    Ideal,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

#[derive(Params)]
struct DistParams {
    #[id = "circuit"]
    circuit: EnumParam<CircuitP>,
    #[id = "gain"]
    gain: FloatParam,
    #[id = "tone_stack"]
    tone_stack: FloatParam,
    #[id = "level"]
    level: FloatParam,
    #[id = "diodes"]
    diodes: EnumParam<DiodesP>,
    #[id = "opamp"]
    opamp: EnumParam<OpAmpP>,
    #[id = "supply"]
    supply: FloatParam,
    #[id = "hp"]
    hp: BoolParam,
    #[id = "tight"]
    tight: FloatParam,
    #[id = "input"]
    input: FloatParam,
    #[id = "bass_keep"]
    bass_keep: FloatParam,
}

impl Default for DistParams {
    fn default() -> Self {
        Self {
            circuit: EnumParam::new("Circuit", CircuitP::OpAmpDist),
            gain: f(1, " %", 0),
            tone_stack: f(2, " %", 0),
            level: f(3, " dB", 1),
            diodes: EnumParam::new("Clippers", DiodesP::Circuit),
            opamp: EnumParam::new("Op-Amp", OpAmpP::Circuit),
            supply: f(6, " %", 0),
            hp: BoolParam::new("HP Mode", false),
            tight: f(8, " Hz", 0),
            input: f(9, " dB", 1),
            bass_keep: f(10, " Hz", 0),
        }
    }
}

impl DistParams {
    fn values(&self) -> [f32; NP] {
        [
            self.circuit.value() as usize as f32,
            self.gain.value(),
            self.tone_stack.value(),
            self.level.value(),
            self.diodes.value() as usize as f32,
            self.opamp.value() as usize as f32,
            self.supply.value(),
            if self.hp.value() { 1.0 } else { 0.0 },
            self.tight.value(),
            self.input.value(),
            self.bass_keep.value(),
        ]
    }
}

pub struct GfxDist {
    params: Arc<DistParams>,
    dsp: Dist,
    last: [f32; NP],
}

impl Default for GfxDist {
    fn default() -> Self {
        Self { params: Arc::new(DistParams::default()), dsp: Dist::new(), last: [f32::NAN; NP] }
    }
}

impl GfxDist {
    fn sync(&mut self, force: bool) {
        let v = self.params.values();
        for (k, p) in PARAMS.iter().enumerate() {
            if force || v[k] != self.last[k] {
                self.last[k] = v[k];
                self.dsp.set_param(p.id, v[k]);
            }
        }
    }
}

impl Plugin for GfxDist {
    const NAME: &'static str = "GoleM FX Dist";
    const VENDOR: &'static str = "GoleM";
    const URL: &'static str = "https://golem.fm";
    const EMAIL: &'static str = "contact@golem.fm";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
    ];

    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(&mut self, layout: &AudioIOLayout, config: &BufferConfig, _ctx: &mut impl InitContext<Self>) -> bool {
        let channels = layout.main_output_channels.map_or(1, |c| c.get() as usize);
        self.sync(true);
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
        true
    }

    fn reset(&mut self) {
        self.sync(true);
        self.dsp.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.sync(false);
        self.dsp.process(buffer.as_slice());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxDist {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxDist00001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_vst3!(GfxDist);
