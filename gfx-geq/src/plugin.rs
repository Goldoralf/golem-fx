// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2. Linear ranges only (golem contract).
//! Zero latency, nothing reported.

use crate::{p, Geq, NP, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

/// Option order is permanent (golem matches enum options by rank); append only.
#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum LayoutP {
    #[id = "octave-11"]
    #[name = "Octave 11"]
    Octave11,
    #[id = "guitar-7"]
    #[name = "Guitar 7"]
    Guitar7,
    #[id = "bass-7"]
    #[name = "Bass 7"]
    Bass7,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum Slope {
    #[id = "12"]
    #[name = "12 dB/oct"]
    S12,
    #[id = "24"]
    #[name = "24 dB/oct"]
    S24,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `g1` … `g11` are golem's catalogue ids (`eq-graphic` block); the Guitar 7 and Bass 7 sets are
/// named by frequency.
#[derive(Params)]
struct GeqParams {
    #[id = "g1"]
    g1: FloatParam,
    #[id = "g2"]
    g2: FloatParam,
    #[id = "g3"]
    g3: FloatParam,
    #[id = "g4"]
    g4: FloatParam,
    #[id = "g5"]
    g5: FloatParam,
    #[id = "g6"]
    g6: FloatParam,
    #[id = "g7"]
    g7: FloatParam,
    #[id = "g8"]
    g8: FloatParam,
    #[id = "g9"]
    g9: FloatParam,
    #[id = "g10"]
    g10: FloatParam,
    #[id = "g11"]
    g11: FloatParam,
    #[id = "layout"]
    layout: EnumParam<LayoutP>,
    #[id = "level"]
    level: FloatParam,
    #[id = "amount"]
    amount: FloatParam,
    #[id = "gt_100"]
    gt_100: FloatParam,
    #[id = "gt_200"]
    gt_200: FloatParam,
    #[id = "gt_400"]
    gt_400: FloatParam,
    #[id = "gt_800"]
    gt_800: FloatParam,
    #[id = "gt_1k6"]
    gt_1k6: FloatParam,
    #[id = "gt_3k2"]
    gt_3k2: FloatParam,
    #[id = "gt_6k4"]
    gt_6k4: FloatParam,
    #[id = "gb_50"]
    gb_50: FloatParam,
    #[id = "gb_120"]
    gb_120: FloatParam,
    #[id = "gb_400"]
    gb_400: FloatParam,
    #[id = "gb_500"]
    gb_500: FloatParam,
    #[id = "gb_800"]
    gb_800: FloatParam,
    #[id = "gb_4k5"]
    gb_4k5: FloatParam,
    #[id = "gb_10k"]
    gb_10k: FloatParam,
    #[id = "lowcut"]
    lowcut: FloatParam,
    #[id = "lowcut_slope"]
    lowcut_slope: EnumParam<Slope>,
    #[id = "highcut"]
    highcut: FloatParam,
    #[id = "highcut_slope"]
    highcut_slope: EnumParam<Slope>,
    #[id = "auto_gain"]
    auto_gain: BoolParam,
}

impl Default for GeqParams {
    fn default() -> Self {
        Self {
            g1: f(0, " dB", 1),
            g2: f(1, " dB", 1),
            g3: f(2, " dB", 1),
            g4: f(3, " dB", 1),
            g5: f(4, " dB", 1),
            g6: f(5, " dB", 1),
            g7: f(6, " dB", 1),
            g8: f(7, " dB", 1),
            g9: f(8, " dB", 1),
            g10: f(9, " dB", 1),
            g11: f(10, " dB", 1),
            layout: EnumParam::new("Layout", LayoutP::Octave11),
            level: f(p::LEVEL, " dB", 1),
            amount: f(p::AMOUNT, " %", 0),
            gt_100: f(p::GT, " dB", 1),
            gt_200: f(p::GT + 1, " dB", 1),
            gt_400: f(p::GT + 2, " dB", 1),
            gt_800: f(p::GT + 3, " dB", 1),
            gt_1k6: f(p::GT + 4, " dB", 1),
            gt_3k2: f(p::GT + 5, " dB", 1),
            gt_6k4: f(p::GT + 6, " dB", 1),
            gb_50: f(p::GB, " dB", 1),
            gb_120: f(p::GB + 1, " dB", 1),
            gb_400: f(p::GB + 2, " dB", 1),
            gb_500: f(p::GB + 3, " dB", 1),
            gb_800: f(p::GB + 4, " dB", 1),
            gb_4k5: f(p::GB + 5, " dB", 1),
            gb_10k: f(p::GB + 6, " dB", 1),
            lowcut: f(p::LOWCUT, " Hz", 0),
            lowcut_slope: EnumParam::new("Low Cut Slope", Slope::S12),
            highcut: f(p::HIGHCUT, " Hz", 0),
            highcut_slope: EnumParam::new("High Cut Slope", Slope::S12),
            auto_gain: BoolParam::new("Auto Gain", false),
        }
    }
}

impl GeqParams {
    fn values(&self) -> [f32; NP] {
        let slope = |s: Slope| if s == Slope::S24 { 1.0 } else { 0.0 };
        [
            self.g1.value(),
            self.g2.value(),
            self.g3.value(),
            self.g4.value(),
            self.g5.value(),
            self.g6.value(),
            self.g7.value(),
            self.g8.value(),
            self.g9.value(),
            self.g10.value(),
            self.g11.value(),
            self.layout.value() as usize as f32,
            self.level.value(),
            self.amount.value(),
            self.gt_100.value(),
            self.gt_200.value(),
            self.gt_400.value(),
            self.gt_800.value(),
            self.gt_1k6.value(),
            self.gt_3k2.value(),
            self.gt_6k4.value(),
            self.gb_50.value(),
            self.gb_120.value(),
            self.gb_400.value(),
            self.gb_500.value(),
            self.gb_800.value(),
            self.gb_4k5.value(),
            self.gb_10k.value(),
            self.lowcut.value(),
            slope(self.lowcut_slope.value()),
            self.highcut.value(),
            slope(self.highcut_slope.value()),
            if self.auto_gain.value() { 1.0 } else { 0.0 },
        ]
    }
}

pub struct GfxGeq {
    params: Arc<GeqParams>,
    dsp: Geq,
}

impl Default for GfxGeq {
    fn default() -> Self {
        Self { params: Arc::new(GeqParams::default()), dsp: Geq::new() }
    }
}

impl GfxGeq {
    fn sync(&mut self) {
        for (i, v) in self.params.values().into_iter().enumerate() {
            if v != self.dsp.value(i) {
                self.dsp.set_index(i, v);
            }
        }
    }
}

impl Plugin for GfxGeq {
    const NAME: &'static str = "GoleM FX Graphic EQ";
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
        self.sync();
        self.dsp.reset();
        true
    }

    fn reset(&mut self) {
        self.sync();
        self.dsp.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.sync();
        self.dsp.process(buffer.as_slice());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxGeq {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxGeq000001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Eq];
}

nih_export_vst3!(GfxGeq);
