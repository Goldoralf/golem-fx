// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second (not stereo by nature; in 1 / 1
//! the stereo modules are inert). Linear ranges only (golem contract). The module values display in
//! their module's unit (dB, bits, Hz, exponent), read from the slot types the processor last saw; the
//! crossovers in Hz. `bypass` is the VST3 bypass parameter (a 10 ms crossfade). Zero latency.
//! Generated from PARAMS (same order and ids).

use crate::modules::Kind;
use crate::{format_amount, Grit, NP, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModuleP {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "soft-clip"]
    #[name = "Soft clip"]
    Soft,
    #[id = "hard-clip"]
    #[name = "Hard clip"]
    Hard,
    #[id = "heavy"]
    #[name = "Heavy"]
    Heavy,
    #[id = "asym"]
    #[name = "Asym"]
    Asym,
    #[id = "rectify"]
    #[name = "Rectify"]
    Rectify,
    #[id = "fold"]
    #[name = "Fold"]
    Fold,
    #[id = "sine-fold"]
    #[name = "Sine fold"]
    SineFold,
    #[id = "zero-cross"]
    #[name = "Zero cross"]
    ZeroCross,
    #[id = "bit-crush"]
    #[name = "Bit crush"]
    BitCrush,
    #[id = "sample-divide"]
    #[name = "Sample divide"]
    SampleDivide,
    #[id = "dc"]
    #[name = "DC"]
    Dc,
    #[id = "width"]
    #[name = "Width"]
    Width,
    #[id = "gain"]
    #[name = "Gain"]
    Gain,
    #[id = "pan"]
    #[name = "Pan"]
    Pan,
    #[id = "lowpass"]
    #[name = "Lowpass"]
    Lowpass,
    #[id = "highpass"]
    #[name = "Highpass"]
    Highpass,
    #[id = "peak"]
    #[name = "Peak"]
    Peak,
    #[id = "dry-wet"]
    #[name = "Dry/wet"]
    DryWet,
    #[id = "stereo-rectify"]
    #[name = "Stereo rectify"]
    StereoRectify,
    #[id = "stereo-dc"]
    #[name = "Stereo DC"]
    StereoDc,
    #[id = "ring-mod"]
    #[name = "Ring mod"]
    RingMod,
    #[id = "cabinet"]
    #[name = "Cabinet"]
    Cabinet,
    #[id = "tube"]
    #[name = "Tube"]
    Tube,
    #[id = "fuzz"]
    #[name = "Fuzz"]
    Fuzz,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SlopeP {
    #[id = "24"]
    #[name = "24 dB/oct"]
    S24,
    #[id = "48"]
    #[name = "48 dB/oct"]
    S48,
}

/// The slot types (8 slots, band-major), as the processor last saw them: the module values' display.
type Kinds = Arc<[AtomicU8; 8]>;

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// A module value (`f(..)` gives the descriptors' unit and decimals): % in, shown with the module's
/// own unit (dB, bits, Hz, exponent) for the slot's current type.
fn amount(p: FloatParam, kinds: &Kinds, slot: usize) -> FloatParam {
    let k = kinds.clone();
    p.with_unit("").with_value_to_string(Arc::new(move |v| {
        let kind = Kind::from_index(k[slot].load(Ordering::Relaxed) as usize);
        format!("{v:.0} % ({})", format_amount(kind, v as f64))
    }))
}

/// A crossover: %, shown with its frequency (20 · 1000^(x / 100), before the ordering push).
fn crossover(p: FloatParam) -> FloatParam {
    p.with_unit("").with_value_to_string(Arc::new(|v| {
        let hz = crate::curves::freq_up(v as f64);
        if hz >= 1000.0 {
            format!("{v:.0} % ({:.2} kHz)", hz / 1000.0)
        } else {
            format!("{v:.0} % ({hz:.0} Hz)")
        }
    }))
}

#[derive(Params)]
struct GritParams {
    #[id = "lo_m1"]
    lo_m1: EnumParam<ModuleP>,
    #[id = "lo_v1"]
    lo_v1: FloatParam,
    #[id = "lo_m2"]
    lo_m2: EnumParam<ModuleP>,
    #[id = "lo_v2"]
    lo_v2: FloatParam,
    #[id = "lo_gain"]
    lo_gain: FloatParam,
    #[id = "lm_m1"]
    lm_m1: EnumParam<ModuleP>,
    #[id = "lm_v1"]
    lm_v1: FloatParam,
    #[id = "lm_m2"]
    lm_m2: EnumParam<ModuleP>,
    #[id = "lm_v2"]
    lm_v2: FloatParam,
    #[id = "lm_gain"]
    lm_gain: FloatParam,
    #[id = "hm_m1"]
    hm_m1: EnumParam<ModuleP>,
    #[id = "hm_v1"]
    hm_v1: FloatParam,
    #[id = "hm_m2"]
    hm_m2: EnumParam<ModuleP>,
    #[id = "hm_v2"]
    hm_v2: FloatParam,
    #[id = "hm_gain"]
    hm_gain: FloatParam,
    #[id = "hi_m1"]
    hi_m1: EnumParam<ModuleP>,
    #[id = "hi_v1"]
    hi_v1: FloatParam,
    #[id = "hi_m2"]
    hi_m2: EnumParam<ModuleP>,
    #[id = "hi_v2"]
    hi_v2: FloatParam,
    #[id = "hi_gain"]
    hi_gain: FloatParam,
    #[id = "x1"]
    x1: FloatParam,
    #[id = "x2"]
    x2: FloatParam,
    #[id = "x3"]
    x3: FloatParam,
    #[id = "wet"]
    wet: FloatParam,
    #[id = "quality"]
    quality: BoolParam,
    #[id = "drive"]
    drive: FloatParam,
    #[id = "focus"]
    focus: FloatParam,
    #[id = "level"]
    level: FloatParam,
    #[id = "lo_mix"]
    lo_mix: FloatParam,
    #[id = "lo_y1"]
    lo_y1: FloatParam,
    #[id = "lo_y2"]
    lo_y2: FloatParam,
    #[id = "lo_solo"]
    lo_solo: BoolParam,
    #[id = "lo_bypass"]
    lo_bypass: BoolParam,
    #[id = "lm_mix"]
    lm_mix: FloatParam,
    #[id = "lm_y1"]
    lm_y1: FloatParam,
    #[id = "lm_y2"]
    lm_y2: FloatParam,
    #[id = "lm_solo"]
    lm_solo: BoolParam,
    #[id = "lm_bypass"]
    lm_bypass: BoolParam,
    #[id = "hm_mix"]
    hm_mix: FloatParam,
    #[id = "hm_y1"]
    hm_y1: FloatParam,
    #[id = "hm_y2"]
    hm_y2: FloatParam,
    #[id = "hm_solo"]
    hm_solo: BoolParam,
    #[id = "hm_bypass"]
    hm_bypass: BoolParam,
    #[id = "hi_mix"]
    hi_mix: FloatParam,
    #[id = "hi_y1"]
    hi_y1: FloatParam,
    #[id = "hi_y2"]
    hi_y2: FloatParam,
    #[id = "hi_solo"]
    hi_solo: BoolParam,
    #[id = "hi_bypass"]
    hi_bypass: BoolParam,
    #[id = "slope"]
    slope: EnumParam<SlopeP>,
    #[id = "bypass"]
    bypass: BoolParam,
    kinds: Kinds,
}

impl Default for GritParams {
    fn default() -> Self {
        let kinds: Kinds = Arc::new(Default::default());
        Self {
            lo_m1: EnumParam::new(PARAMS[0].name, ModuleP::Off),
            lo_v1: amount(f(1, " %", 0), &kinds, 0),
            lo_m2: EnumParam::new(PARAMS[2].name, ModuleP::Off),
            lo_v2: amount(f(3, " %", 0), &kinds, 1),
            lo_gain: f(4, " dB", 1),
            lm_m1: EnumParam::new(PARAMS[5].name, ModuleP::Off),
            lm_v1: amount(f(6, " %", 0), &kinds, 2),
            lm_m2: EnumParam::new(PARAMS[7].name, ModuleP::Off),
            lm_v2: amount(f(8, " %", 0), &kinds, 3),
            lm_gain: f(9, " dB", 1),
            hm_m1: EnumParam::new(PARAMS[10].name, ModuleP::Off),
            hm_v1: amount(f(11, " %", 0), &kinds, 4),
            hm_m2: EnumParam::new(PARAMS[12].name, ModuleP::Off),
            hm_v2: amount(f(13, " %", 0), &kinds, 5),
            hm_gain: f(14, " dB", 1),
            hi_m1: EnumParam::new(PARAMS[15].name, ModuleP::Off),
            hi_v1: amount(f(16, " %", 0), &kinds, 6),
            hi_m2: EnumParam::new(PARAMS[17].name, ModuleP::Off),
            hi_v2: amount(f(18, " %", 0), &kinds, 7),
            hi_gain: f(19, " dB", 1),
            x1: crossover(f(20, " %", 0)),
            x2: crossover(f(21, " %", 0)),
            x3: crossover(f(22, " %", 0)),
            wet: f(23, " %", 0),
            quality: BoolParam::new(PARAMS[24].name, false),
            drive: f(25, " %", 0),
            focus: f(26, " %", 0),
            level: f(27, " dB", 1),
            lo_mix: f(28, " %", 0),
            lo_y1: f(29, " dB", 1),
            lo_y2: f(30, " dB", 1),
            lo_solo: BoolParam::new(PARAMS[31].name, false),
            lo_bypass: BoolParam::new(PARAMS[32].name, false),
            lm_mix: f(33, " %", 0),
            lm_y1: f(34, " dB", 1),
            lm_y2: f(35, " dB", 1),
            lm_solo: BoolParam::new(PARAMS[36].name, false),
            lm_bypass: BoolParam::new(PARAMS[37].name, false),
            hm_mix: f(38, " %", 0),
            hm_y1: f(39, " dB", 1),
            hm_y2: f(40, " dB", 1),
            hm_solo: BoolParam::new(PARAMS[41].name, false),
            hm_bypass: BoolParam::new(PARAMS[42].name, false),
            hi_mix: f(43, " %", 0),
            hi_y1: f(44, " dB", 1),
            hi_y2: f(45, " dB", 1),
            hi_solo: BoolParam::new(PARAMS[46].name, false),
            hi_bypass: BoolParam::new(PARAMS[47].name, false),
            slope: EnumParam::new(PARAMS[48].name, SlopeP::S24),
            bypass: BoolParam::new(PARAMS[49].name, false).make_bypass(),
            kinds,
        }
    }
}

impl GritParams {
    fn values(&self) -> [f32; NP] {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        [
            self.lo_m1.value() as usize as f32,
            self.lo_v1.value(),
            self.lo_m2.value() as usize as f32,
            self.lo_v2.value(),
            self.lo_gain.value(),
            self.lm_m1.value() as usize as f32,
            self.lm_v1.value(),
            self.lm_m2.value() as usize as f32,
            self.lm_v2.value(),
            self.lm_gain.value(),
            self.hm_m1.value() as usize as f32,
            self.hm_v1.value(),
            self.hm_m2.value() as usize as f32,
            self.hm_v2.value(),
            self.hm_gain.value(),
            self.hi_m1.value() as usize as f32,
            self.hi_v1.value(),
            self.hi_m2.value() as usize as f32,
            self.hi_v2.value(),
            self.hi_gain.value(),
            self.x1.value(),
            self.x2.value(),
            self.x3.value(),
            self.wet.value(),
            b(self.quality.value()),
            self.drive.value(),
            self.focus.value(),
            self.level.value(),
            self.lo_mix.value(),
            self.lo_y1.value(),
            self.lo_y2.value(),
            b(self.lo_solo.value()),
            b(self.lo_bypass.value()),
            self.lm_mix.value(),
            self.lm_y1.value(),
            self.lm_y2.value(),
            b(self.lm_solo.value()),
            b(self.lm_bypass.value()),
            self.hm_mix.value(),
            self.hm_y1.value(),
            self.hm_y2.value(),
            b(self.hm_solo.value()),
            b(self.hm_bypass.value()),
            self.hi_mix.value(),
            self.hi_y1.value(),
            self.hi_y2.value(),
            b(self.hi_solo.value()),
            b(self.hi_bypass.value()),
            self.slope.value() as usize as f32,
            b(self.bypass.value()),
        ]
    }
}

pub struct GfxGrit {
    params: Arc<GritParams>,
    dsp: Grit,
    last: [f32; NP],
}

impl Default for GfxGrit {
    fn default() -> Self {
        Self { params: Arc::new(GritParams::default()), dsp: Grit::new(), last: [f32::NAN; NP] }
    }
}

impl GfxGrit {
    fn sync(&mut self, force: bool) {
        let v = self.params.values();
        for (k, p) in PARAMS.iter().enumerate() {
            if force || v[k] != self.last[k] {
                self.last[k] = v[k];
                self.dsp.set_param(p.id, v[k]);
            }
        }
        for b in 0..4 {
            for s in 0..2 {
                self.params.kinds[2 * b + s].store(v[crate::p::m(b, s)] as u8, Ordering::Relaxed);
            }
        }
    }
}

impl Plugin for GfxGrit {
    const NAME: &'static str = "GoleM FX Grit";
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
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-grit"))
    }

    fn initialize(&mut self, layout: &AudioIOLayout, config: &BufferConfig, ctx: &mut impl InitContext<Self>) -> bool {
        let channels = layout.main_output_channels.map_or(1, |c| c.get() as usize);
        self.sync(true);
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
        ctx.set_latency_samples(0);
        true
    }

    fn reset(&mut self) {
        self.sync(true);
        self.dsp.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.sync(false);
        self.dsp.process(buffer.as_slice());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxGrit {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxGrit00001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_vst3!(GfxGrit);
