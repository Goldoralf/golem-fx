// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first (a vocoder is not stereo by nature, and a voice lane
//! stays mono), stereo 2 / 2 second, each with a key on its own auxiliary bus (1 or 2 channels; v1's
//! carriers do not read it: Key Input and Talk Box are appended to `source` when golem routes another
//! lane, README §6). No MIDI input in v1. Zero latency. Linear ranges only (golem contract). `bypass`
//! is the VST3 bypass parameter. `f0` and `state` are read-only meters (golem's fork).

use crate::{Vocoder, NP, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SourceP {
    #[id = "tracked"]
    #[name = "Tracked"]
    Tracked,
    #[id = "drone"]
    #[name = "Drone"]
    Drone,
    #[id = "noise"]
    #[name = "Noise"]
    Noise,
    #[id = "self"]
    #[name = "Self"]
    SelfInput,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ChordP {
    #[id = "unison"]
    #[name = "Unison"]
    Unison,
    #[id = "octave"]
    #[name = "Octave"]
    Octave,
    #[id = "fifth"]
    #[name = "Fifth"]
    Fifth,
    #[id = "triad"]
    #[name = "Triad"]
    Triad,
    #[id = "seventh"]
    #[name = "Seventh"]
    Seventh,
    #[id = "ninth"]
    #[name = "Ninth"]
    Ninth,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SpacingP {
    #[id = "erb"]
    #[name = "ERB"]
    Erb,
    #[id = "log"]
    #[name = "Log"]
    Log,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum WaveP {
    #[id = "saw"]
    #[name = "Saw"]
    Saw,
    #[id = "pulse"]
    #[name = "Pulse"]
    Pulse,
    #[id = "buzz"]
    #[name = "Buzz"]
    Buzz,
    #[id = "supersaw"]
    #[name = "Supersaw"]
    Supersaw,
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
    #[id = "instrument"]
    #[name = "Instrument"]
    Instrument,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum QuantizeP {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "chromatic"]
    #[name = "Chromatic"]
    Chromatic,
    #[id = "key"]
    #[name = "Key"]
    Key,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum KeyP {
    #[id = "c"]
    #[name = "C"]
    C,
    #[id = "cs"]
    #[name = "C#"]
    Cs,
    #[id = "d"]
    #[name = "D"]
    D,
    #[id = "eb"]
    #[name = "Eb"]
    Eb,
    #[id = "e"]
    #[name = "E"]
    E,
    #[id = "f"]
    #[name = "F"]
    F,
    #[id = "fs"]
    #[name = "F#"]
    Fs,
    #[id = "g"]
    #[name = "G"]
    G,
    #[id = "ab"]
    #[name = "Ab"]
    Ab,
    #[id = "a"]
    #[name = "A"]
    A,
    #[id = "bb"]
    #[name = "Bb"]
    Bb,
    #[id = "b"]
    #[name = "B"]
    B,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ScaleP {
    #[id = "major"]
    #[name = "Major"]
    Major,
    #[id = "minor"]
    #[name = "Minor"]
    Minor,
    #[id = "harmonic-minor"]
    #[name = "Harmonic Minor"]
    HarmonicMinor,
    #[id = "melodic-minor"]
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
    #[id = "major-pentatonic"]
    #[name = "Major Pentatonic"]
    MajorPentatonic,
    #[id = "minor-pentatonic"]
    #[name = "Minor Pentatonic"]
    MinorPentatonic,
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

fn b(v: bool) -> f32 {
    if v {
        1.0
    } else {
        0.0
    }
}

#[derive(Params)]
struct VocoderParams {
    #[id = "source"]
    source: EnumParam<SourceP>,
    #[id = "chord"]
    chord: EnumParam<ChordP>,
    #[id = "formant"]
    formant: FloatParam,
    #[id = "env_rel"]
    env_rel: FloatParam,
    #[id = "sib"]
    sib: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "freeze"]
    freeze: BoolParam,
    #[id = "bands"]
    bands: IntParam,
    #[id = "spacing"]
    spacing: EnumParam<SpacingP>,
    #[id = "low"]
    low: FloatParam,
    #[id = "high"]
    high: FloatParam,
    #[id = "width"]
    width: FloatParam,
    #[id = "env_att"]
    env_att: FloatParam,
    #[id = "thresh"]
    thresh: FloatParam,
    #[id = "flatten"]
    flatten: FloatParam,
    #[id = "unvoiced"]
    unvoiced: FloatParam,
    #[id = "noise"]
    noise: FloatParam,
    #[id = "tilt"]
    tilt: FloatParam,
    #[id = "wave"]
    wave: EnumParam<WaveP>,
    #[id = "detune"]
    detune: FloatParam,
    #[id = "register"]
    register: EnumParam<RegisterP>,
    #[id = "quantize"]
    quantize: EnumParam<QuantizeP>,
    #[id = "glide"]
    glide: FloatParam,
    #[id = "octave"]
    octave: IntParam,
    #[id = "degree"]
    degree: IntParam,
    #[id = "key"]
    key: EnumParam<KeyP>,
    #[id = "scale"]
    scale: EnumParam<ScaleP>,
    #[id = "spread"]
    spread: FloatParam,
    #[id = "output"]
    output: FloatParam,
    #[id = "bypass"]
    bypass: BoolParam,
    /// Read-only meters (golem contract): the tracked fundamental (Hz, 0 = none) and the gate (1 = open).
    #[id = "f0"]
    f0: FloatParam,
    #[id = "state"]
    state: FloatParam,
}

impl Default for VocoderParams {
    fn default() -> Self {
        Self {
            source: EnumParam::new("Carrier", SourceP::Tracked),
            chord: EnumParam::new("Chord", ChordP::Unison),
            formant: f(2, " st", 1),
            env_rel: f(3, " ms", 0),
            sib: f(4, " %", 0),
            mix: f(5, " %", 0),
            freeze: BoolParam::new("Freeze", false),
            bands: int(7),
            spacing: EnumParam::new("Spacing", SpacingP::Erb),
            low: f(9, " Hz", 0),
            high: f(10, " Hz", 0),
            width: f(11, " %", 0),
            env_att: f(12, " ms", 1),
            thresh: f(13, " dBFS", 1),
            flatten: f(14, " %", 0),
            unvoiced: f(15, " %", 0),
            noise: f(16, " %", 0),
            tilt: f(17, " dB/oct", 1),
            wave: EnumParam::new("Wave", WaveP::Saw),
            detune: f(19, " ct", 1),
            register: EnumParam::new("Register", RegisterP::Mid),
            quantize: EnumParam::new("Quantize", QuantizeP::Off),
            glide: f(22, " ms", 0),
            octave: int(23),
            degree: int(24),
            key: EnumParam::new("Key", KeyP::C),
            scale: EnumParam::new("Scale", ScaleP::Major),
            spread: f(27, " %", 0),
            output: f(28, " dB", 1),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            f0: FloatParam::new("Pitch", 0.0, FloatRange::Linear { min: 0.0, max: 1500.0 })
                .with_unit(" Hz")
                .with_value_to_string(formatters::v2s_f32_rounded(1))
                .read_only(),
            state: FloatParam::new("Gate Open", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 }).read_only(),
        }
    }
}

impl VocoderParams {
    fn values(&self) -> [f32; NP] {
        [
            self.source.value() as usize as f32,
            self.chord.value() as usize as f32,
            self.formant.value(),
            self.env_rel.value(),
            self.sib.value(),
            self.mix.value(),
            b(self.freeze.value()),
            self.bands.value() as f32,
            self.spacing.value() as usize as f32,
            self.low.value(),
            self.high.value(),
            self.width.value(),
            self.env_att.value(),
            self.thresh.value(),
            self.flatten.value(),
            self.unvoiced.value(),
            self.noise.value(),
            self.tilt.value(),
            self.wave.value() as usize as f32,
            self.detune.value(),
            self.register.value() as usize as f32,
            self.quantize.value() as usize as f32,
            self.glide.value(),
            self.octave.value() as f32,
            self.degree.value() as f32,
            self.key.value() as usize as f32,
            self.scale.value() as usize as f32,
            self.spread.value(),
            self.output.value(),
            b(self.bypass.value()),
        ]
    }
}

pub struct GfxVocoder {
    params: Arc<VocoderParams>,
    dsp: Vocoder,
    last: [f32; NP],
}

impl Default for GfxVocoder {
    fn default() -> Self {
        Self { params: Arc::new(VocoderParams::default()), dsp: Vocoder::new(), last: [f32::NAN; NP] }
    }
}

impl GfxVocoder {
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

impl Plugin for GfxVocoder {
    const NAME: &'static str = "GoleM FX Vocoder";
    const VENDOR: &'static str = "GoleM";
    const URL: &'static str = "https://golem.fm";
    const EMAIL: &'static str = "contact@golem.fm";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            aux_input_ports: &[new_nonzero_u32(1)],
            names: PortNames { aux_inputs: &["Key"], ..PortNames::const_default() },
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[new_nonzero_u32(2)],
            names: PortNames { aux_inputs: &["Key"], ..PortNames::const_default() },
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
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-vocoder"))
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
        self.params.f0.set_output_value(self.dsp.f0());
        self.params.state.set_output_value(if self.dsp.gate_is_open() { 1.0 } else { 0.0 });
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxVocoder {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxVocoder01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Modulation];
}

nih_export_vst3!(GfxVocoder);
