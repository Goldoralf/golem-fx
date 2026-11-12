// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2. Linear ranges only (golem contract).

use crate::{p, Eq, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

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

/// `lowcut`, `midgain`, `midfreq`, `highs` are golem's catalogue ids (`eq` block).
#[derive(Params)]
struct EqParams {
    #[id = "lowcut"]
    lowcut: FloatParam,
    #[id = "midgain"]
    midgain: FloatParam,
    #[id = "midfreq"]
    midfreq: FloatParam,
    #[id = "highs"]
    highs: FloatParam,
    #[id = "lowcut_slope"]
    lowcut_slope: EnumParam<Slope>,
    #[id = "lows"]
    lows: FloatParam,
    #[id = "lowfreq"]
    lowfreq: FloatParam,
    #[id = "midq"]
    midq: FloatParam,
    #[id = "midprop"]
    midprop: FloatParam,
    #[id = "mid2gain"]
    mid2gain: FloatParam,
    #[id = "mid2freq"]
    mid2freq: FloatParam,
    #[id = "mid2q"]
    mid2q: FloatParam,
    #[id = "highfreq"]
    highfreq: FloatParam,
    #[id = "air"]
    air: FloatParam,
    #[id = "airfreq"]
    airfreq: FloatParam,
    #[id = "tilt"]
    tilt: FloatParam,
    #[id = "highcut"]
    highcut: FloatParam,
    #[id = "highcut_slope"]
    highcut_slope: EnumParam<Slope>,
    #[id = "harsh_freq"]
    harsh_freq: FloatParam,
    #[id = "harsh_thresh"]
    harsh_thresh: FloatParam,
    #[id = "harsh_range"]
    harsh_range: FloatParam,
    #[id = "output"]
    output: FloatParam,
    #[id = "auto_gain"]
    auto_gain: BoolParam,
    #[id = "harsh_gr"]
    harsh_gr: FloatParam,
}

impl Default for EqParams {
    fn default() -> Self {
        Self {
            lowcut: f(p::LOWCUT, " Hz", 0),
            midgain: f(p::MIDGAIN, " dB", 1),
            midfreq: f(p::MIDFREQ, " Hz", 0),
            highs: f(p::HIGHS, " dB", 1),
            lowcut_slope: EnumParam::new("Low Cut Slope", Slope::S12),
            lows: f(p::LOWS, " dB", 1),
            lowfreq: f(p::LOWFREQ, " Hz", 0),
            midq: f(p::MIDQ, "", 2),
            midprop: f(p::MIDPROP, " %", 0),
            mid2gain: f(p::MID2GAIN, " dB", 1),
            mid2freq: f(p::MID2FREQ, " Hz", 0),
            mid2q: f(p::MID2Q, "", 2),
            highfreq: f(p::HIGHFREQ, " Hz", 0),
            air: f(p::AIR, " dB", 1),
            airfreq: f(p::AIRFREQ, " Hz", 0),
            tilt: f(p::TILT, " dB", 1),
            highcut: f(p::HIGHCUT, " Hz", 0),
            highcut_slope: EnumParam::new("High Cut Slope", Slope::S12),
            harsh_freq: f(p::HARSH_FREQ, " Hz", 0),
            harsh_thresh: f(p::HARSH_THRESH, " dB", 1),
            harsh_range: f(p::HARSH_RANGE, " dB", 1),
            output: f(p::OUTPUT, " dB", 1),
            auto_gain: BoolParam::new("Auto Gain", false),
            harsh_gr: FloatParam::new("De-harsh Reduction", 0.0, FloatRange::Linear { min: 0.0, max: 12.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1))
                .read_only(),
        }
    }
}

impl EqParams {
    fn values(&self) -> [f32; 23] {
        let slope = |s: Slope| if s == Slope::S24 { 1.0 } else { 0.0 };
        [
            self.lowcut.value(),
            self.midgain.value(),
            self.midfreq.value(),
            self.highs.value(),
            slope(self.lowcut_slope.value()),
            self.lows.value(),
            self.lowfreq.value(),
            self.midq.value(),
            self.midprop.value(),
            self.mid2gain.value(),
            self.mid2freq.value(),
            self.mid2q.value(),
            self.highfreq.value(),
            self.air.value(),
            self.airfreq.value(),
            self.tilt.value(),
            self.highcut.value(),
            slope(self.highcut_slope.value()),
            self.harsh_freq.value(),
            self.harsh_thresh.value(),
            self.harsh_range.value(),
            self.output.value(),
            if self.auto_gain.value() { 1.0 } else { 0.0 },
        ]
    }
}

pub struct GfxEq {
    params: Arc<EqParams>,
    dsp: Eq,
}

impl Default for GfxEq {
    fn default() -> Self {
        Self { params: Arc::new(EqParams::default()), dsp: Eq::new() }
    }
}

impl GfxEq {
    fn sync(&mut self) {
        for (i, v) in self.params.values().into_iter().enumerate() {
            if v != self.dsp.value(i) {
                self.dsp.set_index(i, v);
            }
        }
    }
}

impl Plugin for GfxEq {
    const NAME: &'static str = "GoleM FX EQ";
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
        true
    }

    fn reset(&mut self) {
        self.sync();
        self.dsp.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.sync();
        self.dsp.process(buffer.as_slice());
        self.params.harsh_gr.set_output_value(self.dsp.harsh_reduction_db());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxEq {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxEq0000001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Eq];
}

nih_export_vst3!(GfxEq);
