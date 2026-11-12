// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): stereo 2 / 2 first (stereo by nature), mono 1 / 1. Linear ranges only (golem contract).
//! Generated from PARAMS (same order and ids).

use crate::{Chorus, Settings, PARAMS};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

gfx_dsp::sync_param_enum!();

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ProfileP {
    #[id = "guitar_clean"]
    #[name = "Guitar Clean"]
    GuitarClean,
    #[id = "guitar_drive"]
    #[name = "Guitar Drive"]
    GuitarDrive,
    #[id = "bass"]
    #[name = "Bass"]
    Bass,
    #[id = "keys"]
    #[name = "Keys"]
    Keys,
    #[id = "voice"]
    #[name = "Voice"]
    Voice,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModeP {
    #[id = "classic"]
    #[name = "Classic"]
    Classic,
    #[id = "dual_1"]
    #[name = "Dual I"]
    DualI,
    #[id = "dual_2"]
    #[name = "Dual II"]
    DualIi,
    #[id = "dual_12"]
    #[name = "Dual I+II"]
    DualIIi,
    #[id = "dimension_1"]
    #[name = "Dimension 1"]
    Dimension1,
    #[id = "dimension_2"]
    #[name = "Dimension 2"]
    Dimension2,
    #[id = "dimension_3"]
    #[name = "Dimension 3"]
    Dimension3,
    #[id = "ensemble"]
    #[name = "Ensemble"]
    Ensemble,
    #[id = "doubler"]
    #[name = "Doubler"]
    Doubler,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SlopeP {
    #[id = "12db"]
    #[name = "12 dB"]
    V12Db,
    #[id = "24db"]
    #[name = "24 dB"]
    V24Db,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum EngineP {
    #[id = "clean"]
    #[name = "Clean"]
    Clean,
    #[id = "bbd"]
    #[name = "BBD"]
    Bbd,
    #[id = "tape"]
    #[name = "Tape"]
    Tape,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum InterpP {
    #[id = "lagrange3"]
    #[name = "Lagrange 3"]
    Lagrange3,
    #[id = "lagrange5"]
    #[name = "Lagrange 5"]
    Lagrange5,
    #[id = "sinc"]
    #[name = "Sinc"]
    Sinc,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ChipP {
    #[id = "256"]
    #[name = "256"]
    V256,
    #[id = "512"]
    #[name = "512"]
    V512,
    #[id = "1024"]
    #[name = "1024"]
    V1024,
    #[id = "2048"]
    #[name = "2048"]
    V2048,
    #[id = "4096"]
    #[name = "4096"]
    V4096,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ShapeP {
    #[id = "sine"]
    #[name = "Sine"]
    Sine,
    #[id = "triangle"]
    #[name = "Triangle"]
    Triangle,
    #[id = "rounded"]
    #[name = "Rounded"]
    Rounded,
    #[id = "random"]
    #[name = "Random"]
    Random,
    #[id = "log"]
    #[name = "Log"]
    Log,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum VoicesP {
    #[id = "1"]
    #[name = "1"]
    V1,
    #[id = "2"]
    #[name = "2"]
    V2,
    #[id = "3"]
    #[name = "3"]
    V3,
    #[id = "4"]
    #[name = "4"]
    V4,
    #[id = "5"]
    #[name = "5"]
    V5,
    #[id = "6"]
    #[name = "6"]
    V6,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum MixLawP {
    #[id = "add"]
    #[name = "Add"]
    Add,
    #[id = "crossfade"]
    #[name = "Crossfade"]
    Crossfade,
    #[id = "equal_power"]
    #[name = "Equal Power"]
    EqualPower,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `rate`, `depth`, `mix` are golem's catalogue ids (`chorus` block).
#[derive(Params)]
struct ChorusParams {
    #[id = "profile"]
    profile: EnumParam<ProfileP>,
    #[id = "mode"]
    mode: EnumParam<ModeP>,
    #[id = "rate"]
    rate: FloatParam,
    #[id = "depth"]
    depth: FloatParam,
    #[id = "warmth"]
    warmth: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "xover_custom"]
    xover_custom: BoolParam,
    #[id = "xover_on"]
    xover_on: BoolParam,
    #[id = "xover_freq"]
    xover_freq: FloatParam,
    #[id = "xover_slope"]
    xover_slope: EnumParam<SlopeP>,
    #[id = "color_custom"]
    color_custom: BoolParam,
    #[id = "engine"]
    engine: EnumParam<EngineP>,
    #[id = "interp"]
    interp: EnumParam<InterpP>,
    #[id = "drive"]
    drive: FloatParam,
    #[id = "bbd_chip"]
    bbd_chip: EnumParam<ChipP>,
    #[id = "bbd_bw"]
    bbd_bw: FloatParam,
    #[id = "emphasis"]
    emphasis: FloatParam,
    #[id = "compander"]
    compander: FloatParam,
    #[id = "noise"]
    noise: FloatParam,
    #[id = "wow"]
    wow: FloatParam,
    #[id = "flutter"]
    flutter: FloatParam,
    #[id = "mod_custom"]
    mod_custom: BoolParam,
    #[id = "delay_base"]
    delay_base: FloatParam,
    #[id = "lfo_shape"]
    lfo_shape: EnumParam<ShapeP>,
    #[id = "voices"]
    voices: EnumParam<VoicesP>,
    #[id = "voice_spread"]
    voice_spread: FloatParam,
    #[id = "imperfection"]
    imperfection: FloatParam,
    #[id = "detune_lock"]
    detune_lock: BoolParam,
    #[id = "drift"]
    drift: FloatParam,
    #[id = "crossmix"]
    crossmix: FloatParam,
    #[id = "feedback"]
    feedback: FloatParam,
    #[id = "sync"]
    sync: EnumParam<SyncParam>,
    #[id = "env_depth"]
    env_depth: FloatParam,
    #[id = "env_mix"]
    env_mix: FloatParam,
    #[id = "stereo_custom"]
    stereo_custom: BoolParam,
    #[id = "stereo_phase"]
    stereo_phase: FloatParam,
    #[id = "width"]
    width: FloatParam,
    #[id = "mono_safe"]
    mono_safe: BoolParam,
    #[id = "filter_custom"]
    filter_custom: BoolParam,
    #[id = "wet_hpf"]
    wet_hpf: FloatParam,
    #[id = "wet_lpf"]
    wet_lpf: FloatParam,
    #[id = "mix_law"]
    mix_law: EnumParam<MixLawP>,
    #[id = "level_comp"]
    level_comp: BoolParam,
    #[id = "output"]
    output: FloatParam,
}

impl Default for ChorusParams {
    fn default() -> Self {
        Self {
            profile: EnumParam::new("Profile", ProfileP::GuitarClean),
            mode: EnumParam::new("Mode", ModeP::Classic),
            rate: f(2, " Hz", 2),
            depth: f(3, " ms", 2),
            warmth: f(4, " %", 0),
            mix: f(5, "", 2),
            xover_custom: BoolParam::new("Crossover Custom", false),
            xover_on: BoolParam::new("Crossover", false),
            xover_freq: f(8, " Hz", 0),
            xover_slope: EnumParam::new("Crossover Slope", SlopeP::V24Db),
            color_custom: BoolParam::new("Color Custom", false),
            engine: EnumParam::new("Engine", EngineP::Bbd),
            interp: EnumParam::new("Interpolation", InterpP::Lagrange5),
            drive: f(13, " %", 0),
            bbd_chip: EnumParam::new("BBD Chip", ChipP::V1024),
            bbd_bw: f(15, " Hz", 0),
            emphasis: f(16, " %", 0),
            compander: f(17, " %", 0),
            noise: f(18, " %", 0),
            wow: f(19, " %", 0),
            flutter: f(20, " %", 0),
            mod_custom: BoolParam::new("Modulation Custom", false),
            delay_base: f(22, " ms", 1),
            lfo_shape: EnumParam::new("LFO Shape", ShapeP::Triangle),
            voices: EnumParam::new("Voices", VoicesP::V1),
            voice_spread: f(25, "°", 0),
            imperfection: f(26, " %", 0),
            detune_lock: BoolParam::new("Detune Lock", false),
            drift: f(28, " %", 0),
            crossmix: f(29, " %", 0),
            feedback: f(30, " %", 0),
            sync: EnumParam::new("Sync", SyncParam::Off),
            env_depth: f(32, " %", 0),
            env_mix: f(33, " %", 0),
            stereo_custom: BoolParam::new("Stereo Custom", false),
            stereo_phase: f(35, "°", 0),
            width: f(36, " %", 0),
            mono_safe: BoolParam::new("Mono Safe", false),
            filter_custom: BoolParam::new("Filter Custom", false),
            wet_hpf: f(39, " Hz", 0),
            wet_lpf: f(40, " Hz", 0),
            mix_law: EnumParam::new("Mix Law", MixLawP::Add),
            level_comp: BoolParam::new("Level Comp", true),
            output: f(43, " dB", 1),
        }
    }
}

impl ChorusParams {
    fn settings(&self) -> Settings {
        Settings {
            v: [
                self.profile.value() as usize as f32,
                self.mode.value() as usize as f32,
                self.rate.value(),
                self.depth.value(),
                self.warmth.value(),
                self.mix.value(),
                if self.xover_custom.value() { 1.0 } else { 0.0 },
                if self.xover_on.value() { 1.0 } else { 0.0 },
                self.xover_freq.value(),
                self.xover_slope.value() as usize as f32,
                if self.color_custom.value() { 1.0 } else { 0.0 },
                self.engine.value() as usize as f32,
                self.interp.value() as usize as f32,
                self.drive.value(),
                self.bbd_chip.value() as usize as f32,
                self.bbd_bw.value(),
                self.emphasis.value(),
                self.compander.value(),
                self.noise.value(),
                self.wow.value(),
                self.flutter.value(),
                if self.mod_custom.value() { 1.0 } else { 0.0 },
                self.delay_base.value(),
                self.lfo_shape.value() as usize as f32,
                self.voices.value() as usize as f32,
                self.voice_spread.value(),
                self.imperfection.value(),
                if self.detune_lock.value() { 1.0 } else { 0.0 },
                self.drift.value(),
                self.crossmix.value(),
                self.feedback.value(),
                self.sync.value() as usize as f32,
                self.env_depth.value(),
                self.env_mix.value(),
                if self.stereo_custom.value() { 1.0 } else { 0.0 },
                self.stereo_phase.value(),
                self.width.value(),
                if self.mono_safe.value() { 1.0 } else { 0.0 },
                if self.filter_custom.value() { 1.0 } else { 0.0 },
                self.wet_hpf.value(),
                self.wet_lpf.value(),
                self.mix_law.value() as usize as f32,
                if self.level_comp.value() { 1.0 } else { 0.0 },
                self.output.value(),
            ],
        }
    }
}


pub struct GfxChorus {
    params: Arc<ChorusParams>,
    dsp: Chorus,
}

impl Default for GfxChorus {
    fn default() -> Self {
        Self { params: Arc::new(ChorusParams::default()), dsp: Chorus::new() }
    }
}

impl Plugin for GfxChorus {
    const NAME: &'static str = "GoleM FX Chorus";
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

impl Vst3Plugin for GfxChorus {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxChorus001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Modulation];
}

nih_export_vst3!(GfxChorus);
