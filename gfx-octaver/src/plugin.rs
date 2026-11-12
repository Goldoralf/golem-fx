// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2. Linear ranges only (golem contract).
//! Generated from PARAMS (same order and ids).

use crate::{Octaver, Settings, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModeP {
    #[id = "vintage"]
    #[name = "Vintage"]
    Vintage,
    #[id = "clean"]
    #[name = "Clean"]
    Clean,
    #[id = "synth"]
    #[name = "Synth"]
    Synth,
    #[id = "poly"]
    #[name = "Poly"]
    Poly,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum InstrumentP {
    #[id = "guitar"]
    #[name = "Guitar"]
    Guitar,
    #[id = "bass"]
    #[name = "Bass"]
    Bass,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `direct`, `octave1`, `octave2` are golem's catalogue ids (`octaver` block).
#[derive(Params)]
struct OctaverParams {
    #[id = "direct"]
    direct: FloatParam,
    #[id = "octave1"]
    octave1: FloatParam,
    #[id = "octave2"]
    octave2: FloatParam,
    #[id = "octave_up"]
    octave_up: FloatParam,
    #[id = "mode"]
    mode: EnumParam<ModeP>,
    #[id = "instrument"]
    instrument: EnumParam<InstrumentP>,
    #[id = "sub_tone"]
    sub_tone: FloatParam,
    #[id = "up_tone"]
    up_tone: FloatParam,
    #[id = "sensitivity"]
    sensitivity: FloatParam,
    #[id = "glide"]
    glide: FloatParam,
    #[id = "shape"]
    shape: FloatParam,
    #[id = "range"]
    range: FloatParam,
    #[id = "level"]
    level: FloatParam,
}

impl Default for OctaverParams {
    fn default() -> Self {
        Self {
            direct: f(0, "", 2),
            octave1: f(1, "", 2),
            octave2: f(2, "", 2),
            octave_up: f(3, "", 2),
            mode: EnumParam::new("Mode", ModeP::Vintage),
            instrument: EnumParam::new("Instrument", InstrumentP::Guitar),
            sub_tone: f(6, " Hz", 0),
            up_tone: f(7, " Hz", 0),
            sensitivity: f(8, " %", 0),
            glide: f(9, " ms", 0),
            shape: f(10, " %", 0),
            range: f(11, " Hz", 0),
            level: f(12, " dB", 1),
        }
    }
}

impl OctaverParams {
    fn settings(&self) -> Settings {
        Settings {
            v: [
                self.direct.value(),
                self.octave1.value(),
                self.octave2.value(),
                self.octave_up.value(),
                self.mode.value() as usize as f32,
                self.instrument.value() as usize as f32,
                self.sub_tone.value(),
                self.up_tone.value(),
                self.sensitivity.value(),
                self.glide.value(),
                self.shape.value(),
                self.range.value(),
                self.level.value(),
            ],
        }
    }
}

pub struct GfxOctaver {
    params: Arc<OctaverParams>,
    dsp: Octaver,
}

impl Default for GfxOctaver {
    fn default() -> Self {
        Self { params: Arc::new(OctaverParams::default()), dsp: Octaver::new() }
    }
}

impl Plugin for GfxOctaver {
    const NAME: &'static str = "GoleM FX Octaver";
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

impl Vst3Plugin for GfxOctaver {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxOctaver01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::PitchShift];
}

nih_export_vst3!(GfxOctaver);
