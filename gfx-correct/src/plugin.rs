// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second (analysis on (L + R) / 2, both
//! channels shifted by the same decisions). Linear ranges only (golem contract). Generated from PARAMS
//! (same order and ids). The latency is reported: Live ⌈T_max / 2 + 1 ms⌉, Studio ⌈2 · T_max + 4 ms⌉,
//! changed only by `fastmode` and `register` (setup). `bypass` is the VST3 bypass parameter (the input
//! delayed by the latency, 10 ms crossfade). `state`, `note`, `error` are read-only meters.

use crate::{Correct, NP, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum KeyP {
    #[id = "c"]
    #[name = "C"]
    C,
    #[id = "c#"]
    #[name = "C#"]
    Cs,
    #[id = "d"]
    #[name = "D"]
    D,
    #[id = "d#"]
    #[name = "D#"]
    Ds,
    #[id = "e"]
    #[name = "E"]
    E,
    #[id = "f"]
    #[name = "F"]
    F,
    #[id = "f#"]
    #[name = "F#"]
    Fs,
    #[id = "g"]
    #[name = "G"]
    G,
    #[id = "g#"]
    #[name = "G#"]
    Gs,
    #[id = "a"]
    #[name = "A"]
    A,
    #[id = "a#"]
    #[name = "A#"]
    As,
    #[id = "b"]
    #[name = "B"]
    B,
}

/// golem's catalogue order (Chromatic · Major · Minor), then gfx-pitch's scales: append only.
#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ScaleP {
    #[id = "chromatic"]
    #[name = "Chromatic"]
    Chromatic,
    #[id = "major"]
    #[name = "Major"]
    Major,
    #[id = "minor"]
    #[name = "Minor"]
    Minor,
    #[id = "harmonic_minor"]
    #[name = "Harmonic Minor"]
    HarmonicMinor,
    #[id = "melodic_minor"]
    #[name = "Melodic Minor"]
    MelodicMinor,
    #[id = "dorian"]
    #[name = "Dorian"]
    Dorian,
    #[id = "phrygian"]
    #[name = "Phrygian"]
    Phrygian,
    #[id = "lydian"]
    #[name = "Lydian"]
    Lydian,
    #[id = "mixolydian"]
    #[name = "Mixolydian"]
    Mixolydian,
    #[id = "major_pentatonic"]
    #[name = "Major Pentatonic"]
    MajorPentatonic,
    #[id = "minor_pentatonic"]
    #[name = "Minor Pentatonic"]
    MinorPentatonic,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum RegisterP {
    #[id = "low"]
    #[name = "Low"]
    Low,
    #[id = "mid"]
    #[name = "Mid"]
    Mid,
    #[id = "high"]
    #[name = "High"]
    High,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

fn b(i: usize) -> BoolParam {
    BoolParam::new(PARAMS[i].name, PARAMS[i].default >= 0.5)
}

#[derive(Params)]
struct CorrectParams {
    #[id = "corr"]
    corr: FloatParam,
    #[id = "filter"]
    filter: FloatParam,
    #[id = "transition"]
    transition: FloatParam,
    #[id = "vibrato"]
    vibrato: FloatParam,
    #[id = "key"]
    key: EnumParam<KeyP>,
    #[id = "scale"]
    scale: EnumParam<ScaleP>,
    #[id = "fastmode"]
    fastmode: BoolParam,
    #[id = "offset"]
    offset: FloatParam,
    #[id = "flex"]
    flex: FloatParam,
    #[id = "hysteresis"]
    hysteresis: FloatParam,
    #[id = "formant_keep"]
    formant_keep: BoolParam,
    #[id = "register"]
    register: EnumParam<RegisterP>,
    #[id = "reference"]
    reference: FloatParam,
    #[id = "sens"]
    sens: FloatParam,
    #[id = "thresh"]
    thresh: FloatParam,
    #[id = "m00"]
    m00: BoolParam,
    #[id = "m01"]
    m01: BoolParam,
    #[id = "m02"]
    m02: BoolParam,
    #[id = "m03"]
    m03: BoolParam,
    #[id = "m04"]
    m04: BoolParam,
    #[id = "m05"]
    m05: BoolParam,
    #[id = "m06"]
    m06: BoolParam,
    #[id = "m07"]
    m07: BoolParam,
    #[id = "m08"]
    m08: BoolParam,
    #[id = "m09"]
    m09: BoolParam,
    #[id = "m10"]
    m10: BoolParam,
    #[id = "m11"]
    m11: BoolParam,
    #[id = "bypass"]
    bypass: BoolParam,
    /// Read-only meters (golem contract): 0 unvoiced / idle · 1 correcting · 2 gated by level; the
    /// target's MIDI number (0 = none); steady pitch minus target in cents.
    #[id = "state"]
    state: FloatParam,
    #[id = "note"]
    note: FloatParam,
    #[id = "error"]
    error: FloatParam,
}

impl Default for CorrectParams {
    fn default() -> Self {
        Self {
            // the catalogue's units, as golem shows them: a fraction, seconds
            corr: f(0, "", 2),
            filter: f(1, " s", 3),
            transition: f(2, " ms", 0),
            vibrato: f(3, " %", 0),
            key: EnumParam::new("Key", KeyP::C),
            scale: EnumParam::new("Scale", ScaleP::Chromatic),
            fastmode: b(6),
            offset: f(7, " st", 2),
            flex: f(8, " %", 0),
            hysteresis: f(9, " ct", 0),
            formant_keep: b(10),
            register: EnumParam::new("Register", RegisterP::Mid),
            reference: f(12, " Hz", 1),
            sens: f(13, " %", 0),
            thresh: f(14, " dB", 1),
            m00: b(15),
            m01: b(16),
            m02: b(17),
            m03: b(18),
            m04: b(19),
            m05: b(20),
            m06: b(21),
            m07: b(22),
            m08: b(23),
            m09: b(24),
            m10: b(25),
            m11: b(26),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            state: FloatParam::new("Voice State", 0.0, FloatRange::Linear { min: 0.0, max: 2.0 }).read_only(),
            note: FloatParam::new("Target Note", 0.0, FloatRange::Linear { min: 0.0, max: 127.0 }).read_only(),
            error: FloatParam::new("Pitch Error", 0.0, FloatRange::Linear { min: -200.0, max: 200.0 })
                .with_unit(" ct")
                .with_value_to_string(formatters::v2s_f32_rounded(1))
                .read_only(),
        }
    }
}

impl CorrectParams {
    fn values(&self) -> [f32; NP] {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        [
            self.corr.value(),
            self.filter.value(),
            self.transition.value(),
            self.vibrato.value(),
            self.key.value() as usize as f32,
            self.scale.value() as usize as f32,
            b(self.fastmode.value()),
            self.offset.value(),
            self.flex.value(),
            self.hysteresis.value(),
            b(self.formant_keep.value()),
            self.register.value() as usize as f32,
            self.reference.value(),
            self.sens.value(),
            self.thresh.value(),
            b(self.m00.value()),
            b(self.m01.value()),
            b(self.m02.value()),
            b(self.m03.value()),
            b(self.m04.value()),
            b(self.m05.value()),
            b(self.m06.value()),
            b(self.m07.value()),
            b(self.m08.value()),
            b(self.m09.value()),
            b(self.m10.value()),
            b(self.m11.value()),
            b(self.bypass.value()),
        ]
    }
}

pub struct GfxCorrect {
    params: Arc<CorrectParams>,
    dsp: Correct,
    last: [f32; NP],
    reported: u32,
}

impl Default for GfxCorrect {
    fn default() -> Self {
        Self { params: Arc::new(CorrectParams::default()), dsp: Correct::new(), last: [f32::NAN; NP], reported: 0 }
    }
}

impl GfxCorrect {
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

impl Plugin for GfxCorrect {
    const NAME: &'static str = "GoleM FX Correct";
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

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.sync(false);
        self.dsp.process(buffer.as_slice());
        let l = self.dsp.latency() as u32;
        if l != self.reported {
            self.reported = l;
            ctx.set_latency_samples(l);
        }
        self.params.state.set_output_value(self.dsp.state() as f32);
        self.params.note.set_output_value(self.dsp.note());
        self.params.error.set_output_value(self.dsp.error());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxCorrect {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxCorrect01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::PitchShift];
}

nih_export_vst3!(GfxCorrect);
