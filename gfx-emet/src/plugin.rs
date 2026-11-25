// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second (two independent channels).
//! Linear ranges only (golem contract). Generated from PARAMS (same order and ids).

use crate::{Emet, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

#[derive(Params)]
struct EmetParams {
    #[id = "gain"]
    gain: FloatParam,
    #[id = "tone_stack"]
    tone_stack: FloatParam,
    #[id = "level"]
    level: FloatParam,
    #[id = "headroom"]
    headroom: FloatParam,
    #[id = "asym"]
    asym: FloatParam,
    #[id = "tight"]
    tight: FloatParam,
    #[id = "input"]
    input: FloatParam,
    #[id = "bass_keep"]
    bass_keep: FloatParam,
}

impl Default for EmetParams {
    fn default() -> Self {
        Self {
            gain: f(0, " %", 0),
            tone_stack: f(1, " %", 0),
            level: f(2, " dB", 1),
            headroom: f(3, " dB", 1),
            asym: f(4, " %", 0),
            tight: f(5, " Hz", 0),
            input: f(6, " dB", 1),
            bass_keep: f(7, " Hz", 0),
        }
    }
}

impl EmetParams {
    fn values(&self) -> [f32; 8] {
        [
            self.gain.value(),
            self.tone_stack.value(),
            self.level.value(),
            self.headroom.value(),
            self.asym.value(),
            self.tight.value(),
            self.input.value(),
            self.bass_keep.value(),
        ]
    }
}

pub struct GfxEmet {
    params: Arc<EmetParams>,
    dsp: Emet,
    last: [f32; 8],
}

impl Default for GfxEmet {
    fn default() -> Self {
        Self { params: Arc::new(EmetParams::default()), dsp: Emet::new(), last: [f32::NAN; 8] }
    }
}

impl GfxEmet {
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

impl Plugin for GfxEmet {
    const NAME: &'static str = "GoleM FX Emet";
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

    #[cfg(feature = "gui")]
    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-emet"))
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

impl Vst3Plugin for GfxEmet {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxEmet00001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_vst3!(GfxEmet);
