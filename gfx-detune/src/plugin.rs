// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): stereo 2 / 2 first, mono 1 / 1. Linear ranges only (golem contract).

use crate::{Detune, Settings, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum VoicesP {
    #[id = "1"]
    #[name = "1"]
    One,
    #[id = "2"]
    #[name = "2"]
    Two,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `detune`, `wet`, `dry` are golem's catalogue ids (`detune` block).
#[derive(Params)]
struct DetuneParams {
    #[id = "detune"]
    detune: FloatParam,
    #[id = "wet"]
    wet: FloatParam,
    #[id = "dry"]
    dry: FloatParam,
    #[id = "delay"]
    delay: FloatParam,
    #[id = "voices"]
    voices: EnumParam<VoicesP>,
}

impl Default for DetuneParams {
    fn default() -> Self {
        Self {
            detune: f(0, "", 2),
            wet: f(1, "", 2),
            dry: f(2, "", 2),
            delay: f(3, " ms", 1),
            voices: EnumParam::new("Voices", VoicesP::Two),
        }
    }
}

impl DetuneParams {
    fn settings(&self) -> Settings {
        Settings {
            v: [
                self.detune.value(),
                self.wet.value(),
                self.dry.value(),
                self.delay.value(),
                self.voices.value() as usize as f32,
            ],
        }
    }
}

pub struct GfxDetune {
    params: Arc<DetuneParams>,
    dsp: Detune,
}

impl Default for GfxDetune {
    fn default() -> Self {
        Self { params: Arc::new(DetuneParams::default()), dsp: Detune::new() }
    }
}

impl Plugin for GfxDetune {
    const NAME: &'static str = "GoleM FX Detune";
    const VENDOR: &'static str = "GoleM";
    const URL: &'static str = "https://golem.fm";
    const EMAIL: &'static str = "contact@golem.fm";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
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
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
        self.dsp.set(self.params.settings());
        true
    }

    fn reset(&mut self) {
        self.dsp.set(self.params.settings());
        self.dsp.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let s = self.params.settings();
        if s != self.dsp.settings() {
            self.dsp.set(s);
        }
        self.dsp.process(buffer.as_slice());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxDetune {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxDetune001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::PitchShift];
}

nih_export_vst3!(GfxDetune);
