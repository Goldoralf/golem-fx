// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second (the dry untouched, voice 1
//! centred, voice 2 panned). Linear ranges only (golem contract). Generated from PARAMS (same order and
//! ids). The dry path has no latency; the plugin reports 0.

use crate::{Golem, Settings, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum RegisterP {
    #[id = "low"]
    #[name = "Low"]
    Low,
    #[id = "mid"]
    #[name = "Mid"]
    Mid,
    #[id = "high"]
    #[name = "High"]
    High,
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

#[derive(Params)]
struct GolemParams {
    #[id = "shift"]
    shift: FloatParam,
    #[id = "formant"]
    formant: FloatParam,
    #[id = "voice1"]
    voice1: FloatParam,
    #[id = "lpf"]
    lpf: FloatParam,
    #[id = "shift2"]
    shift2: FloatParam,
    #[id = "voice2"]
    voice2: FloatParam,
    #[id = "pan"]
    pan: FloatParam,
    #[id = "fine"]
    fine: FloatParam,
    #[id = "humanize"]
    humanize: FloatParam,
    #[id = "formant_link"]
    formant_link: BoolParam,
    #[id = "glide"]
    glide: FloatParam,
    #[id = "sens"]
    sens: FloatParam,
    #[id = "thresh"]
    thresh: FloatParam,
    #[id = "register"]
    register: EnumParam<RegisterP>,
    #[id = "harmony"]
    harmony: EnumParam<HarmonyP>,
    #[id = "key"]
    key: EnumParam<KeyP>,
    #[id = "scale"]
    scale: EnumParam<ScaleP>,
    #[id = "hpf"]
    hpf: FloatParam,
    /// Read-only meter (golem contract): 0 idle, 1 tracking, 2 gated by level.
    #[id = "state"]
    state: FloatParam,
}

impl Default for GolemParams {
    fn default() -> Self {
        Self {
            shift: f(0, " st", 2),
            formant: f(1, " st", 2),
            voice1: f(2, " dB", 1),
            lpf: f(3, " Hz", 0),
            shift2: f(4, " st", 2),
            voice2: f(5, " dB", 1),
            pan: f(6, " %", 0),
            fine: f(7, " ct", 1),
            humanize: f(8, " %", 0),
            formant_link: BoolParam::new("Formant Follows Interval", false),
            glide: f(10, " ms", 0),
            sens: f(11, " %", 0),
            thresh: f(12, " dB", 1),
            register: EnumParam::new("Register", RegisterP::Mid),
            harmony: EnumParam::new("Harmony", HarmonyP::Off),
            key: EnumParam::new("Key", KeyP::C),
            scale: EnumParam::new("Scale", ScaleP::Major),
            hpf: f(17, " Hz", 0),
            state: FloatParam::new("Voice State", 0.0, FloatRange::Linear { min: 0.0, max: 2.0 }).read_only(),
        }
    }
}

impl GolemParams {
    fn settings(&self) -> Settings {
        Settings {
            shift: self.shift.value(),
            formant: self.formant.value(),
            voice1: self.voice1.value(),
            lpf: self.lpf.value(),
            shift2: self.shift2.value(),
            voice2: self.voice2.value(),
            pan: self.pan.value(),
            fine: self.fine.value(),
            humanize: self.humanize.value(),
            formant_link: self.formant_link.value(),
            glide: self.glide.value(),
            sens: self.sens.value(),
            thresh: self.thresh.value(),
            register: self.register.value() as usize,
            harmony: self.harmony.value() == HarmonyP::Scale,
            key: self.key.value() as u8,
            scale: self.scale.value() as usize,
            hpf: self.hpf.value(),
        }
    }
}

pub struct GfxGolem {
    params: Arc<GolemParams>,
    dsp: Golem,
}

impl Default for GfxGolem {
    fn default() -> Self {
        Self { params: Arc::new(GolemParams::default()), dsp: Golem::new() }
    }
}

impl Plugin for GfxGolem {
    const NAME: &'static str = "GoleM FX Golem";
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
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-golem"))
    }

    fn initialize(&mut self, layout: &AudioIOLayout, config: &BufferConfig, _ctx: &mut impl InitContext<Self>) -> bool {
        let channels = layout.main_output_channels.map_or(1, |c| c.get() as usize);
        self.dsp.set(self.params.settings());
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
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
        self.params.state.set_output_value(self.dsp.state() as f32);
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxGolem {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxGolem0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::PitchShift];
}

nih_export_vst3!(GfxGolem);
