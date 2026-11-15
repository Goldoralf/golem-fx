// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): stereo 2 / 2 first, mono 1 / 1 (stereo by nature: golem turns the lane
//! stereo here, and a mono DI arrives on both inputs). Latency 0: the dry is never delayed, the wet is
//! late by design (README §2.9). Linear ranges only (golem contract). `bypass` is the VST3 bypass
//! parameter: with `trails` the wet rings out. `state` is a read-only meter (golem's fork).
//! Sample-accurate automation, so `capture` lands on its sample. Generated from PARAMS.

use crate::{Stretch, NP, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModeP {
    #[id = "capture"]
    #[name = "Capture"]
    Capture,
    #[id = "rolling"]
    #[name = "Rolling"]
    Rolling,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

fn b(i: usize) -> BoolParam {
    let pi = PARAMS[i];
    BoolParam::new(pi.name, pi.default > 0.5)
}

#[derive(Params)]
struct StretchParams {
    #[id = "amount"]
    amount: FloatParam,
    #[id = "fft"]
    fft: FloatParam,
    #[id = "spread"]
    spread: FloatParam,
    #[id = "vol"]
    vol: FloatParam,
    #[id = "mode"]
    mode: EnumParam<ModeP>,
    #[id = "capture"]
    capture: BoolParam,
    #[id = "freeze"]
    freeze: BoolParam,
    #[id = "thru"]
    thru: BoolParam,
    #[id = "pitch"]
    pitch: FloatParam,
    #[id = "fshift"]
    fshift: FloatParam,
    #[id = "compress"]
    compress: FloatParam,
    #[id = "maxlen"]
    maxlen: FloatParam,
    #[id = "cloud"]
    cloud: FloatParam,
    #[id = "width"]
    width: FloatParam,
    #[id = "xfade"]
    xfade: FloatParam,
    #[id = "hpf"]
    hpf: FloatParam,
    #[id = "lpf"]
    lpf: FloatParam,
    #[id = "spread_on"]
    spread_on: BoolParam,
    #[id = "comp_on"]
    comp_on: BoolParam,
    #[id = "trails"]
    trails: BoolParam,
    #[id = "bypass"]
    bypass: BoolParam,
    /// Read-only meter (golem contract): 0 Empty · 1 Capturing · 2 Playing · 3 Frozen · 4 Rolling.
    #[id = "state"]
    state: FloatParam,
}

impl Default for StretchParams {
    fn default() -> Self {
        Self {
            amount: f(0, " ×", 2),
            fft: f(1, " ms", 1),
            spread: f(2, " ¢", 0),
            vol: f(3, " dB", 1),
            mode: EnumParam::new(PARAMS[4].name, ModeP::Rolling),
            capture: b(5),
            freeze: b(6),
            thru: b(7),
            pitch: f(8, " st", 2),
            fshift: f(9, " Hz", 1),
            compress: f(10, " %", 0),
            maxlen: f(11, " s", 1),
            cloud: f(12, " %", 0),
            width: f(13, " %", 0),
            xfade: f(14, " ms", 0),
            hpf: f(15, " Hz", 0),
            lpf: f(16, " Hz", 0),
            spread_on: b(17),
            comp_on: b(18),
            trails: b(19),
            bypass: b(20).make_bypass(),
            state: FloatParam::new("State", 0.0, FloatRange::Linear { min: 0.0, max: 4.0 })
                .with_value_to_string(Arc::new(|v| ["Empty", "Capturing", "Playing", "Frozen", "Rolling"][(v.round().clamp(0.0, 4.0)) as usize].into()))
                .read_only(),
        }
    }
}

impl StretchParams {
    fn values(&self) -> [f32; NP] {
        let bo = |p: &BoolParam| if p.value() { 1.0 } else { 0.0 };
        [
            self.amount.value(),
            self.fft.value(),
            self.spread.value(),
            self.vol.value(),
            self.mode.value() as usize as f32,
            bo(&self.capture),
            bo(&self.freeze),
            bo(&self.thru),
            self.pitch.value(),
            self.fshift.value(),
            self.compress.value(),
            self.maxlen.value(),
            self.cloud.value(),
            self.width.value(),
            self.xfade.value(),
            self.hpf.value(),
            self.lpf.value(),
            bo(&self.spread_on),
            bo(&self.comp_on),
            bo(&self.trails),
            bo(&self.bypass),
        ]
    }
}

pub struct GfxStretch {
    params: Arc<StretchParams>,
    dsp: Stretch,
    last: [f32; NP],
}

impl Default for GfxStretch {
    fn default() -> Self {
        Self { params: Arc::new(StretchParams::default()), dsp: Stretch::new(), last: [f32::NAN; NP] }
    }
}

impl GfxStretch {
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

impl Plugin for GfxStretch {
    const NAME: &'static str = "GoleM FX Stretch";
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

    // `capture` is edge-triggered: the buffer is split at its change, so a take starts on its sample
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(&mut self, layout: &AudioIOLayout, config: &BufferConfig, _ctx: &mut impl InitContext<Self>) -> bool {
        let channels = layout.main_output_channels.map_or(1, |c| c.get() as usize);
        self.sync(true);
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
        true
    }

    fn reset(&mut self) {
        self.sync(true);
        self.dsp.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.sync(false);
        self.dsp.process(buffer.as_slice());
        self.params.state.set_output_value(self.dsp.state());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxStretch {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxStretch01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Reverb];
}

nih_export_vst3!(GfxStretch);
