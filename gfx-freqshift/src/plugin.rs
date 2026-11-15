// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): stereo 2 / 2 first, mono 1 / 1 (stereo by nature: golem turns the lane
//! stereo). Linear ranges only (golem contract). `bypass` is the VST3 bypass parameter: with `trails`
//! the loop rings out. Zero latency. Generated from PARAMS (same order and ids).

use crate::{FreqShift, NP, PARAMS};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

gfx_dsp::sync_param_enum!();

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum LoopP {
    #[id = "delay"]
    #[name = "Delay"]
    Delay,
    #[id = "phaser"]
    #[name = "Phaser"]
    Phaser,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ShapeP {
    #[id = "sine"]
    #[name = "Sine"]
    Sine,
    #[id = "triangle"]
    #[name = "Triangle"]
    Triangle,
    #[id = "saw-up"]
    #[name = "Saw Up"]
    SawUp,
    #[id = "saw-down"]
    #[name = "Saw Down"]
    SawDown,
    #[id = "square"]
    #[name = "Square"]
    Square,
    #[id = "random-steps"]
    #[name = "Random Steps"]
    RandomSteps,
    #[id = "random-smooth"]
    #[name = "Random Smooth"]
    RandomSmooth,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `shift`, `shift_r`, `time`, `fbk`, `mix` are golem's catalogue ids (`surge-freqshift` block).
#[derive(Params)]
struct FreqShiftParams {
    #[id = "shift"]
    shift: FloatParam,
    #[id = "fbk"]
    fbk: FloatParam,
    #[id = "time"]
    time: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "shift_r"]
    shift_r: FloatParam,
    #[id = "loop"]
    loop_: EnumParam<LoopP>,
    #[id = "sideband"]
    sideband: FloatParam,
    #[id = "zero_cut"]
    zero_cut: BoolParam,
    #[id = "dry_match"]
    dry_match: BoolParam,
    #[id = "time_sync"]
    time_sync: EnumParam<SyncParam>,
    #[id = "low_cut"]
    low_cut: FloatParam,
    #[id = "high_cut"]
    high_cut: FloatParam,
    #[id = "lfo_depth"]
    lfo_depth: FloatParam,
    #[id = "rate"]
    rate: FloatParam,
    #[id = "sync"]
    sync: EnumParam<SyncParam>,
    #[id = "lfo_shape"]
    lfo_shape: EnumParam<ShapeP>,
    #[id = "stereo_phase"]
    stereo_phase: FloatParam,
    #[id = "env_depth"]
    env_depth: FloatParam,
    #[id = "env_sens"]
    env_sens: FloatParam,
    #[id = "env_att"]
    env_att: FloatParam,
    #[id = "env_rel"]
    env_rel: FloatParam,
    #[id = "bypass"]
    bypass: BoolParam,
    #[id = "trails"]
    trails: BoolParam,
}

impl Default for FreqShiftParams {
    fn default() -> Self {
        Self {
            shift: f(0, " Hz", 2),
            fbk: f(1, " %", 0),
            time: f(2, " ms", 1),
            mix: f(3, " %", 0),
            shift_r: f(4, " %", 0),
            loop_: EnumParam::new("Loop", LoopP::Delay),
            sideband: f(6, " %", 0),
            zero_cut: BoolParam::new("Zero Cut", true),
            dry_match: BoolParam::new("Dry Match", false),
            time_sync: EnumParam::new("Time Sync", SyncParam::Off),
            low_cut: f(10, " Hz", 0),
            high_cut: f(11, " Hz", 0),
            lfo_depth: f(12, " Hz", 1),
            rate: f(13, " Hz", 2),
            sync: EnumParam::new("LFO Sync", SyncParam::Off),
            lfo_shape: EnumParam::new("LFO Shape", ShapeP::Sine),
            stereo_phase: f(16, "°", 0),
            env_depth: f(17, " Hz", 0),
            env_sens: f(18, " %", 0),
            env_att: f(19, " ms", 0),
            env_rel: f(20, " ms", 0),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            trails: BoolParam::new("Trails", true),
        }
    }
}

impl FreqShiftParams {
    fn values(&self) -> [f32; NP] {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        [
            self.shift.value(),
            self.fbk.value(),
            self.time.value(),
            self.mix.value(),
            self.shift_r.value(),
            self.loop_.value() as usize as f32,
            self.sideband.value(),
            b(self.zero_cut.value()),
            b(self.dry_match.value()),
            self.time_sync.value() as usize as f32,
            self.low_cut.value(),
            self.high_cut.value(),
            self.lfo_depth.value(),
            self.rate.value(),
            self.sync.value() as usize as f32,
            self.lfo_shape.value() as usize as f32,
            self.stereo_phase.value(),
            self.env_depth.value(),
            self.env_sens.value(),
            self.env_att.value(),
            self.env_rel.value(),
            b(self.bypass.value()),
            b(self.trails.value()),
        ]
    }
}

pub struct GfxFreqShift {
    params: Arc<FreqShiftParams>,
    dsp: FreqShift,
    last: [f32; NP],
}

impl Default for GfxFreqShift {
    fn default() -> Self {
        Self { params: Arc::new(FreqShiftParams::default()), dsp: FreqShift::new(), last: [f32::NAN; NP] }
    }
}

impl GfxFreqShift {
    fn sync(&mut self, force: bool) {
        let v = self.params.values();
        for (k, p) in PARAMS.iter().enumerate() {
            if force || v[k] != self.last[k] {
                self.last[k] = v[k];
                self.dsp.set_param(p.id, v[k]);
            }
        }
    }
}

impl Plugin for GfxFreqShift {
    const NAME: &'static str = "GoleM FX Freqshift";
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

    fn initialize(&mut self, layout: &AudioIOLayout, config: &BufferConfig, ctx: &mut impl InitContext<Self>) -> bool {
        let channels = layout.main_output_channels.map_or(1, |c| c.get() as usize);
        self.sync(true);
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
        ctx.set_latency_samples(0);
        true
    }

    fn reset(&mut self) {
        self.sync(true);
        self.dsp.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.sync(false);
        let t = ctx.transport();
        self.dsp.set_transport(TransportInfo { tempo: t.tempo, pos_beats: t.pos_beats(), playing: t.playing });
        self.dsp.process(buffer.as_slice());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxFreqShift {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxFreqSh001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::PitchShift];
}

nih_export_vst3!(GfxFreqShift);
