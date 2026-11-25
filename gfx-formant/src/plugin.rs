// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second (Live: the mid analysed, one
//! filter on both channels; Grain: the mid processed and sent to both). Linear ranges only (golem
//! contract). Generated from PARAMS (same order and ids). The latency is reported: Live `lookahead`
//! (0 by default), Grain D = ⌈1.25 · T_max + gd + 2⌉ by `register` (21.1 / 14.2 / 8.6 ms at 48 kHz),
//! changed only by `engine`, `register` and `lookahead` (setup). `bypass` is the VST3 bypass parameter
//! (the input delayed by the latency, 10 ms crossfade). `state` is a read-only meter.

use crate::{Formant, NP, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

/// `engine` (append only).
#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum EngineP {
    #[id = "live"]
    #[name = "Live"]
    Live,
    #[id = "grain"]
    #[name = "Grain"]
    Grain,
}

/// `source` (append only).
#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SourceP {
    #[id = "voice"]
    #[name = "Voice"]
    Voice,
    #[id = "instrument"]
    #[name = "Instrument"]
    Instrument,
}

/// `register`: gfx-golem's ranges (append only).
#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum RegisterP {
    #[id = "low"]
    #[name = "Low"]
    Low,
    #[id = "mid"]
    #[name = "Mid"]
    Mid,
    #[id = "high"]
    #[name = "High"]
    High,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

#[derive(Params)]
struct FormantParams {
    #[id = "formant"]
    formant: FloatParam,
    #[id = "growl"]
    growl: FloatParam,
    #[id = "vowel"]
    vowel: FloatParam,
    #[id = "vowel_amt"]
    vowel_amt: FloatParam,
    #[id = "engine"]
    engine: EnumParam<EngineP>,
    #[id = "shift"]
    shift: FloatParam,
    #[id = "source"]
    source: EnumParam<SourceP>,
    #[id = "register"]
    register: EnumParam<RegisterP>,
    #[id = "unvoiced"]
    unvoiced: FloatParam,
    #[id = "tilt"]
    tilt: FloatParam,
    #[id = "breath"]
    breath: FloatParam,
    #[id = "thresh"]
    thresh: FloatParam,
    #[id = "lookahead"]
    lookahead: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "output"]
    output: FloatParam,
    #[id = "bypass"]
    bypass: BoolParam,
    /// Read-only meter (golem contract): 0 under `thresh` (envelope held) · 1 not tracked · 2 voiced.
    #[id = "state"]
    state: FloatParam,
}

impl Default for FormantParams {
    fn default() -> Self {
        Self {
            formant: f(0, " st", 2),
            growl: f(1, " %", 0),
            vowel: f(2, " %", 0),
            vowel_amt: f(3, " %", 0),
            engine: EnumParam::new("Engine", EngineP::Live),
            shift: f(5, " st", 2),
            source: EnumParam::new("Source", SourceP::Voice),
            register: EnumParam::new("Register", RegisterP::Mid),
            unvoiced: f(8, " %", 0),
            tilt: f(9, " dB/oct", 1),
            breath: f(10, " %", 0),
            thresh: f(11, " dB", 1),
            lookahead: f(12, " ms", 1),
            mix: f(13, " %", 0),
            output: f(14, " dB", 1),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            state: FloatParam::new("Voice State", 0.0, FloatRange::Linear { min: 0.0, max: 2.0 }).read_only(),
        }
    }
}

impl FormantParams {
    fn values(&self) -> [f32; NP] {
        [
            self.formant.value(),
            self.growl.value(),
            self.vowel.value(),
            self.vowel_amt.value(),
            self.engine.value() as usize as f32,
            self.shift.value(),
            self.source.value() as usize as f32,
            self.register.value() as usize as f32,
            self.unvoiced.value(),
            self.tilt.value(),
            self.breath.value(),
            self.thresh.value(),
            self.lookahead.value(),
            self.mix.value(),
            self.output.value(),
            if self.bypass.value() { 1.0 } else { 0.0 },
        ]
    }
}

pub struct GfxFormant {
    params: Arc<FormantParams>,
    dsp: Formant,
    last: [f32; NP],
    reported: u32,
}

impl Default for GfxFormant {
    fn default() -> Self {
        Self { params: Arc::new(FormantParams::default()), dsp: Formant::new(), last: [f32::NAN; NP], reported: 0 }
    }
}

impl GfxFormant {
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

impl Plugin for GfxFormant {
    const NAME: &'static str = "GoleM FX Formant";
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

    #[cfg(feature = "gui")]
    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-formant"))
    }

    fn initialize(&mut self, layout: &AudioIOLayout, config: &BufferConfig, ctx: &mut impl InitContext<Self>) -> bool {
        let channels = layout.main_output_channels.map_or(1, |c| c.get() as usize);
        self.sync(true);
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
        self.reported = self.dsp.latency() as u32;
        ctx.set_latency_samples(self.reported);
        true
    }

    fn reset(&mut self) {
        self.sync(true);
        self.dsp.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.sync(false);
        self.dsp.process(buffer.as_slice());
        let l = self.dsp.latency() as u32;
        if l != self.reported {
            self.reported = l;
            ctx.set_latency_samples(l);
        }
        self.params.state.set_output_value(self.dsp.state() as f32);
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxFormant {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxFormant01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::PitchShift];
}

nih_export_vst3!(GfxFormant);
