// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second, each with the key on its own
//! auxiliary bus (the main layout stays 1 / 1, as the Golem Engine requires). Linear ranges only
//! (golem contract). Generated from PARAMS (same order and ids).

use crate::{Met, Settings, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum Source {
    #[id = "main"]
    Main,
    #[id = "key"]
    Key,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

#[derive(Params)]
struct MetParams {
    #[id = "thresh"]
    thresh: FloatParam,
    #[id = "hold"]
    hold: FloatParam,
    #[id = "release"]
    release: FloatParam,
    #[id = "ratio"]
    ratio: FloatParam,
    #[id = "range"]
    range: FloatParam,
    #[id = "smooth"]
    smooth: FloatParam,
    #[id = "adaptive"]
    adaptive: BoolParam,
    #[id = "hpf"]
    hpf: FloatParam,
    #[id = "lpf"]
    lpf: FloatParam,
    #[id = "source"]
    source: EnumParam<Source>,
    #[id = "listen"]
    listen: BoolParam,
    /// Read-only meters (golem contract): gate open 0 / 1, gain reduction in dB (≥ 0).
    #[id = "state"]
    state: FloatParam,
    #[id = "gr"]
    gr: FloatParam,
}

impl Default for MetParams {
    fn default() -> Self {
        Self {
            thresh: f(0, " dB", 1),
            hold: f(1, " ms", 0),
            release: f(2, " ms", 0),
            ratio: f(3, ":1", 2),
            range: f(4, " dB", 1),
            smooth: f(5, " ms", 0),
            adaptive: BoolParam::new("Adaptive", true),
            hpf: f(7, " Hz", 0),
            lpf: f(8, " Hz", 0),
            source: EnumParam::new("Detector Source", Source::Key),
            listen: BoolParam::new("Listen", false),
            state: FloatParam::new("Gate State", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 }).read_only(),
            gr: FloatParam::new("Gain Reduction", 0.0, FloatRange::Linear { min: 0.0, max: 90.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1))
                .read_only(),
        }
    }
}

impl MetParams {
    fn settings(&self) -> Settings {
        Settings {
            thresh: self.thresh.value(),
            hold: self.hold.value(),
            release: self.release.value(),
            ratio: self.ratio.value(),
            range: self.range.value(),
            smooth: self.smooth.value(),
            adaptive: self.adaptive.value(),
            hpf: self.hpf.value(),
            lpf: self.lpf.value(),
            source_key: self.source.value() == Source::Key,
            listen: self.listen.value(),
        }
    }
}

pub struct GfxMet {
    params: Arc<MetParams>,
    dsp: Met,
}

impl Default for GfxMet {
    fn default() -> Self {
        Self { params: Arc::new(MetParams::default()), dsp: Met::new() }
    }
}

impl Plugin for GfxMet {
    const NAME: &'static str = "GoleM FX Met";
    const VENDOR: &'static str = "GoleM";
    const URL: &'static str = "https://golem.fm";
    const EMAIL: &'static str = "contact@golem.fm";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            aux_input_ports: &[new_nonzero_u32(1)],
            names: PortNames { aux_inputs: &["Key"], ..PortNames::const_default() },
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[new_nonzero_u32(2)],
            names: PortNames { aux_inputs: &["Key"], ..PortNames::const_default() },
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
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-met"))
    }

    fn initialize(&mut self, layout: &AudioIOLayout, config: &BufferConfig, _ctx: &mut impl InitContext<Self>) -> bool {
        let channels = layout.main_output_channels.map_or(1, |c| c.get() as usize);
        self.dsp.set(self.params.settings());
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
        true
    }

    fn reset(&mut self) {
        self.dsp.set(self.params.settings());
        self.dsp.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, aux: &mut AuxiliaryBuffers, _ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let s = self.params.settings();
        if s != self.dsp.settings() {
            self.dsp.set(s);
        }
        let key = aux.inputs.first().map(|b| b.as_slice_immutable());
        match key {
            Some(k) if !k.is_empty() => {
                let k0: &[f32] = &k[0][..];
                let k1: &[f32] = &k[k.len().min(2) - 1][..];
                let keys = [k0, k1];
                self.dsp.process_with_key(buffer.as_slice(), Some(&keys[..k.len().min(2)]));
            }
            _ => self.dsp.process_with_key(buffer.as_slice(), None),
        }
        self.params.state.set_output_value(if self.dsp.is_open() { 1.0 } else { 0.0 });
        self.params.gr.set_output_value((-self.dsp.gain_reduction_db()).clamp(0.0, 90.0));
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxMet {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxMet000001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_vst3!(GfxMet);
