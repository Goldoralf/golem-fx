// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2. Linear ranges only (golem contract).

use crate::{Model, Settings, Vibrato, PARAMS};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

gfx_dsp::sync_param_enum!();

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModelParam {
    #[id = "clean"]
    Clean,
    #[id = "bbd"]
    #[name = "BBD"]
    Bbd,
    #[id = "varistor"]
    Varistor,
    #[id = "vibe"]
    Vibe,
    #[id = "scanner"]
    Scanner,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum Shape {
    #[id = "sine"]
    Sine,
    #[id = "rounded"]
    #[name = "Rounded Triangle"]
    Rounded,
    #[id = "random"]
    Random,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `depth`, `speed` are golem's catalogue ids (`vibrato` block).
#[derive(Params)]
struct VibratoParams {
    #[id = "depth"]
    depth: FloatParam,
    #[id = "speed"]
    speed: FloatParam,
    #[id = "model"]
    model: EnumParam<ModelParam>,
    #[id = "shape"]
    shape: EnumParam<Shape>,
    #[id = "rise"]
    rise: FloatParam,
    #[id = "tone"]
    tone: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "sync"]
    sync: EnumParam<SyncParam>,
    #[id = "stereo_phase"]
    stereo_phase: FloatParam,
    #[id = "drive"]
    drive: FloatParam,
    #[id = "level"]
    level: FloatParam,
}

impl Default for VibratoParams {
    fn default() -> Self {
        Self {
            depth: f(0, "", 2),
            speed: f(1, " Hz", 2),
            model: EnumParam::new("Model", ModelParam::Clean),
            shape: EnumParam::new("Shape", Shape::Sine),
            rise: f(4, " ms", 0),
            tone: f(5, "", 2),
            mix: f(6, "", 2),
            sync: EnumParam::new("Sync", SyncParam::Off),
            stereo_phase: f(8, "°", 0),
            drive: f(9, "", 2),
            level: f(10, " dB", 1),
        }
    }
}

impl VibratoParams {
    fn settings(&self) -> Settings {
        Settings {
            depth: self.depth.value(),
            speed: self.speed.value(),
            model: Model::ALL[self.model.value() as usize],
            shape: self.shape.value() as u32,
            rise: self.rise.value(),
            tone: self.tone.value(),
            mix: self.mix.value(),
            sync: self.sync.value() as u32,
            stereo_phase: self.stereo_phase.value(),
            drive: self.drive.value(),
            level: self.level.value(),
        }
    }
}

pub struct GfxVibrato {
    params: Arc<VibratoParams>,
    dsp: Vibrato,
}

impl Default for GfxVibrato {
    fn default() -> Self {
        Self { params: Arc::new(VibratoParams::default()), dsp: Vibrato::new() }
    }
}

impl Plugin for GfxVibrato {
    const NAME: &'static str = "GoleM FX Vibrato";
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

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let s = self.params.settings();
        if s != self.dsp.settings() {
            self.dsp.set(s);
        }
        let t = ctx.transport();
        self.dsp.set_transport(TransportInfo { tempo: t.tempo, pos_beats: t.pos_beats(), playing: t.playing });
        self.dsp.process(buffer.as_slice());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxVibrato {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxVibrato01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Modulation];
}

nih_export_vst3!(GfxVibrato);
