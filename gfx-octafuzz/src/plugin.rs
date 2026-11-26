// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second (linked mono while L == R, as
//! gfx-fuzz). Linear ranges only (golem contract). Reported latency 0 (minimum-phase oversampling, a
//! few samples of group delay, the gfx-fuzz precedent). `bypass` is the VST3 bypass parameter (10 ms
//! crossfade). `bias_v` (Q3's collector) and `supply_v` are read-only meters. Generated from PARAMS
//! (same order and ids).

use crate::{Octafuzz, NP, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

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
struct OctafuzzParams {
    #[id = "fuzz"]
    fuzz: FloatParam,
    #[id = "octave"]
    octave: FloatParam,
    #[id = "tone"]
    tone: FloatParam,
    #[id = "level"]
    level: FloatParam,
    #[id = "guitar_vol"]
    guitar_vol: FloatParam,
    #[id = "buffer"]
    buffer: BoolParam,
    #[id = "pickup"]
    pickup: EnumParam<PickupP>,
    #[id = "bias"]
    bias: FloatParam,
    #[id = "temp"]
    temp: FloatParam,
    #[id = "voltage"]
    voltage: FloatParam,
    #[id = "sag"]
    sag: FloatParam,
    #[id = "input"]
    input: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "bass_keep"]
    bass_keep: FloatParam,
    #[id = "bypass"]
    bypass: BoolParam,
    /// Read-only meters (golem contract): Q3's collector (the transformer's driver), the supply.
    #[id = "bias_v"]
    bias_v: FloatParam,
    #[id = "supply_v"]
    supply_v: FloatParam,
}

impl Default for OctafuzzParams {
    fn default() -> Self {
        Self {
            fuzz: f(0, " %", 0),
            octave: f(1, " %", 0),
            tone: f(2, " %", 0),
            level: f(3, " dB", 1),
            guitar_vol: f(4, " %", 0),
            buffer: BoolParam::new("Buffer", false),
            pickup: EnumParam::new("Pickup", PickupP::SingleCoil),
            bias: f(7, " %", 0),
            temp: f(8, " °C", 1),
            voltage: f(9, " V", 1),
            sag: f(10, " %", 0),
            input: f(11, " dB", 1),
            mix: f(12, " %", 0),
            bass_keep: f(13, " Hz", 0),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            bias_v: meter("Bias (V)"),
            supply_v: meter("Supply (V)"),
        }
    }
}

impl OctafuzzParams {
    fn values(&self) -> [f32; NP] {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        [
            self.fuzz.value(),
            self.octave.value(),
            self.tone.value(),
            self.level.value(),
            self.guitar_vol.value(),
            b(self.buffer.value()),
            self.pickup.value() as usize as f32,
            self.bias.value(),
            self.temp.value(),
            self.voltage.value(),
            self.sag.value(),
            self.input.value(),
            self.mix.value(),
            self.bass_keep.value(),
            b(self.bypass.value()),
        ]
    }
}

pub struct GfxOctafuzz {
    params: Arc<OctafuzzParams>,
    dsp: Octafuzz,
    last: [f32; NP],
}

impl Default for GfxOctafuzz {
    fn default() -> Self {
        Self {
            params: Arc::new(OctafuzzParams::default()),
            dsp: Octafuzz::new(),
            last: [f32::NAN; NP],
        }
    }
}

impl GfxOctafuzz {
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

impl Plugin for GfxOctafuzz {
    const NAME: &'static str = "GoleM FX Octafuzz";
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
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-octafuzz"))
    }

    fn initialize(
        &mut self,
        layout: &AudioIOLayout,
        config: &BufferConfig,
        _ctx: &mut impl InitContext<Self>,
    ) -> bool {
        let channels = layout.main_output_channels.map_or(1, |c| c.get() as usize);
        self.sync(true);
        // every configuration compiled and biased here, never in process
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

impl Vst3Plugin for GfxOctafuzz {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxOctaFz001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_vst3!(GfxOctafuzz);
