// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2. Linear ranges only (golem contract).

use crate::{Model, Settings, Tremolo, PARAMS};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

gfx_dsp::sync_param_enum!();

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum Wave {
    #[id = "sine"]
    Sine,
    #[id = "triangle"]
    Triangle,
    #[id = "square"]
    Square,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModelParam {
    #[id = "clean"]
    Clean,
    #[id = "bias"]
    Bias,
    #[id = "optical"]
    Optical,
    #[id = "neon"]
    Neon,
    #[id = "harmonic"]
    Harmonic,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `rate`, `depth`, `wave` are golem's catalogue ids (`tremolo` block).
#[derive(Params)]
struct TremoloParams {
    #[id = "rate"]
    rate: FloatParam,
    #[id = "depth"]
    depth: FloatParam,
    #[id = "wave"]
    wave: EnumParam<Wave>,
    #[id = "model"]
    model: EnumParam<ModelParam>,
    #[id = "sync"]
    sync: EnumParam<SyncParam>,
    #[id = "duty"]
    duty: FloatParam,
    #[id = "xover"]
    xover: FloatParam,
    #[id = "blend"]
    blend: FloatParam,
    #[id = "drive"]
    drive: FloatParam,
    #[id = "duck"]
    duck: FloatParam,
    #[id = "stereo_phase"]
    stereo_phase: FloatParam,
    #[id = "level"]
    level: FloatParam,
}

impl Default for TremoloParams {
    fn default() -> Self {
        Self {
            rate: f(0, " Hz", 2),
            depth: f(1, "", 2),
            wave: EnumParam::new("Wave", Wave::Sine),
            model: EnumParam::new("Model", ModelParam::Clean),
            sync: EnumParam::new("Sync", SyncParam::Off),
            duty: f(5, "", 2),
            xover: f(6, " Hz", 0),
            blend: f(7, "", 2),
            drive: f(8, "", 2),
            duck: f(9, "", 2),
            stereo_phase: f(10, "°", 0),
            level: f(11, " dB", 1),
        }
    }
}

impl TremoloParams {
    fn settings(&self) -> Settings {
        Settings {
            rate: self.rate.value(),
            depth: self.depth.value(),
            wave: self.wave.value() as u32,
            model: Model::ALL[self.model.value() as usize],
            sync: self.sync.value() as u32,
            duty: self.duty.value(),
            xover: self.xover.value(),
            blend: self.blend.value(),
            drive: self.drive.value(),
            duck: self.duck.value(),
            stereo_phase: self.stereo_phase.value(),
            level: self.level.value(),
        }
    }
}

pub struct GfxTremolo {
    params: Arc<TremoloParams>,
    dsp: Tremolo,
}

impl Default for GfxTremolo {
    fn default() -> Self {
        Self { params: Arc::new(TremoloParams::default()), dsp: Tremolo::new() }
    }
}

impl Plugin for GfxTremolo {
    const NAME: &'static str = "GoleM FX Tremolo";
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

impl Vst3Plugin for GfxTremolo {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxTremolo01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Modulation];
}

nih_export_vst3!(GfxTremolo);
