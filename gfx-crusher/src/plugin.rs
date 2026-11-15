// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second (one shared clock). Linear
//! ranges only (golem contract). `rate_out` is a read-only meter (the effective virtual rate).
//! Reported latency 0. Generated from PARAMS (same order and ids).

use crate::{Crusher, NP, PARAMS};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

gfx_dsp::sync_param_enum!();

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModelP {
    #[id = "raw"]
    #[name = "Raw"]
    Raw,
    #[id = "clean"]
    #[name = "Clean"]
    Clean,
    #[id = "average"]
    #[name = "Average"]
    Average,
    #[id = "sampler"]
    #[name = "Sampler"]
    Sampler,
    #[id = "drum-machine"]
    #[name = "Drum Machine"]
    DrumMachine,
    #[id = "console"]
    #[name = "Console"]
    Console,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum LawP {
    #[id = "linear"]
    #[name = "Linear"]
    Linear,
    #[id = "mu-law"]
    #[name = "µ-law"]
    MuLaw,
    #[id = "float"]
    #[name = "Float"]
    Float,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum QuantizerP {
    #[id = "tread"]
    #[name = "Tread"]
    Tread,
    #[id = "riser"]
    #[name = "Riser"]
    Riser,
    #[id = "truncate"]
    #[name = "Truncate"]
    Truncate,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum TailsP {
    #[id = "clean"]
    #[name = "Clean"]
    Clean,
    #[id = "gate"]
    #[name = "Gate"]
    Gate,
    #[id = "raw"]
    #[name = "Raw"]
    Raw,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModP {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "lfo"]
    #[name = "LFO"]
    Lfo,
    #[id = "envelope"]
    #[name = "Envelope"]
    Envelope,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

#[derive(Params)]
struct CrusherParams {
    #[id = "quant"]
    quant: FloatParam,
    #[id = "rate"]
    rate: FloatParam,
    #[id = "nonlin"]
    nonlin: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "model"]
    model: EnumParam<ModelP>,
    #[id = "law"]
    law: EnumParam<LawP>,
    #[id = "quantizer"]
    quantizer: EnumParam<QuantizerP>,
    #[id = "dither"]
    dither: FloatParam,
    #[id = "tails"]
    tails: EnumParam<TailsP>,
    #[id = "input"]
    input: FloatParam,
    #[id = "level"]
    level: FloatParam,
    #[id = "low_cut"]
    low_cut: FloatParam,
    #[id = "high_cut"]
    high_cut: FloatParam,
    #[id = "asym"]
    asym: FloatParam,
    #[id = "ladder"]
    ladder: FloatParam,
    #[id = "rate_shift"]
    rate_shift: FloatParam,
    #[id = "jitter"]
    jitter: FloatParam,
    #[id = "track"]
    track: BoolParam,
    #[id = "mod"]
    modulation: EnumParam<ModP>,
    #[id = "mod_depth"]
    mod_depth: FloatParam,
    #[id = "mod_rate"]
    mod_rate: FloatParam,
    #[id = "sync"]
    sync: EnumParam<SyncParam>,
    #[id = "env_sens"]
    env_sens: FloatParam,
    #[id = "env_att"]
    env_att: FloatParam,
    #[id = "env_rel"]
    env_rel: FloatParam,
    #[id = "rate_out"]
    rate_out: FloatParam,
}

impl Default for CrusherParams {
    fn default() -> Self {
        Self {
            quant: f(0, " bits", 2),
            rate: f(1, " Hz", 0),
            nonlin: f(2, " %", 0),
            mix: f(3, " %", 0),
            model: EnumParam::new("Model", ModelP::Raw),
            law: EnumParam::new("Format", LawP::Linear),
            quantizer: EnumParam::new("Quantizer", QuantizerP::Tread),
            dither: f(7, " %", 0),
            tails: EnumParam::new("Tails", TailsP::Clean),
            input: f(9, " dB", 1),
            level: f(10, " dB", 1),
            low_cut: f(11, " Hz", 0),
            high_cut: f(12, " Hz", 0),
            asym: f(13, " %", 0),
            ladder: f(14, " %", 0),
            rate_shift: f(15, " st", 1),
            jitter: f(16, " %", 0),
            track: BoolParam::new("Track", false),
            modulation: EnumParam::new("Rate Mod", ModP::Off),
            mod_depth: f(19, " %", 0),
            mod_rate: f(20, " Hz", 2),
            sync: EnumParam::new("Sync", SyncParam::Off),
            env_sens: f(22, " %", 0),
            env_att: f(23, " ms", 0),
            env_rel: f(24, " ms", 0),
            rate_out: FloatParam::new("Rate (now)", 0.0, FloatRange::Linear { min: 0.0, max: 48000.0 })
                .with_unit(" Hz")
                .with_value_to_string(formatters::v2s_f32_rounded(0))
                .read_only(),
        }
    }
}

impl CrusherParams {
    fn values(&self) -> [f32; NP] {
        [
            self.quant.value(),
            self.rate.value(),
            self.nonlin.value(),
            self.mix.value(),
            self.model.value() as usize as f32,
            self.law.value() as usize as f32,
            self.quantizer.value() as usize as f32,
            self.dither.value(),
            self.tails.value() as usize as f32,
            self.input.value(),
            self.level.value(),
            self.low_cut.value(),
            self.high_cut.value(),
            self.asym.value(),
            self.ladder.value(),
            self.rate_shift.value(),
            self.jitter.value(),
            if self.track.value() { 1.0 } else { 0.0 },
            self.modulation.value() as usize as f32,
            self.mod_depth.value(),
            self.mod_rate.value(),
            self.sync.value() as usize as f32,
            self.env_sens.value(),
            self.env_att.value(),
            self.env_rel.value(),
        ]
    }
}

pub struct GfxCrusher {
    params: Arc<CrusherParams>,
    dsp: Crusher,
    last: [f32; NP],
}

impl Default for GfxCrusher {
    fn default() -> Self {
        Self { params: Arc::new(CrusherParams::default()), dsp: Crusher::new(), last: [f32::NAN; NP] }
    }
}

impl GfxCrusher {
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

impl Plugin for GfxCrusher {
    const NAME: &'static str = "GoleM FX Crusher";
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
        self.sync(true);
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
        true
    }

    fn reset(&mut self) {
        self.sync(true);
        self.dsp.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.sync(false);
        let t = ctx.transport();
        self.dsp.set_transport(TransportInfo { tempo: t.tempo, pos_beats: t.pos_beats(), playing: t.playing });
        self.dsp.process(buffer.as_slice());
        self.params.rate_out.set_output_value(self.dsp.rate_now());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxCrusher {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxCrusher01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_vst3!(GfxCrusher);
