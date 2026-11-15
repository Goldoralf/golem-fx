// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second (one transport). Linear ranges
//! only (golem contract). `wf_out` is a read-only meter (IEC 386-weighted W&F, %: its weighting is
//! [U] until checked against AES6). `bypass` is the VST3 bypass parameter (10 ms crossfade).
//! Reported latency 0. Generated from PARAMS (same order and ids).

use crate::{Tape, NP, PARAMS};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

gfx_dsp::sync_param_enum!();

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModelP {
    #[id = "studio-reel"]
    #[name = "Studio Reel"]
    StudioReel,
    #[id = "home-reel"]
    #[name = "Home Reel"]
    HomeReel,
    #[id = "cassette"]
    #[name = "Cassette"]
    Cassette,
    #[id = "portable-cassette"]
    #[name = "Portable Cassette"]
    PortableCassette,
    #[id = "microcassette"]
    #[name = "Microcassette"]
    Microcassette,
    #[id = "vhs"]
    #[name = "VHS"]
    Vhs,
    #[id = "vinyl"]
    #[name = "Vinyl"]
    Vinyl,
    #[id = "shellac-78"]
    #[name = "Shellac 78"]
    Shellac78,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SpeedP {
    #[id = "model"]
    #[name = "Model"]
    Model,
    #[id = "1-7-8"]
    #[name = "1⅞ ips"]
    S1,
    #[id = "3-3-4"]
    #[name = "3¾ ips"]
    S3,
    #[id = "7-1-2"]
    #[name = "7½ ips"]
    S7,
    #[id = "15"]
    #[name = "15 ips"]
    S15,
    #[id = "30"]
    #[name = "30 ips"]
    S30,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum FormulaP {
    #[id = "model"]
    #[name = "Model"]
    Model,
    #[id = "ferric"]
    #[name = "Ferric"]
    Ferric,
    #[id = "high-output"]
    #[name = "High Output"]
    HighOutput,
    #[id = "chrome"]
    #[name = "Chrome"]
    Chrome,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum AlcP {
    #[id = "model"]
    #[name = "Model"]
    Model,
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "on"]
    #[name = "On"]
    On,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum NrP {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "compander"]
    #[name = "Compander"]
    Compander,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum NoiseModeP {
    #[id = "follow"]
    #[name = "Follow"]
    Follow,
    #[id = "constant"]
    #[name = "Constant"]
    Constant,
    #[id = "duck"]
    #[name = "Duck"]
    Duck,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

#[derive(Params)]
struct TapeParams {
    #[id = "input"]
    input: FloatParam,
    #[id = "wow"]
    wow: FloatParam,
    #[id = "flutter"]
    flutter: FloatParam,
    #[id = "wear"]
    wear: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "model"]
    model: EnumParam<ModelP>,
    #[id = "speed"]
    speed: EnumParam<SpeedP>,
    #[id = "noise"]
    noise: BoolParam,
    #[id = "formula"]
    formula: EnumParam<FormulaP>,
    #[id = "bias"]
    bias: FloatParam,
    #[id = "head_bump"]
    head_bump: FloatParam,
    #[id = "spacing"]
    spacing: FloatParam,
    #[id = "azimuth"]
    azimuth: FloatParam,
    #[id = "alc"]
    alc: EnumParam<AlcP>,
    #[id = "nr"]
    nr: EnumParam<NrP>,
    #[id = "wow_rate"]
    wow_rate: FloatParam,
    #[id = "wow_rand"]
    wow_rand: FloatParam,
    #[id = "snags"]
    snags: FloatParam,
    #[id = "dropouts"]
    dropouts: FloatParam,
    #[id = "drop_sync"]
    drop_sync: EnumParam<SyncParam>,
    #[id = "crinkle"]
    crinkle: FloatParam,
    #[id = "crackle"]
    crackle: FloatParam,
    #[id = "noise_mode"]
    noise_mode: EnumParam<NoiseModeP>,
    #[id = "hiss"]
    hiss: FloatParam,
    #[id = "mech"]
    mech: FloatParam,
    #[id = "stop"]
    stop: BoolParam,
    #[id = "stop_time"]
    stop_time: FloatParam,
    #[id = "spread"]
    spread: FloatParam,
    #[id = "low_cut"]
    low_cut: FloatParam,
    #[id = "high_cut"]
    high_cut: FloatParam,
    #[id = "level"]
    level: FloatParam,
    #[id = "bypass"]
    bypass: BoolParam,
    #[id = "wf_out"]
    wf_out: FloatParam,
}

impl Default for TapeParams {
    fn default() -> Self {
        Self {
            input: f(0, " dB", 1),
            wow: f(1, " %", 3),
            flutter: f(2, " %", 3),
            wear: f(3, " %", 0),
            mix: f(4, " %", 0),
            model: EnumParam::new("Model", ModelP::StudioReel),
            speed: EnumParam::new("Speed", SpeedP::Model),
            noise: BoolParam::new("Noise", true),
            formula: EnumParam::new("Formula", FormulaP::Model),
            bias: f(9, " %", 0),
            head_bump: f(10, " %", 0),
            spacing: f(11, " µm", 1),
            azimuth: f(12, " ′", 1),
            alc: EnumParam::new("ALC", AlcP::Model),
            nr: EnumParam::new("Noise Reduction", NrP::Off),
            wow_rate: f(15, " %", 0),
            wow_rand: f(16, " %", 0),
            snags: f(17, " %", 0),
            dropouts: f(18, " %", 0),
            drop_sync: EnumParam::new("Dropout Sync", SyncParam::Off),
            crinkle: f(20, " %", 0),
            crackle: f(21, " %", 0),
            noise_mode: EnumParam::new("Noise Mode", NoiseModeP::Follow),
            hiss: f(23, " dB", 1),
            mech: f(24, " dB", 1),
            stop: BoolParam::new("Stop", false),
            stop_time: f(26, " ms", 0),
            spread: f(27, " %", 0),
            low_cut: f(28, " Hz", 0),
            high_cut: f(29, " Hz", 0),
            level: f(30, " dB", 1),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            wf_out: FloatParam::new("W&F (weighted)", 0.0, FloatRange::Linear { min: 0.0, max: 5.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_rounded(3))
                .read_only(),
        }
    }
}

impl TapeParams {
    fn values(&self) -> [f32; NP] {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        [
            self.input.value(),
            self.wow.value(),
            self.flutter.value(),
            self.wear.value(),
            self.mix.value(),
            self.model.value() as usize as f32,
            self.speed.value() as usize as f32,
            b(self.noise.value()),
            self.formula.value() as usize as f32,
            self.bias.value(),
            self.head_bump.value(),
            self.spacing.value(),
            self.azimuth.value(),
            self.alc.value() as usize as f32,
            self.nr.value() as usize as f32,
            self.wow_rate.value(),
            self.wow_rand.value(),
            self.snags.value(),
            self.dropouts.value(),
            self.drop_sync.value() as usize as f32,
            self.crinkle.value(),
            self.crackle.value(),
            self.noise_mode.value() as usize as f32,
            self.hiss.value(),
            self.mech.value(),
            b(self.stop.value()),
            self.stop_time.value(),
            self.spread.value(),
            self.low_cut.value(),
            self.high_cut.value(),
            self.level.value(),
            b(self.bypass.value()),
        ]
    }
}

pub struct GfxTape {
    params: Arc<TapeParams>,
    dsp: Tape,
    last: [f32; NP],
}

impl Default for GfxTape {
    fn default() -> Self {
        Self { params: Arc::new(TapeParams::default()), dsp: Tape::new(), last: [f32::NAN; NP] }
    }
}

impl GfxTape {
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

impl Plugin for GfxTape {
    const NAME: &'static str = "GoleM FX Tape";
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
        // the stop buffer (3.5 s per channel) is allocated here, never in process
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
        self.params.wf_out.set_output_value(self.dsp.wf_out());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxTape {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxTape00001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion, Vst3SubCategory::Modulation];
}

nih_export_vst3!(GfxTape);
