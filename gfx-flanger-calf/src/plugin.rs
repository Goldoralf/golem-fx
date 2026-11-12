// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): stereo 2 / 2 first, mono 1 / 1. Linear ranges only (golem contract).
//! Same engine as gfx-flanger; Tape TZ modes report their 1 ms latency.

use crate::{CalfFlanger, Settings, PARAMS};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

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

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `rate`, `depth`, `feedback` are golem's catalogue ids (`flanger` block, Calf Flanger's meaning).
#[derive(Params)]
struct CalfParams {
    #[id = "rate"]
    rate: FloatParam,
    #[id = "depth"]
    depth: FloatParam,
    #[id = "feedback"]
    feedback: FloatParam,
    #[id = "min_delay"]
    min_delay: FloatParam,
    #[id = "amount"]
    amount: FloatParam,
    #[id = "dry"]
    dry: FloatParam,
    #[id = "stereo_phase"]
    stereo_phase: FloatParam,
    #[id = "mode"]
    mode: EnumParam<ModeP>,
}

impl Default for CalfParams {
    fn default() -> Self {
        Self {
            rate: f(0, " Hz", 2),
            depth: f(1, " ms", 2),
            feedback: f(2, "", 2),
            min_delay: f(3, " ms", 2),
            amount: f(4, "", 2),
            dry: f(5, "", 2),
            stereo_phase: f(6, "°", 0),
            mode: EnumParam::new("Mode", ModeP::Mistress),
        }
    }
}

impl CalfParams {
    fn settings(&self) -> Settings {
        Settings {
            v: [
                self.rate.value(),
                self.depth.value(),
                self.feedback.value(),
                self.min_delay.value(),
                self.amount.value(),
                self.dry.value(),
                self.stereo_phase.value(),
                self.mode.value() as usize as f32,
            ],
        }
    }
}

pub struct GfxFlangerCalf {
    params: Arc<CalfParams>,
    dsp: CalfFlanger,
    reported: u32,
}

impl Default for GfxFlangerCalf {
    fn default() -> Self {
        Self { params: Arc::new(CalfParams::default()), dsp: CalfFlanger::new(), reported: 0 }
    }
}

impl Plugin for GfxFlangerCalf {
    const NAME: &'static str = "GoleM FX Flanger Calf";
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

impl Vst3Plugin for GfxFlangerCalf {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxFlangerC1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Modulation];
}

nih_export_vst3!(GfxFlangerCalf);
