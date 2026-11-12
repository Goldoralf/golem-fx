// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): stereo 2 / 2 first (stereo by nature), mono 1 / 1. Linear ranges only
//! (golem contract). Generated from PARAMS (same order and ids).

use crate::{Phaser, Settings, PARAMS};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

gfx_dsp::sync_param_enum!();

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModelP {
    #[id = "classic"]
    #[name = "Classic"]
    Classic,
    #[id = "script_90"]
    #[name = "Script 90"]
    Script90,
    #[id = "block_90"]
    #[name = "Block 90"]
    Block90,
    #[id = "stone"]
    #[name = "Stone"]
    Stone,
    #[id = "vibe"]
    #[name = "Vibe"]
    Vibe,
    #[id = "bi_phase"]
    #[name = "Bi-Phase"]
    BiPhase,
    #[id = "deep_12"]
    #[name = "Deep 12"]
    Deep12,
    #[id = "barber_up"]
    #[name = "Barber ↑"]
    BarberUp,
    #[id = "barber_down"]
    #[name = "Barber ↓"]
    BarberDown,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum StagesP {
    #[id = "2"]
    #[name = "2"]
    S2,
    #[id = "4"]
    #[name = "4"]
    S4,
    #[id = "6"]
    #[name = "6"]
    S6,
    #[id = "8"]
    #[name = "8"]
    S8,
    #[id = "10"]
    #[name = "10"]
    S10,
    #[id = "12"]
    #[name = "12"]
    S12,
    #[id = "16"]
    #[name = "16"]
    S16,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum PolarityP {
    #[id = "plus"]
    #[name = "+"]
    Plus,
    #[id = "minus"]
    #[name = "−"]
    Minus,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ShapeP {
    #[id = "sine"]
    #[name = "Sine"]
    Sine,
    #[id = "triangle"]
    #[name = "Triangle"]
    Triangle,
    #[id = "log"]
    #[name = "Log"]
    Log,
    #[id = "exp"]
    #[name = "Exp"]
    Exp,
    #[id = "square"]
    #[name = "Square"]
    Square,
    #[id = "ramp"]
    #[name = "Ramp"]
    Ramp,
    #[id = "random"]
    #[name = "Random"]
    Random,
    #[id = "steps"]
    #[name = "Steps"]
    Steps,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SourceP {
    #[id = "lfo"]
    #[name = "LFO"]
    Lfo,
    #[id = "envelope"]
    #[name = "Envelope"]
    Envelope,
    #[id = "manual"]
    #[name = "Manual"]
    Manual,
    #[id = "lfo_envelope"]
    #[name = "LFO + Envelope"]
    LfoEnvelope,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum Lfo2P {
    #[id = "same"]
    #[name = "Same"]
    Same,
    #[id = "reverse"]
    #[name = "Reverse"]
    Reverse,
    #[id = "free"]
    #[name = "Free"]
    Free,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum RoutingP {
    #[id = "series"]
    #[name = "Series"]
    Series,
    #[id = "parallel"]
    #[name = "Parallel"]
    Parallel,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum VibeModeP {
    #[id = "chorus"]
    #[name = "Chorus"]
    Chorus,
    #[id = "vibrato"]
    #[name = "Vibrato"]
    Vibrato,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `rate`, `low`, `high`, `feedback`, `wet` are golem's catalogue ids (`phaser-js` block).
#[derive(Params)]
struct PhaserParams {
    #[id = "rate"]
    rate: FloatParam,
    #[id = "low"]
    low: FloatParam,
    #[id = "high"]
    high: FloatParam,
    #[id = "feedback"]
    feedback: FloatParam,
    #[id = "wet"]
    wet: FloatParam,
    #[id = "model"]
    model: EnumParam<ModelP>,
    #[id = "stages"]
    stages: EnumParam<StagesP>,
    #[id = "spread"]
    spread: FloatParam,
    #[id = "fb_polarity"]
    fb_polarity: EnumParam<PolarityP>,
    #[id = "dry"]
    dry: FloatParam,
    #[id = "manual"]
    manual: FloatParam,
    #[id = "depth"]
    depth: FloatParam,
    #[id = "shape"]
    shape: EnumParam<ShapeP>,
    #[id = "skew"]
    skew: FloatParam,
    #[id = "sweep_src"]
    sweep_src: EnumParam<SourceP>,
    #[id = "env_sens"]
    env_sens: FloatParam,
    #[id = "env_att"]
    env_att: FloatParam,
    #[id = "env_rel"]
    env_rel: FloatParam,
    #[id = "stereo_phase"]
    stereo_phase: FloatParam,
    #[id = "sync"]
    sync: EnumParam<SyncParam>,
    #[id = "color"]
    color: FloatParam,
    #[id = "lfo2"]
    lfo2: EnumParam<Lfo2P>,
    #[id = "rate2"]
    rate2: FloatParam,
    #[id = "routing"]
    routing: EnumParam<RoutingP>,
    #[id = "lamp_lag"]
    lamp_lag: FloatParam,
    #[id = "vibe_mode"]
    vibe_mode: EnumParam<VibeModeP>,
    #[id = "tone"]
    tone: FloatParam,
    #[id = "bass_keep"]
    bass_keep: FloatParam,
}

impl Default for PhaserParams {
    fn default() -> Self {
        Self {
            rate: f(0, " Hz", 2),
            low: f(1, " Hz", 0),
            high: f(2, " Hz", 0),
            feedback: f(3, " dB", 1),
            wet: f(4, " dB", 1),
            model: EnumParam::new("Model", ModelP::Classic),
            stages: EnumParam::new("Stages", StagesP::S4),
            spread: f(7, " %", 0),
            fb_polarity: EnumParam::new("Feedback Polarity", PolarityP::Plus),
            dry: f(9, " dB", 1),
            manual: f(10, " %", 0),
            depth: f(11, " %", 0),
            shape: EnumParam::new("LFO Shape", ShapeP::Sine),
            skew: f(13, " %", 0),
            sweep_src: EnumParam::new("Sweep Source", SourceP::Lfo),
            env_sens: f(15, " %", 0),
            env_att: f(16, " ms", 0),
            env_rel: f(17, " ms", 0),
            stereo_phase: f(18, "°", 0),
            sync: EnumParam::new("Sync", SyncParam::Off),
            color: f(20, " %", 0),
            lfo2: EnumParam::new("Second LFO", Lfo2P::Same),
            rate2: f(22, " Hz", 2),
            routing: EnumParam::new("Routing", RoutingP::Series),
            lamp_lag: f(24, " ms", 0),
            vibe_mode: EnumParam::new("Vibe Mode", VibeModeP::Chorus),
            tone: f(26, " %", 0),
            bass_keep: f(27, " Hz", 0),
        }
    }
}

impl PhaserParams {
    fn settings(&self) -> Settings {
        Settings {
            v: [
                self.rate.value(),
                self.low.value(),
                self.high.value(),
                self.feedback.value(),
                self.wet.value(),
                self.model.value() as usize as f32,
                self.stages.value() as usize as f32,
                self.spread.value(),
                self.fb_polarity.value() as usize as f32,
                self.dry.value(),
                self.manual.value(),
                self.depth.value(),
                self.shape.value() as usize as f32,
                self.skew.value(),
                self.sweep_src.value() as usize as f32,
                self.env_sens.value(),
                self.env_att.value(),
                self.env_rel.value(),
                self.stereo_phase.value(),
                self.sync.value() as usize as f32,
                self.color.value(),
                self.lfo2.value() as usize as f32,
                self.rate2.value(),
                self.routing.value() as usize as f32,
                self.lamp_lag.value(),
                self.vibe_mode.value() as usize as f32,
                self.tone.value(),
                self.bass_keep.value(),
            ],
        }
    }
}

pub struct GfxPhaser {
    params: Arc<PhaserParams>,
    dsp: Phaser,
}

impl Default for GfxPhaser {
    fn default() -> Self {
        Self { params: Arc::new(PhaserParams::default()), dsp: Phaser::new() }
    }
}

impl Plugin for GfxPhaser {
    const NAME: &'static str = "GoleM FX Phaser";
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

impl Vst3Plugin for GfxPhaser {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxPhaser001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Modulation];
}

nih_export_vst3!(GfxPhaser);
