// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): stereo 2 / 2 first, mono 1 / 1. Linear ranges only (golem contract).
//! `bypass` is the VST3 bypass parameter: the pad rings out (trails). Generated from PARAMS.

use crate::{Settings, Shimmer, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum PlacementP {
    #[id = "regen"]
    #[name = "Regen"]
    Regen,
    #[id = "input"]
    #[name = "Input"]
    Input,
    #[id = "both"]
    #[name = "Both"]
    Both,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum CharacterP {
    #[id = "grainy"]
    #[name = "Grainy"]
    Grainy,
    #[id = "smooth"]
    #[name = "Smooth"]
    Smooth,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SpaceP {
    #[id = "hall"]
    #[name = "Hall"]
    Hall,
    #[id = "cathedral"]
    #[name = "Cathedral"]
    Cathedral,
    #[id = "cloud"]
    #[name = "Cloud"]
    Cloud,
    #[id = "plate"]
    #[name = "Plate"]
    Plate,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum FreezeP {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "freeze"]
    #[name = "Freeze"]
    Freeze,
    #[id = "infinite"]
    #[name = "Infinite"]
    Infinite,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `mix`, `decay`, `shimmer`, `pitch`, `hicut` are golem's catalogue ids (`reverb-shimmer` block).
#[derive(Params)]
struct ShimmerParams {
    #[id = "mix"]
    mix: FloatParam,
    #[id = "decay"]
    decay: FloatParam,
    #[id = "shimmer"]
    shimmer: FloatParam,
    #[id = "pitch"]
    pitch: FloatParam,
    #[id = "hicut"]
    hicut: FloatParam,
    #[id = "pitch2"]
    pitch2: FloatParam,
    #[id = "voice2"]
    voice2: FloatParam,
    #[id = "detune"]
    detune: FloatParam,
    #[id = "lowcut"]
    lowcut: FloatParam,
    #[id = "placement"]
    placement: EnumParam<PlacementP>,
    #[id = "size"]
    size: FloatParam,
    #[id = "predelay"]
    predelay: FloatParam,
    #[id = "bloom"]
    bloom: FloatParam,
    #[id = "moddepth"]
    moddepth: FloatParam,
    #[id = "character"]
    character: EnumParam<CharacterP>,
    #[id = "space"]
    space: EnumParam<SpaceP>,
    #[id = "mod_rate"]
    mod_rate: FloatParam,
    #[id = "swell"]
    swell: FloatParam,
    #[id = "voice_pan"]
    voice_pan: FloatParam,
    #[id = "bass_mult"]
    bass_mult: FloatParam,
    #[id = "tone"]
    tone: FloatParam,
    #[id = "glide"]
    glide: FloatParam,
    #[id = "duck"]
    duck: FloatParam,
    #[id = "freeze"]
    freeze: EnumParam<FreezeP>,
    #[id = "width"]
    width: FloatParam,
    #[id = "kill_dry"]
    kill_dry: BoolParam,
    #[id = "trails"]
    trails: BoolParam,
    #[id = "bypass"]
    bypass: BoolParam,
    #[id = "wet_level"]
    wet_level: FloatParam,
}

impl Default for ShimmerParams {
    fn default() -> Self {
        Self {
            mix: f(0, " %", 0),
            decay: f(1, " s", 2),
            shimmer: f(2, " %", 0),
            pitch: f(3, " st", 1),
            hicut: f(4, " Hz", 0),
            pitch2: f(5, " st", 1),
            voice2: f(6, " %", 0),
            detune: f(7, " ct", 0),
            lowcut: f(8, " Hz", 0),
            placement: EnumParam::new("Placement", PlacementP::Regen),
            size: f(10, " %", 0),
            predelay: f(11, " ms", 0),
            bloom: f(12, " %", 0),
            moddepth: f(13, " %", 0),
            character: EnumParam::new("Character", CharacterP::Grainy),
            space: EnumParam::new("Space", SpaceP::Hall),
            mod_rate: f(16, " %", 0),
            swell: f(17, " ms", 0),
            voice_pan: f(18, " %", 0),
            bass_mult: f(19, "×", 2),
            tone: f(20, " %", 0),
            glide: f(21, " ms", 0),
            duck: f(22, " %", 0),
            freeze: EnumParam::new("Freeze", FreezeP::Off),
            width: f(24, " %", 0),
            kill_dry: BoolParam::new("Kill Dry", false),
            trails: BoolParam::new("Trails", true),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            wet_level: f(28, " dB", 1),
        }
    }
}

impl ShimmerParams {
    fn settings(&self) -> Settings {
        Settings {
            v: [
                self.mix.value(),
                self.decay.value(),
                self.shimmer.value(),
                self.pitch.value(),
                self.hicut.value(),
                self.pitch2.value(),
                self.voice2.value(),
                self.detune.value(),
                self.lowcut.value(),
                self.placement.value() as usize as f32,
                self.size.value(),
                self.predelay.value(),
                self.bloom.value(),
                self.moddepth.value(),
                self.character.value() as usize as f32,
                self.space.value() as usize as f32,
                self.mod_rate.value(),
                self.swell.value(),
                self.voice_pan.value(),
                self.bass_mult.value(),
                self.tone.value(),
                self.glide.value(),
                self.duck.value(),
                self.freeze.value() as usize as f32,
                self.width.value(),
                if self.kill_dry.value() { 1.0 } else { 0.0 },
                if self.trails.value() { 1.0 } else { 0.0 },
                if self.bypass.value() { 1.0 } else { 0.0 },
                self.wet_level.value(),
            ],
        }
    }
}

pub struct GfxShimmer {
    params: Arc<ShimmerParams>,
    dsp: Shimmer,
}

impl Default for GfxShimmer {
    fn default() -> Self {
        Self { params: Arc::new(ShimmerParams::default()), dsp: Shimmer::new() }
    }
}

impl Plugin for GfxShimmer {
    const NAME: &'static str = "GoleM FX Shimmer";
    const VENDOR: &'static str = "GoleM";
    const URL: &'static str = "https://golem.fm";
    const EMAIL: &'static str = "contact@golem.fm";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        // stereo by nature: 2 / 2 first, so golem turns the lane stereo here (a mono DI arrives on both inputs)
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

impl Vst3Plugin for GfxShimmer {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxShimmer01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Reverb];
}

nih_export_vst3!(GfxShimmer);
