// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second (two independent channels: a
//! fuzz is mono by nature). Linear ranges only (golem contract). Reported latency 0 (minimum-phase
//! oversampling, a few samples of group delay, the gfx-drive / gfx-emet precedent). `bypass` is the
//! VST3 bypass parameter (10 ms crossfade). `bias_v` and `supply_v` are read-only meters. Generated
//! from PARAMS (same order and ids).

use crate::{Fuzz, NP, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

/// `model`: the shipped circuits, in the README's order (Si Pair, Ge Three and Octave Up are
/// appended when they ship).
#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModelP {
    #[id = "ge-pair"]
    #[name = "Ge Pair"]
    GePair,
    #[id = "si-four"]
    #[name = "Si Four"]
    SiFour,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum PickupP {
    #[id = "single-coil"]
    #[name = "Single Coil"]
    SingleCoil,
    #[id = "humbucker"]
    #[name = "Humbucker"]
    Humbucker,
    #[id = "p-90"]
    #[name = "P-90"]
    P90,
    #[id = "active"]
    #[name = "Active"]
    Active,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum StackP {
    #[id = "classic"]
    #[name = "Classic"]
    Classic,
    #[id = "early"]
    #[name = "Early"]
    Early,
    #[id = "triangle"]
    #[name = "Triangle"]
    Triangle,
    #[id = "russian"]
    #[name = "Russian"]
    Russian,
    #[id = "bypass"]
    #[name = "Bypass"]
    Bypass,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(
        pi.name,
        pi.default,
        FloatRange::Linear {
            min: pi.min,
            max: pi.max,
        },
    )
    .with_unit(unit)
    .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

fn meter(name: &str) -> FloatParam {
    FloatParam::new(
        name,
        0.0,
        FloatRange::Linear {
            min: 0.0,
            max: 18.0,
        },
    )
    .with_unit(" V")
    .with_value_to_string(formatters::v2s_f32_rounded(2))
    .read_only()
}

#[derive(Params)]
struct FuzzParams {
    #[id = "fuzz"]
    fuzz: FloatParam,
    #[id = "tone"]
    tone: FloatParam,
    #[id = "level"]
    level: FloatParam,
    #[id = "guitar_vol"]
    guitar_vol: FloatParam,
    #[id = "model"]
    model: EnumParam<ModelP>,
    #[id = "buffer"]
    buffer: BoolParam,
    #[id = "pickup"]
    pickup: EnumParam<PickupP>,
    #[id = "bias"]
    bias: FloatParam,
    #[id = "temp"]
    temp: FloatParam,
    #[id = "leakage"]
    leakage: FloatParam,
    #[id = "voltage"]
    voltage: FloatParam,
    #[id = "sag"]
    sag: FloatParam,
    #[id = "stack"]
    stack: EnumParam<StackP>,
    #[id = "mids"]
    mids: FloatParam,
    #[id = "input"]
    input: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "bass_keep"]
    bass_keep: FloatParam,
    #[id = "bypass"]
    bypass: BoolParam,
    /// Read-only meters (golem contract): the trimmed transistor's collector, the circuit's supply.
    #[id = "bias_v"]
    bias_v: FloatParam,
    #[id = "supply_v"]
    supply_v: FloatParam,
}

impl Default for FuzzParams {
    fn default() -> Self {
        Self {
            fuzz: f(0, " %", 0),
            tone: f(1, " %", 0),
            level: f(2, " dB", 1),
            guitar_vol: f(3, " %", 0),
            model: EnumParam::new("Model", ModelP::GePair),
            buffer: BoolParam::new("Buffer", false),
            pickup: EnumParam::new("Pickup", PickupP::SingleCoil),
            bias: f(7, " %", 0),
            temp: f(8, " °C", 1),
            leakage: f(9, " %", 0),
            voltage: f(10, " V", 1),
            sag: f(11, " %", 0),
            stack: EnumParam::new("Tone Stack", StackP::Classic),
            mids: f(13, " %", 0),
            input: f(14, " dB", 1),
            mix: f(15, " %", 0),
            bass_keep: f(16, " Hz", 0),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            bias_v: meter("Bias (V)"),
            supply_v: meter("Supply (V)"),
        }
    }
}

impl FuzzParams {
    fn values(&self) -> [f32; NP] {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        [
            self.fuzz.value(),
            self.tone.value(),
            self.level.value(),
            self.guitar_vol.value(),
            self.model.value() as usize as f32,
            b(self.buffer.value()),
            self.pickup.value() as usize as f32,
            self.bias.value(),
            self.temp.value(),
            self.leakage.value(),
            self.voltage.value(),
            self.sag.value(),
            self.stack.value() as usize as f32,
            self.mids.value(),
            self.input.value(),
            self.mix.value(),
            self.bass_keep.value(),
            b(self.bypass.value()),
        ]
    }
}

pub struct GfxFuzz {
    params: Arc<FuzzParams>,
    dsp: Fuzz,
    last: [f32; NP],
}

impl Default for GfxFuzz {
    fn default() -> Self {
        Self {
            params: Arc::new(FuzzParams::default()),
            dsp: Fuzz::new(),
            last: [f32::NAN; NP],
        }
    }
}

impl GfxFuzz {
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

impl Plugin for GfxFuzz {
    const NAME: &'static str = "GoleM FX Fuzz";
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
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-fuzz"))
    }

    fn initialize(
        &mut self,
        layout: &AudioIOLayout,
        config: &BufferConfig,
        _ctx: &mut impl InitContext<Self>,
    ) -> bool {
        let channels = layout.main_output_channels.map_or(1, |c| c.get() as usize);
        self.sync(true);
        // every configuration compiled and biased here (≈ 6 ms), never in process
        self.dsp.prepare(
            config.sample_rate,
            config.max_buffer_size as usize,
            channels,
        );
        true
    }

    fn reset(&mut self) {
        self.sync(true);
        self.dsp.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _ctx: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.sync(false);
        self.dsp.process(buffer.as_slice());
        self.params
            .bias_v
            .set_output_value(self.dsp.bias_v().clamp(0.0, 18.0));
        self.params
            .supply_v
            .set_output_value(self.dsp.supply_v().clamp(0.0, 18.0));
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxFuzz {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxFuzz00001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_vst3!(GfxFuzz);
