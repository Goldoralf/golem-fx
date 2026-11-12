// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2. Linear ranges only (golem contract).

use crate::{Drive, Settings, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum VoiceP {
    #[id = "overdrive"]
    Overdrive,
    #[id = "distortion"]
    Distortion,
    #[id = "fuzz"]
    Fuzz,
    #[id = "legacy"]
    Legacy,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `gain`, `hardness`, `ceiling` are golem's catalogue ids (`guitar-drive` block).
#[derive(Params)]
struct DriveParams {
    #[id = "gain"]
    gain: FloatParam,
    #[id = "hardness"]
    hardness: FloatParam,
    #[id = "ceiling"]
    ceiling: FloatParam,
    #[id = "voice"]
    voice: EnumParam<VoiceP>,
    #[id = "tight"]
    tight: FloatParam,
    #[id = "tone"]
    tone: FloatParam,
    #[id = "asym"]
    asym: FloatParam,
    #[id = "sag"]
    sag: FloatParam,
    #[id = "blend"]
    blend: FloatParam,
    #[id = "bass_keep"]
    bass_keep: FloatParam,
}

impl Default for DriveParams {
    fn default() -> Self {
        Self {
            gain: f(0, " dB", 1),
            hardness: f(1, "", 1),
            ceiling: f(2, " dB", 1),
            voice: EnumParam::new("Voice", VoiceP::Overdrive),
            tight: f(4, " Hz", 0),
            tone: f(5, " %", 0),
            asym: f(6, " %", 0),
            sag: f(7, " %", 0),
            blend: f(8, " %", 0),
            bass_keep: f(9, " Hz", 0),
        }
    }
}

impl DriveParams {
    fn settings(&self) -> Settings {
        Settings {
            v: [
                self.gain.value(),
                self.hardness.value(),
                self.ceiling.value(),
                self.voice.value() as usize as f32,
                self.tight.value(),
                self.tone.value(),
                self.asym.value(),
                self.sag.value(),
                self.blend.value(),
                self.bass_keep.value(),
            ],
        }
    }
}

pub struct GfxDrive {
    params: Arc<DriveParams>,
    dsp: Drive,
}

impl Default for GfxDrive {
    fn default() -> Self {
        Self { params: Arc::new(DriveParams::default()), dsp: Drive::new() }
    }
}

impl Plugin for GfxDrive {
    const NAME: &'static str = "GoleM FX Drive";
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

impl Vst3Plugin for GfxDrive {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxDrive0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_vst3!(GfxDrive);
