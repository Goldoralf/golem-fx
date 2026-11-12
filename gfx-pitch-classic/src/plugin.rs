// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2. Linear ranges only (golem contract).

use crate::{PitchClassic, Settings, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `shift`, `window`, `overlap`, `wet`, `dry` are golem's catalogue ids (`pitch-shift-js` block).
#[derive(Params)]
struct ClassicParams {
    #[id = "shift"]
    shift: FloatParam,
    #[id = "window"]
    window: FloatParam,
    #[id = "overlap"]
    overlap: FloatParam,
    #[id = "wet"]
    wet: FloatParam,
    #[id = "dry"]
    dry: FloatParam,
    #[id = "search"]
    search: BoolParam,
}

impl Default for ClassicParams {
    fn default() -> Self {
        Self {
            shift: f(0, " st", 2),
            window: f(1, " ms", 0),
            overlap: f(2, " ms", 0),
            wet: f(3, " dB", 1),
            dry: f(4, " dB", 1),
            search: BoolParam::new("Splice Search", false),
        }
    }
}

impl ClassicParams {
    fn settings(&self) -> Settings {
        Settings {
            v: [
                self.shift.value(),
                self.window.value(),
                self.overlap.value(),
                self.wet.value(),
                self.dry.value(),
                if self.search.value() { 1.0 } else { 0.0 },
            ],
        }
    }
}

pub struct GfxPitchClassic {
    params: Arc<ClassicParams>,
    dsp: PitchClassic,
}

impl Default for GfxPitchClassic {
    fn default() -> Self {
        Self { params: Arc::new(ClassicParams::default()), dsp: PitchClassic::new() }
    }
}

impl Plugin for GfxPitchClassic {
    const NAME: &'static str = "GoleM FX Pitch Classic";
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

impl Vst3Plugin for GfxPitchClassic {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxPitchCl01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::PitchShift];
}

nih_export_vst3!(GfxPitchClassic);
