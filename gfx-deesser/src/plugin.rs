// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2. Linear ranges only (golem contract).
//! The lookahead's latency is reported at initialisation and when the lookahead changes.

use crate::{Deesser, Settings, Shape};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum Detect {
    #[id = "relative"]
    Relative,
    #[id = "absolute"]
    Absolute,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ShapeParam {
    #[id = "shelf"]
    Shelf,
    #[id = "bell"]
    Bell,
    #[id = "wide"]
    Wide,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum Focus {
    #[id = "vocal"]
    Vocal,
    #[id = "allround"]
    Allround,
}

fn lin(name: &str, default: f32, min: f32, max: f32, unit: &'static str, decimals: usize) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min, max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `thresh`, `freq`, `range`, `listen` are golem's catalogue ids (`deesser` block).
#[derive(Params)]
struct DeesserParams {
    #[id = "thresh"]
    thresh: FloatParam,
    #[id = "freq"]
    freq: FloatParam,
    #[id = "range"]
    range: FloatParam,
    #[id = "listen"]
    listen: BoolParam,
    #[id = "detect"]
    detect: EnumParam<Detect>,
    #[id = "shape"]
    shape: EnumParam<ShapeParam>,
    #[id = "focus"]
    focus: EnumParam<Focus>,
    #[id = "ratio"]
    ratio: FloatParam,
    #[id = "release"]
    release: FloatParam,
    #[id = "lookahead"]
    lookahead: FloatParam,
    #[id = "gr"]
    gr: FloatParam,
}

impl Default for DeesserParams {
    fn default() -> Self {
        Self {
            thresh: lin("Threshold", -28.0, -50.0, 0.0, " dB", 1),
            freq: lin("Frequency", 6000.0, 1500.0, 14000.0, " Hz", 0),
            range: lin("Range", 10.0, 0.0, 24.0, " dB", 1),
            listen: BoolParam::new("Listen", false),
            detect: EnumParam::new("Detection", Detect::Relative),
            shape: EnumParam::new("Shape", ShapeParam::Shelf),
            focus: EnumParam::new("Focus", Focus::Vocal),
            ratio: lin("Ratio", 6.0, 1.0, 20.0, ":1", 1),
            release: lin("Release", 60.0, 10.0, 200.0, " ms", 0),
            lookahead: lin("Lookahead", 0.0, 0.0, 10.0, " ms", 1),
            gr: lin("Gain Reduction", 0.0, 0.0, 24.0, " dB", 1).read_only(),
        }
    }
}

impl DeesserParams {
    fn settings(&self) -> Settings {
        Settings {
            thresh: self.thresh.value(),
            freq: self.freq.value(),
            range: self.range.value(),
            listen: self.listen.value(),
            relative: self.detect.value() == Detect::Relative,
            shape: match self.shape.value() {
                ShapeParam::Shelf => Shape::Shelf,
                ShapeParam::Bell => Shape::Bell,
                ShapeParam::Wide => Shape::Wide,
            },
            vocal: self.focus.value() == Focus::Vocal,
            ratio: self.ratio.value(),
            release: self.release.value(),
            lookahead: self.lookahead.value(),
        }
    }
}

fn same(a: &Settings, b: &Settings) -> bool {
    a.thresh == b.thresh
        && a.freq == b.freq
        && a.range == b.range
        && a.listen == b.listen
        && a.relative == b.relative
        && a.shape == b.shape
        && a.vocal == b.vocal
        && a.ratio == b.ratio
        && a.release == b.release
        && a.lookahead == b.lookahead
}

pub struct GfxDeesser {
    params: Arc<DeesserParams>,
    dsp: Deesser,
    reported: u32,
}

impl Default for GfxDeesser {
    fn default() -> Self {
        Self { params: Arc::new(DeesserParams::default()), dsp: Deesser::new(), reported: 0 }
    }
}

impl Plugin for GfxDeesser {
    const NAME: &'static str = "GoleM FX De-esser";
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
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
        self.dsp.set(self.params.settings());
        self.reported = self.dsp.latency_samples() as u32;
        ctx.set_latency_samples(self.reported);
        true
    }

    fn reset(&mut self) {
        self.dsp.set(self.params.settings());
        self.dsp.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let s = self.params.settings();
        if !same(&s, &self.dsp.settings()) {
            self.dsp.set(s);
            let l = self.dsp.latency_samples() as u32;
            if l != self.reported {
                self.reported = l;
                ctx.set_latency_samples(l);
            }
        }
        self.dsp.process(buffer.as_slice());
        self.params.gr.set_output_value(self.dsp.gain_reduction_db());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxDeesser {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxDeesser01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_vst3!(GfxDeesser);
