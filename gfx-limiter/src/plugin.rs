// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2. Linear ranges only (golem contract).
//! The lookahead's latency is reported at initialisation and whenever the lookahead or true-peak
//! setting changes (a preset-level change: the host re-aligns once).

use crate::{Limiter, Settings, Style};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum StyleParam {
    #[id = "clean"]
    Clean,
    #[id = "punchy"]
    Punchy,
    #[id = "clip"]
    Clip,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum Lookahead {
    #[id = "off"]
    Off,
    #[id = "1ms"]
    #[name = "1 ms"]
    Ms1,
    #[id = "2ms"]
    #[name = "2 ms"]
    Ms2,
    #[id = "5ms"]
    #[name = "5 ms"]
    Ms5,
}

fn lin(name: &str, default: f32, min: f32, max: f32, unit: &'static str, decimals: usize) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min, max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `threshold` and `release` are golem's catalogue ids (`limiter` block), x42-dpl's meanings, release
/// in ms.
#[derive(Params)]
struct LimiterParams {
    #[id = "threshold"]
    threshold: FloatParam,
    #[id = "release"]
    release: FloatParam,
    #[id = "gain"]
    gain: FloatParam,
    #[id = "style"]
    style: EnumParam<StyleParam>,
    #[id = "lookahead"]
    lookahead: EnumParam<Lookahead>,
    #[id = "true_peak"]
    true_peak: BoolParam,
    #[id = "auto_release"]
    auto_release: BoolParam,
    #[id = "clip"]
    clip: FloatParam,
    #[id = "link"]
    link: FloatParam,
    #[id = "gr"]
    gr: FloatParam,
}

impl Default for LimiterParams {
    fn default() -> Self {
        Self {
            threshold: lin("Threshold", -6.0, -24.0, 0.0, " dB", 2),
            release: lin("Release", 50.0, 1.0, 1000.0, " ms", 1),
            gain: lin("Input Gain", 0.0, 0.0, 24.0, " dB", 1),
            style: EnumParam::new("Style", StyleParam::Clean),
            lookahead: EnumParam::new("Lookahead", Lookahead::Off),
            true_peak: BoolParam::new("True Peak", false),
            auto_release: BoolParam::new("Auto Release", true),
            clip: lin("Clip Softness", 50.0, 0.0, 100.0, " %", 0),
            link: lin("Stereo Link", 100.0, 0.0, 100.0, " %", 0),
            gr: lin("Gain Reduction", 0.0, 0.0, 60.0, " dB", 1).read_only(),
        }
    }
}

impl LimiterParams {
    fn settings(&self) -> Settings {
        Settings {
            threshold: self.threshold.value(),
            release: self.release.value(),
            gain: self.gain.value(),
            style: match self.style.value() {
                StyleParam::Clean => Style::Clean,
                StyleParam::Punchy => Style::Punchy,
                StyleParam::Clip => Style::Clip,
            },
            lookahead: match self.lookahead.value() {
                Lookahead::Off => 0,
                Lookahead::Ms1 => 1,
                Lookahead::Ms2 => 2,
                Lookahead::Ms5 => 3,
            },
            true_peak: self.true_peak.value(),
            auto_release: self.auto_release.value(),
            clip: self.clip.value(),
            link: self.link.value(),
        }
    }
}

fn same(a: &Settings, b: &Settings) -> bool {
    a.threshold == b.threshold
        && a.release == b.release
        && a.gain == b.gain
        && a.style == b.style
        && a.lookahead == b.lookahead
        && a.true_peak == b.true_peak
        && a.auto_release == b.auto_release
        && a.clip == b.clip
        && a.link == b.link
}

pub struct GfxLimiter {
    params: Arc<LimiterParams>,
    dsp: Limiter,
    reported: u32,
}

impl Default for GfxLimiter {
    fn default() -> Self {
        Self { params: Arc::new(LimiterParams::default()), dsp: Limiter::new(), reported: 0 }
    }
}

impl Plugin for GfxLimiter {
    const NAME: &'static str = "GoleM FX Limiter";
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
        self.dsp.set(self.params.settings());
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
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

impl Vst3Plugin for GfxLimiter {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxLimiter01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_vst3!(GfxLimiter);
