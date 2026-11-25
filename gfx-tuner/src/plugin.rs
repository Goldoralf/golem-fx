// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second (the analysis reads (L + R) / 2).
//! Zero latency. `bypass` is the VST3 bypass parameter: on = the tuner is put away (audio bit-exact,
//! analysis only with `always_on`). The 23 meters are read-only parameters (golem's fork:
//! `read_only()` + `set_output_value`), in [`crate::METERS`] order. Linear ranges only (golem
//! contract). Generated from PARAMS.

use crate::{Tuner, NP, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

/// `tuning`: rank = value, append-only (README §3.2). golem sends Chromatic, or Custom + `string_1…8`.
#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum TuningP {
    #[id = "chromatic"]
    #[name = "Chromatic"]
    Chromatic,
    #[id = "custom"]
    #[name = "Custom"]
    Custom,
    #[id = "6-std"]
    #[name = "6-std"]
    T6Std,
    #[id = "eb-std"]
    #[name = "eb-std"]
    TEbStd,
    #[id = "d-std"]
    #[name = "d-std"]
    TDStd,
    #[id = "drop-d"]
    #[name = "drop-d"]
    TDropD,
    #[id = "dadgad"]
    #[name = "dadgad"]
    TDadgad,
    #[id = "drop-c"]
    #[name = "drop-c"]
    TDropC,
    #[id = "7-std"]
    #[name = "7-std"]
    T7Std,
    #[id = "7-drop-a"]
    #[name = "7-drop-a"]
    T7DropA,
    #[id = "7-gdgcfad"]
    #[name = "7-gdgcfad"]
    T7Gdgcfad,
    #[id = "8-std"]
    #[name = "8-std"]
    T8Std,
    #[id = "8-e"]
    #[name = "8-e"]
    T8E,
    #[id = "8-drop-e"]
    #[name = "8-drop-e"]
    T8DropE,
    #[id = "8-dbdadgbe"]
    #[name = "8-dbdadgbe"]
    T8Dbdadgbe,
    #[id = "bass-4-std"]
    #[name = "bass-4-std"]
    TBass4Std,
    #[id = "bass-4-eb"]
    #[name = "bass-4-eb"]
    TBass4Eb,
    #[id = "bass-4-d-std"]
    #[name = "bass-4-d-std"]
    TBass4DStd,
    #[id = "bass-4-drop-d"]
    #[name = "bass-4-drop-d"]
    TBass4DropD,
    #[id = "bass-5-std"]
    #[name = "bass-5-std"]
    TBass5Std,
    #[id = "bass-5-high-c"]
    #[name = "bass-5-high-c"]
    TBass5HighC,
    #[id = "bass-5-drop-a"]
    #[name = "bass-5-drop-a"]
    TBass5DropA,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModeP {
    #[id = "auto"]
    Auto,
    #[id = "mono"]
    Mono,
    #[id = "poly"]
    Poly,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum PrecisionP {
    #[id = "fast"]
    Fast,
    #[id = "strobe"]
    Strobe,
}

fn lin(min: f32, max: f32) -> FloatRange {
    FloatRange::Linear { min, max }
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, lin(pi.min, pi.max))
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

fn int(i: usize, unit: &'static str) -> IntParam {
    let pi = PARAMS[i];
    IntParam::new(pi.name, pi.default as i32, IntRange::Linear { min: pi.min as i32, max: pi.max as i32 }).with_unit(unit)
}

fn b(v: bool) -> f32 {
    if v {
        1.0
    } else {
        0.0
    }
}

#[derive(Params)]
struct TunerParams {
    #[id = "tuning"]
    tuning: EnumParam<TuningP>,
    #[id = "mode"]
    mode: EnumParam<ModeP>,
    #[id = "precision"]
    precision: EnumParam<PrecisionP>,
    #[id = "reference"]
    reference: FloatParam,
    #[id = "mute"]
    mute: BoolParam,
    #[id = "bypass"]
    bypass: BoolParam,
    #[id = "transpose"]
    transpose: IntParam,
    #[id = "offset_1"]
    offset_1: FloatParam,
    #[id = "offset_2"]
    offset_2: FloatParam,
    #[id = "offset_3"]
    offset_3: FloatParam,
    #[id = "offset_4"]
    offset_4: FloatParam,
    #[id = "offset_5"]
    offset_5: FloatParam,
    #[id = "offset_6"]
    offset_6: FloatParam,
    #[id = "offset_7"]
    offset_7: FloatParam,
    #[id = "offset_8"]
    offset_8: FloatParam,
    #[id = "string_1"]
    string_1: IntParam,
    #[id = "string_2"]
    string_2: IntParam,
    #[id = "string_3"]
    string_3: IntParam,
    #[id = "string_4"]
    string_4: IntParam,
    #[id = "string_5"]
    string_5: IntParam,
    #[id = "string_6"]
    string_6: IntParam,
    #[id = "string_7"]
    string_7: IntParam,
    #[id = "string_8"]
    string_8: IntParam,
    #[id = "threshold"]
    threshold: FloatParam,
    #[id = "always_on"]
    always_on: BoolParam,
    // Read-only meters (golem contract, README §3.5), published once per 10 ms hop.
    #[id = "freq"]
    m_freq: FloatParam,
    #[id = "note"]
    m_note: FloatParam,
    #[id = "cents"]
    m_cents: FloatParam,
    #[id = "confidence"]
    m_confidence: FloatParam,
    #[id = "string"]
    m_string: FloatParam,
    #[id = "state"]
    m_state: FloatParam,
    #[id = "seq"]
    m_seq: FloatParam,
    #[id = "s1_cents"]
    m_s1_cents: FloatParam,
    #[id = "s2_cents"]
    m_s2_cents: FloatParam,
    #[id = "s3_cents"]
    m_s3_cents: FloatParam,
    #[id = "s4_cents"]
    m_s4_cents: FloatParam,
    #[id = "s5_cents"]
    m_s5_cents: FloatParam,
    #[id = "s6_cents"]
    m_s6_cents: FloatParam,
    #[id = "s7_cents"]
    m_s7_cents: FloatParam,
    #[id = "s8_cents"]
    m_s8_cents: FloatParam,
    #[id = "s1_conf"]
    m_s1_conf: FloatParam,
    #[id = "s2_conf"]
    m_s2_conf: FloatParam,
    #[id = "s3_conf"]
    m_s3_conf: FloatParam,
    #[id = "s4_conf"]
    m_s4_conf: FloatParam,
    #[id = "s5_conf"]
    m_s5_conf: FloatParam,
    #[id = "s6_conf"]
    m_s6_conf: FloatParam,
    #[id = "s7_conf"]
    m_s7_conf: FloatParam,
    #[id = "s8_conf"]
    m_s8_conf: FloatParam,
}

impl Default for TunerParams {
    fn default() -> Self {
        Self {
            tuning: EnumParam::new(PARAMS[0].name, TuningP::Chromatic),
            mode: EnumParam::new(PARAMS[1].name, ModeP::Auto),
            precision: EnumParam::new(PARAMS[2].name, PrecisionP::Fast),
            reference: f(3, " Hz", 1),
            mute: BoolParam::new(PARAMS[4].name, false),
            bypass: BoolParam::new(PARAMS[5].name, false).make_bypass(),
            transpose: int(6, " st"),
            offset_1: f(7, " ct", 1),
            offset_2: f(8, " ct", 1),
            offset_3: f(9, " ct", 1),
            offset_4: f(10, " ct", 1),
            offset_5: f(11, " ct", 1),
            offset_6: f(12, " ct", 1),
            offset_7: f(13, " ct", 1),
            offset_8: f(14, " ct", 1),
            string_1: int(15, ""),
            string_2: int(16, ""),
            string_3: int(17, ""),
            string_4: int(18, ""),
            string_5: int(19, ""),
            string_6: int(20, ""),
            string_7: int(21, ""),
            string_8: int(22, ""),
            threshold: f(23, " dBFS", 1),
            always_on: BoolParam::new(PARAMS[24].name, false),
            m_freq: FloatParam::new("Frequency", 0.0, lin(0.0, 5000.0)).with_unit(" Hz").with_value_to_string(formatters::v2s_f32_rounded(3)).read_only(),
            m_note: FloatParam::new("Note", 0.0, lin(0.0, 127.0)).with_value_to_string(formatters::v2s_f32_rounded(0)).read_only(),
            m_cents: FloatParam::new("Cents", 0.0, lin(-100.0, 100.0)).with_unit(" ct").with_value_to_string(formatters::v2s_f32_rounded(2)).read_only(),
            m_confidence: FloatParam::new("Confidence", 0.0, lin(0.0, 1.0)).with_value_to_string(formatters::v2s_f32_rounded(2)).read_only(),
            m_string: FloatParam::new("String", 0.0, lin(0.0, 8.0)).with_value_to_string(formatters::v2s_f32_rounded(0)).read_only(),
            m_state: FloatParam::new("State", 0.0, lin(0.0, 3.0)).with_value_to_string(formatters::v2s_f32_rounded(0)).read_only(),
            m_seq: FloatParam::new("Update", 0.0, lin(0.0, 1023.0)).with_value_to_string(formatters::v2s_f32_rounded(0)).read_only(),
            m_s1_cents: FloatParam::new("String 1 Cents", 0.0, lin(-100.0, 100.0)).with_unit(" ct").with_value_to_string(formatters::v2s_f32_rounded(2)).read_only(),
            m_s2_cents: FloatParam::new("String 2 Cents", 0.0, lin(-100.0, 100.0)).with_unit(" ct").with_value_to_string(formatters::v2s_f32_rounded(2)).read_only(),
            m_s3_cents: FloatParam::new("String 3 Cents", 0.0, lin(-100.0, 100.0)).with_unit(" ct").with_value_to_string(formatters::v2s_f32_rounded(2)).read_only(),
            m_s4_cents: FloatParam::new("String 4 Cents", 0.0, lin(-100.0, 100.0)).with_unit(" ct").with_value_to_string(formatters::v2s_f32_rounded(2)).read_only(),
            m_s5_cents: FloatParam::new("String 5 Cents", 0.0, lin(-100.0, 100.0)).with_unit(" ct").with_value_to_string(formatters::v2s_f32_rounded(2)).read_only(),
            m_s6_cents: FloatParam::new("String 6 Cents", 0.0, lin(-100.0, 100.0)).with_unit(" ct").with_value_to_string(formatters::v2s_f32_rounded(2)).read_only(),
            m_s7_cents: FloatParam::new("String 7 Cents", 0.0, lin(-100.0, 100.0)).with_unit(" ct").with_value_to_string(formatters::v2s_f32_rounded(2)).read_only(),
            m_s8_cents: FloatParam::new("String 8 Cents", 0.0, lin(-100.0, 100.0)).with_unit(" ct").with_value_to_string(formatters::v2s_f32_rounded(2)).read_only(),
            m_s1_conf: FloatParam::new("String 1 Confidence", 0.0, lin(0.0, 1.0)).with_value_to_string(formatters::v2s_f32_rounded(2)).read_only(),
            m_s2_conf: FloatParam::new("String 2 Confidence", 0.0, lin(0.0, 1.0)).with_value_to_string(formatters::v2s_f32_rounded(2)).read_only(),
            m_s3_conf: FloatParam::new("String 3 Confidence", 0.0, lin(0.0, 1.0)).with_value_to_string(formatters::v2s_f32_rounded(2)).read_only(),
            m_s4_conf: FloatParam::new("String 4 Confidence", 0.0, lin(0.0, 1.0)).with_value_to_string(formatters::v2s_f32_rounded(2)).read_only(),
            m_s5_conf: FloatParam::new("String 5 Confidence", 0.0, lin(0.0, 1.0)).with_value_to_string(formatters::v2s_f32_rounded(2)).read_only(),
            m_s6_conf: FloatParam::new("String 6 Confidence", 0.0, lin(0.0, 1.0)).with_value_to_string(formatters::v2s_f32_rounded(2)).read_only(),
            m_s7_conf: FloatParam::new("String 7 Confidence", 0.0, lin(0.0, 1.0)).with_value_to_string(formatters::v2s_f32_rounded(2)).read_only(),
            m_s8_conf: FloatParam::new("String 8 Confidence", 0.0, lin(0.0, 1.0)).with_value_to_string(formatters::v2s_f32_rounded(2)).read_only(),
        }
    }
}

impl TunerParams {
    fn values(&self) -> [f32; NP] {
        [
            self.tuning.value() as usize as f32,
            self.mode.value() as usize as f32,
            self.precision.value() as usize as f32,
            self.reference.value(),
            b(self.mute.value()),
            b(self.bypass.value()),
            self.transpose.value() as f32,
            self.offset_1.value(),
            self.offset_2.value(),
            self.offset_3.value(),
            self.offset_4.value(),
            self.offset_5.value(),
            self.offset_6.value(),
            self.offset_7.value(),
            self.offset_8.value(),
            self.string_1.value() as f32,
            self.string_2.value() as f32,
            self.string_3.value() as f32,
            self.string_4.value() as f32,
            self.string_5.value() as f32,
            self.string_6.value() as f32,
            self.string_7.value() as f32,
            self.string_8.value() as f32,
            self.threshold.value(),
            b(self.always_on.value()),
        ]
    }

    /// The meters in [`crate::METERS`] order.
    fn meters(&self) -> [&FloatParam; 23] {
        [&self.m_freq, &self.m_note, &self.m_cents, &self.m_confidence, &self.m_string, &self.m_state, &self.m_seq, &self.m_s1_cents, &self.m_s2_cents, &self.m_s3_cents, &self.m_s4_cents, &self.m_s5_cents, &self.m_s6_cents, &self.m_s7_cents, &self.m_s8_cents, &self.m_s1_conf, &self.m_s2_conf, &self.m_s3_conf, &self.m_s4_conf, &self.m_s5_conf, &self.m_s6_conf, &self.m_s7_conf, &self.m_s8_conf]
    }
}

pub struct GfxTuner {
    params: Arc<TunerParams>,
    dsp: Tuner,
    last: [f32; NP],
}

impl Default for GfxTuner {
    fn default() -> Self {
        Self { params: Arc::new(TunerParams::default()), dsp: Tuner::new(), last: [f32::NAN; NP] }
    }
}

impl GfxTuner {
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

impl Plugin for GfxTuner {
    const NAME: &'static str = "GoleM FX Tuner";
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
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-tuner"))
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
        // one value per block, which is ample (README §2.6); `seq` guards golem's polls
        let m = *self.dsp.meters();
        for (p, v) in self.params.meters().into_iter().zip(m) {
            p.set_output_value(v);
        }
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxTuner {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxTuner0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Analyzer];
}

nih_export_vst3!(GfxTuner);
