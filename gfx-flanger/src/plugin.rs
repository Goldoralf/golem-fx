// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): stereo 2 / 2 first (stereo by nature), mono 1 / 1. Linear ranges only
//! (golem contract). Tape TZ reports its 1 ms latency, at initialisation and when the mode changes.
//! Generated from PARAMS (same order and ids).

use crate::{Flanger, Settings, PARAMS};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

gfx_dsp::sync_param_enum!();

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModeP {
    #[id = "mistress"]
    #[name = "Mistress"]
    Mistress,
    #[id = "grey"]
    #[name = "Grey"]
    Grey,
    #[id = "ada"]
    #[name = "A/DA"]
    Ada,
    #[id = "tape_tz_plus"]
    #[name = "Tape TZ+"]
    TapeTzPlus,
    #[id = "tape_tz_minus"]
    #[name = "Tape TZ−"]
    TapeTzMinus,
    #[id = "pseudo_tz"]
    #[name = "Pseudo TZ"]
    PseudoTz,
    #[id = "barber_up"]
    #[name = "Barberpole ↑"]
    BarberUp,
    #[id = "barber_down"]
    #[name = "Barberpole ↓"]
    BarberDown,
    #[id = "pedal"]
    #[name = "Pedal"]
    Pedal,
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
    #[id = "mode"]
    #[name = "Mode"]
    Mode,
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
    #[id = "ramp_up"]
    #[name = "Ramp Up"]
    RampUp,
    #[id = "ramp_down"]
    #[name = "Ramp Down"]
    RampDown,
    #[id = "random"]
    #[name = "Random"]
    Random,
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

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `length`, `feedback`, `wet`, `dry`, `rate` are golem's catalogue ids (`flanger-js` block).
#[derive(Params)]
struct FlangerParams {
    #[id = "length"]
    length: FloatParam,
    #[id = "feedback"]
    feedback: FloatParam,
    #[id = "wet"]
    wet: FloatParam,
    #[id = "dry"]
    dry: FloatParam,
    #[id = "rate"]
    rate: FloatParam,
    #[id = "mode"]
    mode: EnumParam<ModeP>,
    #[id = "manual"]
    manual: FloatParam,
    #[id = "depth"]
    depth: FloatParam,
    #[id = "fb_polarity"]
    fb_polarity: EnumParam<PolarityP>,
    #[id = "shape"]
    shape: EnumParam<ShapeP>,
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
    #[id = "drive"]
    drive: FloatParam,
    #[id = "fb_hpf"]
    fb_hpf: FloatParam,
    #[id = "fb_lpf"]
    fb_lpf: FloatParam,
    #[id = "bass_keep"]
    bass_keep: FloatParam,
}

impl Default for FlangerParams {
    fn default() -> Self {
        Self {
            length: f(0, " ms", 2),
            feedback: f(1, " dB", 1),
            wet: f(2, " dB", 1),
            dry: f(3, " dB", 1),
            rate: f(4, " Hz", 2),
            mode: EnumParam::new("Mode", ModeP::Mistress),
            manual: f(6, " ms", 2),
            depth: f(7, " %", 0),
            fb_polarity: EnumParam::new("Feedback Polarity", PolarityP::Plus),
            shape: EnumParam::new("LFO Shape", ShapeP::Mode),
            sweep_src: EnumParam::new("Sweep Source", SourceP::Lfo),
            env_sens: f(11, " dB", 1),
            env_att: f(12, " ms", 0),
            env_rel: f(13, " ms", 0),
            stereo_phase: f(14, "°", 0),
            sync: EnumParam::new("Sync", SyncParam::Off),
            drive: f(16, " %", 0),
            fb_hpf: f(17, " Hz", 0),
            fb_lpf: f(18, " Hz", 0),
            bass_keep: f(19, " Hz", 0),
        }
    }
}

impl FlangerParams {
    fn settings(&self) -> Settings {
        Settings {
            v: [
                self.length.value(),
                self.feedback.value(),
                self.wet.value(),
                self.dry.value(),
                self.rate.value(),
                self.mode.value() as usize as f32,
                self.manual.value(),
                self.depth.value(),
                self.fb_polarity.value() as usize as f32,
                self.shape.value() as usize as f32,
                self.sweep_src.value() as usize as f32,
                self.env_sens.value(),
                self.env_att.value(),
                self.env_rel.value(),
                self.stereo_phase.value(),
                self.sync.value() as usize as f32,
                self.drive.value(),
                self.fb_hpf.value(),
                self.fb_lpf.value(),
                self.bass_keep.value(),
            ],
        }
    }
}

pub struct GfxFlanger {
    params: Arc<FlangerParams>,
    dsp: Flanger,
    reported: u32,
}

impl Default for GfxFlanger {
    fn default() -> Self {
        Self { params: Arc::new(FlangerParams::default()), dsp: Flanger::new(), reported: 0 }
    }
}

impl Plugin for GfxFlanger {
    const NAME: &'static str = "GoleM FX Flanger";
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
        let t = ctx.transport();
        self.dsp.set_transport(TransportInfo { tempo: t.tempo, pos_beats: t.pos_beats(), playing: t.playing });
        self.dsp.process(buffer.as_slice());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxFlanger {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxFlanger01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Modulation];
}

nih_export_vst3!(GfxFlanger);
