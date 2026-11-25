// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second (the two-microphone pair,
//! README §2.9). Linear ranges only (golem contract). Reported latency 0. `bypass` is the VST3
//! bypass parameter (10 ms crossfade). `gr` is the shared read-only meter. Generated from PARAMS
//! (same order and ids).

use crate::{Acoustic, NP, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModelP {
    #[id = "dreadnought"]
    #[name = "Dreadnought"]
    Dreadnought,
    #[id = "jumbo"]
    #[name = "Jumbo"]
    Jumbo,
    #[id = "concert"]
    #[name = "Concert"]
    Concert,
    #[id = "parlor"]
    #[name = "Parlor"]
    Parlor,
    #[id = "classical"]
    #[name = "Classical"]
    Classical,
    #[id = "round-back"]
    #[name = "Round-back"]
    RoundBack,
}

/// gfx-fuzz's four options in its order, `Piezo` appended here only.
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
    #[id = "piezo"]
    #[name = "Piezo"]
    Piezo,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum PosP {
    #[id = "neck"]
    #[name = "Neck"]
    Neck,
    #[id = "middle"]
    #[name = "Middle"]
    Middle,
    #[id = "bridge"]
    #[name = "Bridge"]
    Bridge,
    #[id = "neck-middle"]
    #[name = "Neck + Middle"]
    NeckMiddle,
    #[id = "middle-bridge"]
    #[name = "Middle + Bridge"]
    MiddleBridge,
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

#[derive(Params)]
struct AcousticParams {
    #[id = "model"]
    model: EnumParam<ModelP>,
    #[id = "body"]
    body: FloatParam,
    #[id = "top"]
    top: FloatParam,
    #[id = "mic"]
    mic: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "twelve"]
    twelve: BoolParam,
    #[id = "pickup"]
    pickup: EnumParam<PickupP>,
    #[id = "pickup_pos"]
    pickup_pos: EnumParam<PosP>,
    #[id = "size"]
    size: FloatParam,
    #[id = "decay"]
    decay: FloatParam,
    #[id = "piezo"]
    piezo: FloatParam,
    #[id = "quack"]
    quack: FloatParam,
    #[id = "low_cut"]
    low_cut: FloatParam,
    #[id = "comp"]
    comp: FloatParam,
    #[id = "room"]
    room: FloatParam,
    #[id = "width"]
    width: FloatParam,
    #[id = "twelve_level"]
    twelve_level: FloatParam,
    #[id = "twelve_detune"]
    twelve_detune: FloatParam,
    #[id = "twelve_delay"]
    twelve_delay: FloatParam,
    #[id = "level"]
    level: FloatParam,
    #[id = "bypass"]
    bypass: BoolParam,
    /// Read-only meter (golem contract): the compressor's gain reduction.
    #[id = "gr"]
    gr: FloatParam,
}

impl Default for AcousticParams {
    fn default() -> Self {
        Self {
            model: EnumParam::new("Model", ModelP::Dreadnought),
            body: f(1, " %", 0),
            top: f(2, " %", 0),
            mic: f(3, " %", 0),
            mix: f(4, " %", 0),
            twelve: BoolParam::new("12-String", false),
            pickup: EnumParam::new("Pickup", PickupP::SingleCoil),
            pickup_pos: EnumParam::new("Pickup Position", PosP::Neck),
            size: f(8, " %", 0),
            decay: f(9, " %", 0),
            piezo: f(10, " %", 0),
            quack: f(11, " %", 0),
            low_cut: f(12, " Hz", 0),
            comp: f(13, " %", 0),
            room: f(14, " %", 0),
            width: f(15, " %", 0),
            twelve_level: f(16, " dB", 1),
            twelve_detune: f(17, " ct", 1),
            twelve_delay: f(18, " ms", 1),
            level: f(19, " dB", 1),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            gr: FloatParam::new("Comp GR", 0.0, FloatRange::Linear { min: 0.0, max: 30.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1))
                .read_only(),
        }
    }
}

impl AcousticParams {
    fn values(&self) -> [f32; NP] {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        [
            self.model.value() as usize as f32,
            self.body.value(),
            self.top.value(),
            self.mic.value(),
            self.mix.value(),
            b(self.twelve.value()),
            self.pickup.value() as usize as f32,
            self.pickup_pos.value() as usize as f32,
            self.size.value(),
            self.decay.value(),
            self.piezo.value(),
            self.quack.value(),
            self.low_cut.value(),
            self.comp.value(),
            self.room.value(),
            self.width.value(),
            self.twelve_level.value(),
            self.twelve_detune.value(),
            self.twelve_delay.value(),
            self.level.value(),
            b(self.bypass.value()),
        ]
    }
}

pub struct GfxAcoustic {
    params: Arc<AcousticParams>,
    dsp: Acoustic,
    last: [f32; NP],
}

impl Default for GfxAcoustic {
    fn default() -> Self {
        Self {
            params: Arc::new(AcousticParams::default()),
            dsp: Acoustic::new(),
            last: [f32::NAN; NP],
        }
    }
}

impl GfxAcoustic {
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

impl Plugin for GfxAcoustic {
    const NAME: &'static str = "GoleM FX Acoustic";
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
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-acoustic"))
    }

    fn initialize(
        &mut self,
        layout: &AudioIOLayout,
        config: &BufferConfig,
        _ctx: &mut impl InitContext<Self>,
    ) -> bool {
        let channels = layout.main_output_channels.map_or(1, |c| c.get() as usize);
        self.sync(true);
        // the body kernels are resampled to the host rate here (a few ms), never in process
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
            .gr
            .set_output_value(self.dsp.gain_reduction_db().clamp(0.0, 30.0));
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxAcoustic {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxAcoustic1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx];
}

nih_export_vst3!(GfxAcoustic);
