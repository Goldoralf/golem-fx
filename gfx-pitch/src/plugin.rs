// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2. Linear ranges only (golem contract).
//! The engine's fixed latency is reported at initialisation and on an engine / tracking change only.

use crate::{Pitch, Settings, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum EngineP {
    #[id = "whammy"]
    #[name = "Whammy"]
    Whammy,
    #[id = "poly"]
    #[name = "Poly"]
    Poly,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum TrackingP {
    #[id = "fast"]
    #[name = "Fast"]
    Fast,
    #[id = "stable"]
    #[name = "Stable"]
    Stable,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum HarmonyP {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "scale"]
    #[name = "Scale"]
    Scale,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum KeyP {
    #[id = "c"]
    #[name = "C"]
    C,
    #[id = "c#"]
    #[name = "C#"]
    Cs,
    #[id = "d"]
    #[name = "D"]
    D,
    #[id = "d#"]
    #[name = "Eb"]
    Ds,
    #[id = "e"]
    #[name = "E"]
    E,
    #[id = "f"]
    #[name = "F"]
    F,
    #[id = "f#"]
    #[name = "F#"]
    Fs,
    #[id = "g"]
    #[name = "G"]
    G,
    #[id = "g#"]
    #[name = "Ab"]
    Gs,
    #[id = "a"]
    #[name = "A"]
    A,
    #[id = "a#"]
    #[name = "Bb"]
    As,
    #[id = "b"]
    #[name = "B"]
    B,
}

/// gfx_dsp::detect::SCALES' order: grows at the end only.
#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ScaleP {
    #[id = "major"]
    #[name = "Major"]
    Major,
    #[id = "minor"]
    #[name = "Minor"]
    Minor,
    #[id = "harmonic_minor"]
    #[name = "Harmonic Minor"]
    HarmonicMinor,
    #[id = "melodic_minor"]
    #[name = "Melodic Minor"]
    MelodicMinor,
    #[id = "dorian"]
    #[name = "Dorian"]
    Dorian,
    #[id = "phrygian"]
    #[name = "Phrygian"]
    Phrygian,
    #[id = "lydian"]
    #[name = "Lydian"]
    Lydian,
    #[id = "mixolydian"]
    #[name = "Mixolydian"]
    Mixolydian,
    #[id = "major_pentatonic"]
    #[name = "Major Pentatonic"]
    MajorPentatonic,
    #[id = "minor_pentatonic"]
    #[name = "Minor Pentatonic"]
    MinorPentatonic,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `shift`, `mix` are golem's catalogue ids (`pitch` block).
#[derive(Params)]
struct PitchParams {
    #[id = "shift"]
    shift: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "engine"]
    engine: EnumParam<EngineP>,
    #[id = "tracking"]
    tracking: EnumParam<TrackingP>,
    #[id = "glide"]
    glide: FloatParam,
    #[id = "shift2"]
    shift2: FloatParam,
    #[id = "voice2"]
    voice2: FloatParam,
    #[id = "fine"]
    fine: FloatParam,
    #[id = "harmony"]
    harmony: EnumParam<HarmonyP>,
    #[id = "key"]
    key: EnumParam<KeyP>,
    #[id = "scale"]
    scale: EnumParam<ScaleP>,
}

impl Default for PitchParams {
    fn default() -> Self {
        Self {
            shift: f(0, " st", 2),
            mix: f(1, " %", 0),
            engine: EnumParam::new("Engine", EngineP::Whammy),
            tracking: EnumParam::new("Tracking", TrackingP::Fast),
            glide: f(4, " ms", 0),
            shift2: f(5, " st", 2),
            voice2: f(6, " dB", 1),
            fine: f(7, " ct", 0),
            harmony: EnumParam::new("Harmony", HarmonyP::Off),
            key: EnumParam::new("Key", KeyP::C),
            scale: EnumParam::new("Scale", ScaleP::Major),
        }
    }
}

impl PitchParams {
    fn settings(&self) -> Settings {
        Settings {
            v: [
                self.shift.value(),
                self.mix.value(),
                self.engine.value() as usize as f32,
                self.tracking.value() as usize as f32,
                self.glide.value(),
                self.shift2.value(),
                self.voice2.value(),
                self.fine.value(),
                self.harmony.value() as usize as f32,
                self.key.value() as usize as f32,
                self.scale.value() as usize as f32,
            ],
        }
    }
}

pub struct GfxPitch {
    params: Arc<PitchParams>,
    dsp: Pitch,
    reported: u32,
}

impl Default for GfxPitch {
    fn default() -> Self {
        Self { params: Arc::new(PitchParams::default()), dsp: Pitch::new(), reported: 0 }
    }
}

impl Plugin for GfxPitch {
    const NAME: &'static str = "GoleM FX Pitch";
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
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-pitch"))
    }

    fn initialize(&mut self, layout: &AudioIOLayout, config: &BufferConfig, _ctx: &mut impl InitContext<Self>) -> bool {
        let channels = layout.main_output_channels.map_or(1, |c| c.get() as usize);
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
        self.dsp.set(self.params.settings());
        self.reported = self.dsp.latency_samples() as u32;
        _ctx.set_latency_samples(self.reported);
        true
    }

    fn reset(&mut self) {
        self.dsp.set(self.params.settings());
        self.dsp.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let s = self.params.settings();
        if s != self.dsp.settings() {
            self.dsp.set(s);
            let l = self.dsp.latency_samples() as u32;
            if l != self.reported {
                self.reported = l;
                ctx.set_latency_samples(l);
            }
        }
        self.dsp.process(buffer.as_slice());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxPitch {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxPitch0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::PitchShift];
}

nih_export_vst3!(GfxPitch);
