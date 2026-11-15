// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): stereo 2 / 2 only (stereo by nature: golem turns the lane stereo here,
//! a mono DI arrives on both inputs; a mono widener would have nothing to widen into). Linear ranges
//! only (golem contract). `bypass` is the VST3 bypass parameter (20 ms crossfade). Zero latency.
//! `corr` and `mono_err` are read-only meters. Generated from PARAMS.

use crate::{Widener, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModeP {
    #[id = "velvet"]
    #[name = "Velvet"]
    Velvet,
    #[id = "allpass"]
    #[name = "Allpass"]
    Allpass,
    #[id = "comb"]
    #[name = "Comb"]
    Comb,
    #[id = "double"]
    #[name = "Double"]
    Double,
    #[id = "ms"]
    #[name = "M/S"]
    Ms,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum LawP {
    #[id = "exact"]
    #[name = "Exact"]
    Exact,
    #[id = "classic"]
    #[name = "Classic"]
    Classic,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ListenP {
    #[id = "stereo"]
    #[name = "Stereo"]
    Stereo,
    #[id = "mono"]
    #[name = "Mono"]
    Mono,
    #[id = "side"]
    #[name = "Side"]
    Side,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

fn int(i: usize) -> IntParam {
    let pi = PARAMS[i];
    IntParam::new(pi.name, pi.default as i32, IntRange::Linear { min: pi.min as i32, max: pi.max as i32 })
}

#[derive(Params)]
struct WidenerParams {
    #[id = "mode"]
    mode: EnumParam<ModeP>,
    #[id = "amount"]
    amount: FloatParam,
    #[id = "width"]
    width: FloatParam,
    #[id = "time"]
    time: FloatParam,
    #[id = "bass_keep"]
    bass_keep: FloatParam,
    #[id = "mono_law"]
    mono_law: EnumParam<LawP>,
    #[id = "variant"]
    variant: IntParam,
    #[id = "cents"]
    cents: FloatParam,
    #[id = "high_cut"]
    high_cut: FloatParam,
    #[id = "transient"]
    transient: FloatParam,
    #[id = "balance"]
    balance: FloatParam,
    #[id = "rotate"]
    rotate: FloatParam,
    #[id = "level"]
    level: FloatParam,
    #[id = "listen"]
    listen: EnumParam<ListenP>,
    #[id = "bypass"]
    bypass: BoolParam,
    /// Read-only meters (golem contract): output correlation, fold vs the dry fold (dB).
    #[id = "corr"]
    corr: FloatParam,
    #[id = "mono_err"]
    mono_err: FloatParam,
}

impl Default for WidenerParams {
    fn default() -> Self {
        Self {
            mode: EnumParam::new("Mode", ModeP::Velvet),
            amount: f(1, " %", 0),
            width: f(2, " %", 0),
            time: f(3, " ms", 1),
            bass_keep: f(4, " Hz", 0),
            mono_law: EnumParam::new("Mono Law", LawP::Exact),
            variant: int(6),
            cents: f(7, " ct", 1),
            high_cut: f(8, " Hz", 0),
            transient: f(9, " %", 0),
            balance: f(10, " %", 0),
            rotate: f(11, " °", 1),
            level: f(12, " dB", 1),
            listen: EnumParam::new("Listen", ListenP::Stereo),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            corr: FloatParam::new("Correlation", 1.0, FloatRange::Linear { min: -1.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2))
                .read_only(),
            mono_err: FloatParam::new("Mono Error", -80.0, FloatRange::Linear { min: -80.0, max: 20.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1))
                .read_only(),
        }
    }
}

impl WidenerParams {
    fn values(&self) -> [f32; crate::NP] {
        [
            self.mode.value() as usize as f32,
            self.amount.value(),
            self.width.value(),
            self.time.value(),
            self.bass_keep.value(),
            self.mono_law.value() as usize as f32,
            self.variant.value() as f32,
            self.cents.value(),
            self.high_cut.value(),
            self.transient.value(),
            self.balance.value(),
            self.rotate.value(),
            self.level.value(),
            self.listen.value() as usize as f32,
            if self.bypass.value() { 1.0 } else { 0.0 },
        ]
    }
}

pub struct GfxWidener {
    params: Arc<WidenerParams>,
    dsp: Widener,
    last: [f32; crate::NP],
}

impl Default for GfxWidener {
    fn default() -> Self {
        Self { params: Arc::new(WidenerParams::default()), dsp: Widener::new(), last: [f32::NAN; crate::NP] }
    }
}

impl GfxWidener {
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

impl Plugin for GfxWidener {
    const NAME: &'static str = "GoleM FX Widener";
    const VENDOR: &'static str = "GoleM";
    const URL: &'static str = "https://golem.fm";
    const EMAIL: &'static str = "contact@golem.fm";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // stereo by nature, 2 / 2 only: golem turns the lane stereo here (README §3.3)
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(&mut self, layout: &AudioIOLayout, config: &BufferConfig, _ctx: &mut impl InitContext<Self>) -> bool {
        let channels = layout.main_output_channels.map_or(2, |c| c.get() as usize);
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
        self.params.corr.set_output_value(self.dsp.corr());
        self.params.mono_err.set_output_value(self.dsp.mono_err_db());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxWidener {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxWidener01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Spatial];
}

nih_export_vst3!(GfxWidener);
