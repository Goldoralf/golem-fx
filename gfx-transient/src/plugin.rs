// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first (a dynamics block before the amp must not make the
//! lane stereo), stereo 2 / 2 second, each with the key on its own auxiliary bus. Linear ranges only
//! (golem contract). `bypass` is the VST3 bypass parameter. The latency is reported only with
//! `lookahead` (setup only). `shaping` and `gr` are read-only.

use crate::{Transient, NP, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum BandP {
    #[id = "full"]
    #[name = "Full"]
    Full,
    #[id = "highs"]
    #[name = "Highs"]
    Highs,
    #[id = "lows"]
    #[name = "Lows"]
    Lows,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum CeilP {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "soft"]
    #[name = "Soft"]
    Soft,
    #[id = "limit"]
    #[name = "Limit"]
    Limit,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SourceP {
    #[id = "main"]
    #[name = "Main"]
    Main,
    #[id = "key"]
    #[name = "Key"]
    Key,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum LookaheadP {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "1"]
    #[name = "1 ms"]
    Ms1,
    #[id = "2"]
    #[name = "2 ms"]
    Ms2,
    #[id = "5"]
    #[name = "5 ms"]
    Ms5,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

#[derive(Params)]
struct TransientParams {
    #[id = "attack"]
    attack: FloatParam,
    #[id = "sustain"]
    sustain: FloatParam,
    #[id = "att_time"]
    att_time: FloatParam,
    #[id = "sus_time"]
    sus_time: FloatParam,
    #[id = "att_band"]
    att_band: EnumParam<BandP>,
    #[id = "ceil_mode"]
    ceil_mode: EnumParam<CeilP>,
    #[id = "sens"]
    sens: FloatParam,
    #[id = "smooth"]
    smooth: FloatParam,
    #[id = "att_freq"]
    att_freq: FloatParam,
    #[id = "sc_hpf"]
    sc_hpf: FloatParam,
    #[id = "sc_lpf"]
    sc_lpf: FloatParam,
    #[id = "thresh"]
    thresh: FloatParam,
    #[id = "source"]
    source: EnumParam<SourceP>,
    #[id = "link"]
    link: FloatParam,
    #[id = "lookahead"]
    lookahead: EnumParam<LookaheadP>,
    #[id = "ceiling"]
    ceiling: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "level"]
    level: FloatParam,
    #[id = "listen"]
    listen: BoolParam,
    #[id = "delta"]
    delta: BoolParam,
    #[id = "bypass"]
    bypass: BoolParam,
    /// Read-only meters (golem contract): the attack + sustain gain applied (dB, −48 … +48), the
    /// ceiling's reduction (dB, ≥ 0).
    #[id = "shaping"]
    shaping: FloatParam,
    #[id = "gr"]
    gr: FloatParam,
}

impl Default for TransientParams {
    fn default() -> Self {
        Self {
            attack: f(0, " dB", 1),
            sustain: f(1, " dB", 1),
            att_time: f(2, " ms", 1),
            sus_time: f(3, " ms", 0),
            att_band: EnumParam::new("Attack Band", BandP::Full),
            ceil_mode: EnumParam::new("Ceiling", CeilP::Off),
            sens: f(6, " %", 0),
            smooth: f(7, " ms", 2),
            att_freq: f(8, " Hz", 0),
            sc_hpf: f(9, " Hz", 0),
            sc_lpf: f(10, " Hz", 0),
            thresh: f(11, " dB", 1),
            source: EnumParam::new("Detector Source", SourceP::Main),
            link: f(13, " %", 0),
            lookahead: EnumParam::new("Lookahead", LookaheadP::Off),
            ceiling: f(15, " dB", 1),
            mix: f(16, " %", 0),
            level: f(17, " dB", 1),
            listen: BoolParam::new("Listen", false),
            delta: BoolParam::new("Delta", false),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            shaping: FloatParam::new("Shaping", 0.0, FloatRange::Linear { min: -48.0, max: 48.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1))
                .read_only(),
            gr: FloatParam::new("Gain Reduction", 0.0, FloatRange::Linear { min: 0.0, max: 90.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1))
                .read_only(),
        }
    }
}

impl TransientParams {
    fn values(&self) -> [f32; NP] {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        [
            self.attack.value(),
            self.sustain.value(),
            self.att_time.value(),
            self.sus_time.value(),
            self.att_band.value() as usize as f32,
            self.ceil_mode.value() as usize as f32,
            self.sens.value(),
            self.smooth.value(),
            self.att_freq.value(),
            self.sc_hpf.value(),
            self.sc_lpf.value(),
            self.thresh.value(),
            self.source.value() as usize as f32,
            self.link.value(),
            self.lookahead.value() as usize as f32,
            self.ceiling.value(),
            self.mix.value(),
            self.level.value(),
            b(self.listen.value()),
            b(self.delta.value()),
            b(self.bypass.value()),
        ]
    }
}

pub struct GfxTransient {
    params: Arc<TransientParams>,
    dsp: Transient,
    last: [f32; NP],
    reported: u32,
}

impl Default for GfxTransient {
    fn default() -> Self {
        Self { params: Arc::new(TransientParams::default()), dsp: Transient::new(), last: [f32::NAN; NP], reported: 0 }
    }
}

impl GfxTransient {
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

impl Plugin for GfxTransient {
    const NAME: &'static str = "GoleM FX Transient";
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
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-transient"))
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

    fn process(&mut self, buffer: &mut Buffer, aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.sync(false);
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
        // `set_latency_samples` only when the value changes (setup only)
        let l = self.dsp.latency() as u32;
        if l != self.reported {
            self.reported = l;
            ctx.set_latency_samples(l);
        }
        self.params.shaping.set_output_value(self.dsp.shaping_db());
        self.params.gr.set_output_value(self.dsp.gr_db());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxTransient {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxTransnt01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_vst3!(GfxTransient);
