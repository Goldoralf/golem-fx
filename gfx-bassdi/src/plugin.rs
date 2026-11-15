// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second (two engines, the compressor
//! linked, the `bass_keep` band mono). Linear ranges only (golem contract). `bypass` is the VST3
//! bypass parameter (a 10 ms crossfade); `gr` is a read-only meter. Zero latency. Generated from
//! PARAMS (same order and ids).

use crate::{BassDi, NP, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModelP {
    #[id = "modern"]
    #[name = "Modern"]
    Modern,
    #[id = "classic"]
    #[name = "Classic"]
    Classic,
    #[id = "vintage"]
    #[name = "Vintage"]
    Vintage,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum CabP {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "smooth"]
    #[name = "Smooth"]
    Smooth,
    #[id = "8x10"]
    #[name = "8×10"]
    Ten8,
    #[id = "4x10"]
    #[name = "4×10"]
    Ten4,
    #[id = "1x15"]
    #[name = "1×15"]
    Fifteen1,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum CompCharP {
    #[id = "fet"]
    #[name = "FET"]
    Fet,
    #[id = "opto"]
    #[name = "Opto"]
    Opto,
    #[id = "clean"]
    #[name = "Clean"]
    Clean,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum CompPathP {
    #[id = "dirt"]
    #[name = "Dirt"]
    Dirt,
    #[id = "both"]
    #[name = "Both"]
    Both,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

#[derive(Params)]
struct BassDiParams {
    #[id = "gain"]
    gain: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "level"]
    level: FloatParam,
    #[id = "bass"]
    bass: FloatParam,
    #[id = "lo_mid"]
    lo_mid: FloatParam,
    #[id = "hi_mid"]
    hi_mid: FloatParam,
    #[id = "treble"]
    treble: FloatParam,
    #[id = "model"]
    model: EnumParam<ModelP>,
    #[id = "cab"]
    cab: EnumParam<CabP>,
    #[id = "grunt"]
    grunt: FloatParam,
    #[id = "attack"]
    attack: FloatParam,
    #[id = "dirt_level"]
    dirt_level: FloatParam,
    #[id = "lo_mid_freq"]
    lo_mid_freq: FloatParam,
    #[id = "hi_mid_freq"]
    hi_mid_freq: FloatParam,
    #[id = "bass_keep"]
    bass_keep: FloatParam,
    #[id = "low_note"]
    low_note: FloatParam,
    #[id = "comp"]
    comp: FloatParam,
    #[id = "comp_char"]
    comp_char: EnumParam<CompCharP>,
    #[id = "comp_path"]
    comp_path: EnumParam<CompPathP>,
    #[id = "sag"]
    sag: FloatParam,
    #[id = "input"]
    input: FloatParam,
    #[id = "cab_lows"]
    cab_lows: BoolParam,
    #[id = "bypass"]
    bypass: BoolParam,
    #[id = "gr"]
    gr: FloatParam,
}

impl Default for BassDiParams {
    fn default() -> Self {
        Self {
            gain: f(0, " %", 0),
            mix: f(1, " %", 0),
            level: f(2, " dB", 1),
            bass: f(3, " dB", 1),
            lo_mid: f(4, " dB", 1),
            hi_mid: f(5, " dB", 1),
            treble: f(6, " dB", 1),
            model: EnumParam::new("Model", ModelP::Modern),
            cab: EnumParam::new("Cab", CabP::Smooth),
            grunt: f(9, " dB", 1),
            attack: f(10, " dB", 1),
            dirt_level: f(11, " dB", 1),
            lo_mid_freq: f(12, " Hz", 0),
            hi_mid_freq: f(13, " Hz", 0),
            bass_keep: f(14, " Hz", 0),
            low_note: f(15, " Hz", 1),
            comp: f(16, " %", 0),
            comp_char: EnumParam::new("Comp Type", CompCharP::Fet),
            comp_path: EnumParam::new("Comp Path", CompPathP::Dirt),
            sag: f(19, " %", 0),
            input: f(20, " dB", 1),
            cab_lows: BoolParam::new("Cab Lows", true),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            gr: FloatParam::new("Comp GR", 0.0, FloatRange::Linear { min: 0.0, max: 30.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1))
                .read_only(),
        }
    }
}

impl BassDiParams {
    fn values(&self) -> [f32; NP] {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        [
            self.gain.value(),
            self.mix.value(),
            self.level.value(),
            self.bass.value(),
            self.lo_mid.value(),
            self.hi_mid.value(),
            self.treble.value(),
            self.model.value() as usize as f32,
            self.cab.value() as usize as f32,
            self.grunt.value(),
            self.attack.value(),
            self.dirt_level.value(),
            self.lo_mid_freq.value(),
            self.hi_mid_freq.value(),
            self.bass_keep.value(),
            self.low_note.value(),
            self.comp.value(),
            self.comp_char.value() as usize as f32,
            self.comp_path.value() as usize as f32,
            self.sag.value(),
            self.input.value(),
            b(self.cab_lows.value()),
            b(self.bypass.value()),
        ]
    }
}

pub struct GfxBassDi {
    params: Arc<BassDiParams>,
    dsp: BassDi,
    last: [f32; NP],
}

impl Default for GfxBassDi {
    fn default() -> Self {
        Self { params: Arc::new(BassDiParams::default()), dsp: BassDi::new(), last: [f32::NAN; NP] }
    }
}

impl GfxBassDi {
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

impl Plugin for GfxBassDi {
    const NAME: &'static str = "GoleM FX Bass DI";
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

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.sync(false);
        self.dsp.process(buffer.as_slice());
        self.params.gr.set_output_value(self.dsp.gain_reduction_db().clamp(0.0, 30.0));
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxBassDi {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxBassDi001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_vst3!(GfxBassDi);
