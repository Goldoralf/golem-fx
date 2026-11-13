// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second. Linear ranges only (golem
//! contract). Generated from PARAMS (same order and ids).

use crate::{Shem, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum Voice {
    #[id = "focused"]
    Focused,
    #[id = "full"]
    Full,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

#[derive(Params)]
struct ShemParams {
    #[id = "grind"]
    grind: FloatParam,
    #[id = "voice"]
    voice: EnumParam<Voice>,
    #[id = "bright"]
    bright: FloatParam,
}

impl Default for ShemParams {
    fn default() -> Self {
        Self {
            grind: f(0, " dB", 1),
            voice: EnumParam::new("Voice", Voice::Focused),
            bright: f(2, " dB", 1),
        }
    }
}

pub struct GfxShem {
    params: Arc<ShemParams>,
    dsp: Shem,
    last: [f32; 3],
}

impl Default for GfxShem {
    fn default() -> Self {
        Self { params: Arc::new(ShemParams::default()), dsp: Shem::new(), last: [f32::NAN; 3] }
    }
}

impl GfxShem {
    fn sync(&mut self, force: bool) {
        let v = [
            self.params.grind.value(),
            if self.params.voice.value() == Voice::Full { 1.0 } else { 0.0 },
            self.params.bright.value(),
        ];
        for (k, p) in PARAMS.iter().enumerate() {
            if force || v[k] != self.last[k] {
                self.last[k] = v[k];
                self.dsp.set_param(p.id, v[k]);
            }
        }
    }
}

impl Plugin for GfxShem {
    const NAME: &'static str = "GoleM FX Shem";
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

impl Vst3Plugin for GfxShem {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxShem00001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Eq];
}

nih_export_vst3!(GfxShem);
