// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first (a voice lane stays mono), stereo 2 / 2 second (two
//! identical chains with linked detectors). Linear ranges only (golem contract). `drive` shows its dB
//! beside the %. `bypass` is the VST3 bypass parameter: the input fades out of the chain over 10 ms while
//! the box and the echo ring out. Zero latency in every setting (no re-sync on `split`, `quality` or
//! `order`: pre-rolled switches inside the processor). `gr`, `state` and `defizz_gr` are read-only.
//! Generated from PARAMS (same order and ids).

use crate::{Morph, NP, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum TypeP {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "soft"]
    #[name = "Soft"]
    Soft,
    #[id = "hard"]
    #[name = "Hard"]
    Hard,
    #[id = "tube"]
    #[name = "Tube"]
    Tube,
    #[id = "fuzz"]
    #[name = "Fuzz"]
    Fuzz,
    #[id = "rectify"]
    #[name = "Rectify"]
    Rectify,
    #[id = "fold"]
    #[name = "Fold"]
    Fold,
    #[id = "sine-fold"]
    #[name = "Sine Fold"]
    SineFold,
    #[id = "crush"]
    #[name = "Crush"]
    Crush,
    #[id = "decimate"]
    #[name = "Decimate"]
    Decimate,
    #[id = "sputter"]
    #[name = "Sputter"]
    Sputter,
    #[id = "ring"]
    #[name = "Ring"]
    Ring,
    #[id = "carbon"]
    #[name = "Carbon"]
    Carbon,
    #[id = "starve"]
    #[name = "Starve"]
    Starve,
    #[id = "rasp"]
    #[name = "Rasp"]
    Rasp,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ObjectP {
    #[id = "none"]
    #[name = "None"]
    None,
    #[id = "tin-can"]
    #[name = "Tin Can"]
    TinCan,
    #[id = "bucket"]
    #[name = "Bucket"]
    Bucket,
    #[id = "pipe"]
    #[name = "Pipe"]
    Pipe,
    #[id = "bottle"]
    #[name = "Bottle"]
    Bottle,
    #[id = "cardboard-box"]
    #[name = "Cardboard Box"]
    CardboardBox,
    #[id = "oil-drum"]
    #[name = "Oil Drum"]
    OilDrum,
    #[id = "glass-jar"]
    #[name = "Glass Jar"]
    GlassJar,
    #[id = "cone"]
    #[name = "Cone"]
    Cone,
    #[id = "hands"]
    #[name = "Hands"]
    Hands,
}

/// `object` shifted by its leading Same (both grow together, at the end).
#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ObjectBP {
    #[id = "same"]
    #[name = "Same"]
    Same,
    #[id = "none"]
    #[name = "None"]
    None,
    #[id = "tin-can"]
    #[name = "Tin Can"]
    TinCan,
    #[id = "bucket"]
    #[name = "Bucket"]
    Bucket,
    #[id = "pipe"]
    #[name = "Pipe"]
    Pipe,
    #[id = "bottle"]
    #[name = "Bottle"]
    Bottle,
    #[id = "cardboard-box"]
    #[name = "Cardboard Box"]
    CardboardBox,
    #[id = "oil-drum"]
    #[name = "Oil Drum"]
    OilDrum,
    #[id = "glass-jar"]
    #[name = "Glass Jar"]
    GlassJar,
    #[id = "cone"]
    #[name = "Cone"]
    Cone,
    #[id = "hands"]
    #[name = "Hands"]
    Hands,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SplitP {
    #[id = "single"]
    #[name = "Single"]
    Single,
    #[id = "two"]
    #[name = "Two"]
    Two,
    #[id = "three"]
    #[name = "Three"]
    Three,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum OrderP {
    #[id = "dirt-box"]
    #[name = "Dirt → Box"]
    DirtBox,
    #[id = "box-dirt"]
    #[name = "Box → Dirt"]
    BoxDirt,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `drive`: %, shown with its dB into the dirt (0.48 dB per %).
fn drive(p: FloatParam) -> FloatParam {
    p.with_unit("").with_value_to_string(Arc::new(|v| format!("{v:.0} % ({:.1} dB)", crate::DRIVE_DB_PER_PCT as f32 * v)))
}

fn meter(name: &str, min: f32, max: f32, unit: &'static str) -> FloatParam {
    FloatParam::new(name, 0.0, FloatRange::Linear { min, max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(1))
        .read_only()
}

/// The ids are this block's own (`morph`, a new golem block); `drive`, `mix`, `level`, `bypass`, `gr`
/// and `state` are the suite's shared ids.
#[derive(Params)]
struct MorphParams {
    #[id = "drive"]
    drive: FloatParam,
    #[id = "morph"]
    morph: FloatParam,
    #[id = "box"]
    box_amount: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "object"]
    object: EnumParam<ObjectP>,
    #[id = "guard"]
    guard: FloatParam,
    #[id = "defizz"]
    defizz: FloatParam,
    #[id = "env_drive"]
    env_drive: FloatParam,
    #[id = "env_morph"]
    env_morph: FloatParam,
    #[id = "pre_hp"]
    pre_hp: FloatParam,
    #[id = "pre_lp"]
    pre_lp: FloatParam,
    #[id = "tilt"]
    tilt: FloatParam,
    #[id = "tilt_freq"]
    tilt_freq: FloatParam,
    #[id = "split"]
    split: EnumParam<SplitP>,
    #[id = "x_low"]
    x_low: FloatParam,
    #[id = "x_air"]
    x_air: FloatParam,
    #[id = "low_a"]
    low_a: EnumParam<TypeP>,
    #[id = "body_a"]
    body_a: EnumParam<TypeP>,
    #[id = "air_a"]
    air_a: EnumParam<TypeP>,
    #[id = "low_b"]
    low_b: EnumParam<TypeP>,
    #[id = "body_b"]
    body_b: EnumParam<TypeP>,
    #[id = "air_b"]
    air_b: EnumParam<TypeP>,
    #[id = "low_trim"]
    low_trim: FloatParam,
    #[id = "body_trim"]
    body_trim: FloatParam,
    #[id = "air_trim"]
    air_trim: FloatParam,
    #[id = "low_level"]
    low_level: FloatParam,
    #[id = "body_level"]
    body_level: FloatParam,
    #[id = "air_level"]
    air_level: FloatParam,
    #[id = "ring_hz"]
    ring_hz: FloatParam,
    #[id = "quality"]
    quality: BoolParam,
    #[id = "object_b"]
    object_b: EnumParam<ObjectBP>,
    #[id = "size"]
    size: FloatParam,
    #[id = "decay"]
    decay: FloatParam,
    #[id = "position"]
    position: FloatParam,
    #[id = "order"]
    order: EnumParam<OrderP>,
    #[id = "comp"]
    comp: FloatParam,
    #[id = "post_hp"]
    post_hp: FloatParam,
    #[id = "post_lp"]
    post_lp: FloatParam,
    #[id = "reso"]
    reso: FloatParam,
    #[id = "env_filter"]
    env_filter: FloatParam,
    #[id = "echo_time"]
    echo_time: FloatParam,
    #[id = "echo_fb"]
    echo_fb: FloatParam,
    #[id = "echo_mix"]
    echo_mix: FloatParam,
    #[id = "level"]
    level: FloatParam,
    #[id = "bypass"]
    bypass: BoolParam,
    /// Read-only meters (golem contract): the comp's gain reduction (dB), the guard (1 = open), the
    /// de-fizz cut (dB).
    #[id = "gr"]
    gr: FloatParam,
    #[id = "state"]
    state: FloatParam,
    #[id = "defizz_gr"]
    defizz_gr: FloatParam,
}

impl Default for MorphParams {
    fn default() -> Self {
        Self {
            drive: drive(f(0, " %", 0)),
            morph: f(1, " %", 0),
            box_amount: f(2, " %", 0),
            mix: f(3, " %", 0),
            object: EnumParam::new("Object", ObjectP::TinCan),
            guard: f(5, " dBFS", 1),
            defizz: f(6, " %", 0),
            env_drive: f(7, " dB", 1),
            env_morph: f(8, " %", 0),
            pre_hp: f(9, " Hz", 0),
            pre_lp: f(10, " Hz", 0),
            tilt: f(11, " dB", 1),
            tilt_freq: f(12, " Hz", 0),
            split: EnumParam::new("Bands", SplitP::Single),
            x_low: f(14, " Hz", 0),
            x_air: f(15, " Hz", 0),
            low_a: EnumParam::new("Low Type A", TypeP::Off),
            body_a: EnumParam::new("Body Type A", TypeP::Soft),
            air_a: EnumParam::new("Air Type A", TypeP::Off),
            low_b: EnumParam::new("Low Type B", TypeP::Off),
            body_b: EnumParam::new("Body Type B", TypeP::Hard),
            air_b: EnumParam::new("Air Type B", TypeP::Off),
            low_trim: f(22, " dB", 1),
            body_trim: f(23, " dB", 1),
            air_trim: f(24, " dB", 1),
            low_level: f(25, " dB", 1),
            body_level: f(26, " dB", 1),
            air_level: f(27, " dB", 1),
            ring_hz: f(28, " Hz", 0),
            quality: BoolParam::new("Quality", false),
            object_b: EnumParam::new("Object B", ObjectBP::Same),
            size: f(31, " %", 0),
            decay: f(32, " %", 0),
            position: f(33, " %", 0),
            order: EnumParam::new("Order", OrderP::DirtBox),
            comp: f(35, " %", 0),
            post_hp: f(36, " Hz", 0),
            post_lp: f(37, " Hz", 0),
            reso: f(38, " %", 0),
            env_filter: f(39, " oct", 2),
            echo_time: f(40, " ms", 0),
            echo_fb: f(41, " %", 0),
            echo_mix: f(42, " %", 0),
            level: f(43, " dB", 1),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            gr: meter("Gain Reduction", 0.0, 40.0, " dB"),
            state: meter("Guard State", 0.0, 1.0, ""),
            defizz_gr: meter("De-fizz", 0.0, 18.0, " dB"),
        }
    }
}

impl MorphParams {
    fn values(&self) -> [f32; NP] {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        [
            self.drive.value(),
            self.morph.value(),
            self.box_amount.value(),
            self.mix.value(),
            self.object.value() as usize as f32,
            self.guard.value(),
            self.defizz.value(),
            self.env_drive.value(),
            self.env_morph.value(),
            self.pre_hp.value(),
            self.pre_lp.value(),
            self.tilt.value(),
            self.tilt_freq.value(),
            self.split.value() as usize as f32,
            self.x_low.value(),
            self.x_air.value(),
            self.low_a.value() as usize as f32,
            self.body_a.value() as usize as f32,
            self.air_a.value() as usize as f32,
            self.low_b.value() as usize as f32,
            self.body_b.value() as usize as f32,
            self.air_b.value() as usize as f32,
            self.low_trim.value(),
            self.body_trim.value(),
            self.air_trim.value(),
            self.low_level.value(),
            self.body_level.value(),
            self.air_level.value(),
            self.ring_hz.value(),
            b(self.quality.value()),
            self.object_b.value() as usize as f32,
            self.size.value(),
            self.decay.value(),
            self.position.value(),
            self.order.value() as usize as f32,
            self.comp.value(),
            self.post_hp.value(),
            self.post_lp.value(),
            self.reso.value(),
            self.env_filter.value(),
            self.echo_time.value(),
            self.echo_fb.value(),
            self.echo_mix.value(),
            self.level.value(),
            b(self.bypass.value()),
        ]
    }
}

pub struct GfxMorph {
    params: Arc<MorphParams>,
    dsp: Morph,
    last: [f32; NP],
}

impl Default for GfxMorph {
    fn default() -> Self {
        Self { params: Arc::new(MorphParams::default()), dsp: Morph::new(), last: [f32::NAN; NP] }
    }
}

impl GfxMorph {
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

impl Plugin for GfxMorph {
    const NAME: &'static str = "GoleM FX Morph";
    const VENDOR: &'static str = "GoleM";
    const URL: &'static str = "https://golem.fm";
    const EMAIL: &'static str = "contact@golem.fm";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout { main_input_channels: NonZeroU32::new(1), main_output_channels: NonZeroU32::new(1), ..AudioIOLayout::const_default() },
        AudioIOLayout { main_input_channels: NonZeroU32::new(2), main_output_channels: NonZeroU32::new(2), ..AudioIOLayout::const_default() },
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

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.sync(false);
        self.dsp.process(buffer.as_slice());
        let m = |id: &str| self.dsp.meter(id).unwrap_or(0.0);
        self.params.gr.set_output_value(m("gr").clamp(0.0, 40.0));
        self.params.state.set_output_value(m("state"));
        self.params.defizz_gr.set_output_value(m("defizz_gr").clamp(0.0, 18.0));
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxMorph {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxMorph0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_vst3!(GfxMorph);
