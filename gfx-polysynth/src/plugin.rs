// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first (the pedal sits first in a mono lane), stereo 2 / 2
//! second (the mono sum is processed; `spread` places the unison pair). Linear ranges only (golem
//! contract). `bypass` is the VST3 bypass parameter: with `trails` the synth releases over `release`.
//! Zero latency. Read-only meters `state` and `note`. Generated from PARAMS (same order and ids).

use crate::{Polysynth, NP, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum EngineP {
    #[id = "spectral"]
    #[name = "Spectral"]
    Spectral,
    #[id = "mono"]
    #[name = "Mono"]
    Mono,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum WaveP {
    #[id = "saw"]
    #[name = "Saw"]
    Saw,
    #[id = "pulse"]
    #[name = "Pulse"]
    Pulse,
    #[id = "triangle"]
    #[name = "Triangle"]
    Triangle,
    #[id = "sine"]
    #[name = "Sine"]
    Sine,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum AmpModeP {
    #[id = "follow"]
    #[name = "Follow"]
    Follow,
    #[id = "gate"]
    #[name = "Gate"]
    Gate,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
#[allow(clippy::enum_variant_names)]
enum TypeP {
    #[id = "low-pass"]
    #[name = "Low-pass"]
    LowPass,
    #[id = "band-pass"]
    #[name = "Band-pass"]
    BandPass,
    #[id = "high-pass"]
    #[name = "High-pass"]
    HighPass,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum PolesP {
    #[id = "2"]
    #[name = "2"]
    Two,
    #[id = "4"]
    #[name = "4"]
    Four,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum BankP {
    #[id = "dual"]
    #[name = "Dual"]
    Dual,
    #[id = "fast"]
    #[name = "Fast"]
    Fast,
    #[id = "clean"]
    #[name = "Clean"]
    Clean,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum QuantizeP {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "chromatic"]
    #[name = "Chromatic"]
    Chromatic,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

#[derive(Params)]
struct PolysynthParams {
    #[id = "engine"]
    engine: EnumParam<EngineP>,
    #[id = "wave"]
    wave: EnumParam<WaveP>,
    #[id = "cutoff"]
    cutoff: FloatParam,
    #[id = "reso"]
    reso: FloatParam,
    #[id = "filt_env"]
    filt_env: FloatParam,
    #[id = "synth"]
    synth: FloatParam,
    #[id = "dry"]
    dry: FloatParam,
    #[id = "pw"]
    pw: FloatParam,
    #[id = "octave"]
    octave: IntParam,
    #[id = "sub"]
    sub: FloatParam,
    #[id = "detune"]
    detune: FloatParam,
    #[id = "noise"]
    noise: FloatParam,
    #[id = "amp_mode"]
    amp_mode: EnumParam<AmpModeP>,
    #[id = "attack"]
    attack: FloatParam,
    #[id = "decay"]
    decay: FloatParam,
    #[id = "sustain"]
    sustain: FloatParam,
    #[id = "release"]
    release: FloatParam,
    #[id = "compress"]
    compress: FloatParam,
    #[id = "type"]
    ftype: EnumParam<TypeP>,
    #[id = "poles"]
    poles: EnumParam<PolesP>,
    #[id = "drive"]
    drive: FloatParam,
    #[id = "key_track"]
    key_track: FloatParam,
    #[id = "fenv_att"]
    fenv_att: FloatParam,
    #[id = "fenv_dec"]
    fenv_dec: FloatParam,
    #[id = "fenv_vel"]
    fenv_vel: FloatParam,
    #[id = "bank"]
    bank: EnumParam<BankP>,
    #[id = "residue"]
    residue: FloatParam,
    #[id = "glide"]
    glide: FloatParam,
    #[id = "quantize"]
    quantize: EnumParam<QuantizeP>,
    #[id = "lowest"]
    lowest: FloatParam,
    #[id = "thresh"]
    thresh: FloatParam,
    #[id = "spread"]
    spread: FloatParam,
    #[id = "bypass"]
    bypass: BoolParam,
    #[id = "trails"]
    trails: BoolParam,
    /// Read-only meters (golem contract): 0 silence · 1 core only · 2 voice sounding; the voice's tracked
    /// f0 (Hz, 0 = none or Spectral).
    #[id = "state"]
    state: FloatParam,
    #[id = "note"]
    note: FloatParam,
}

impl Default for PolysynthParams {
    fn default() -> Self {
        Self {
            engine: EnumParam::new("Engine", EngineP::Spectral),
            wave: EnumParam::new("Wave", WaveP::Saw),
            cutoff: f(2, " Hz", 0),
            reso: f(3, " %", 0),
            filt_env: f(4, " st", 1),
            synth: f(5, " dB", 1),
            dry: f(6, " dB", 1),
            pw: f(7, " %", 0),
            octave: IntParam::new("Octave", 0, IntRange::Linear { min: -2, max: 2 }),
            sub: f(9, " %", 0),
            detune: f(10, " ct", 1),
            noise: f(11, " %", 0),
            amp_mode: EnumParam::new("Amp Mode", AmpModeP::Follow),
            attack: f(13, " ms", 0),
            decay: f(14, " ms", 0),
            sustain: f(15, " %", 0),
            release: f(16, " ms", 0),
            compress: f(17, " %", 0),
            ftype: EnumParam::new("Filter Type", TypeP::LowPass),
            poles: EnumParam::new("Poles", PolesP::Four),
            drive: f(20, " %", 0),
            key_track: f(21, " %", 0),
            fenv_att: f(22, " ms", 0),
            fenv_dec: f(23, " ms", 0),
            fenv_vel: f(24, " %", 0),
            bank: EnumParam::new("Resolution", BankP::Dual),
            residue: f(26, " %", 0),
            glide: f(27, " ms", 0),
            quantize: EnumParam::new("Pitch", QuantizeP::Off),
            lowest: f(29, " Hz", 0),
            thresh: f(30, " dB", 1),
            spread: f(31, " %", 0),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            trails: BoolParam::new("Trails", true),
            state: FloatParam::new("Synth State", 0.0, FloatRange::Linear { min: 0.0, max: 2.0 }).read_only(),
            note: FloatParam::new("Note", 0.0, FloatRange::Linear { min: 0.0, max: 2000.0 })
                .with_unit(" Hz")
                .with_value_to_string(formatters::v2s_f32_rounded(1))
                .read_only(),
        }
    }
}

impl PolysynthParams {
    fn values(&self) -> [f32; NP] {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        [
            self.engine.value() as usize as f32,
            self.wave.value() as usize as f32,
            self.cutoff.value(),
            self.reso.value(),
            self.filt_env.value(),
            self.synth.value(),
            self.dry.value(),
            self.pw.value(),
            self.octave.value() as f32,
            self.sub.value(),
            self.detune.value(),
            self.noise.value(),
            self.amp_mode.value() as usize as f32,
            self.attack.value(),
            self.decay.value(),
            self.sustain.value(),
            self.release.value(),
            self.compress.value(),
            self.ftype.value() as usize as f32,
            self.poles.value() as usize as f32,
            self.drive.value(),
            self.key_track.value(),
            self.fenv_att.value(),
            self.fenv_dec.value(),
            self.fenv_vel.value(),
            self.bank.value() as usize as f32,
            self.residue.value(),
            self.glide.value(),
            self.quantize.value() as usize as f32,
            self.lowest.value(),
            self.thresh.value(),
            self.spread.value(),
            b(self.bypass.value()),
            b(self.trails.value()),
        ]
    }
}

pub struct GfxPolysynth {
    params: Arc<PolysynthParams>,
    dsp: Polysynth,
    last: [f32; NP],
}

impl Default for GfxPolysynth {
    fn default() -> Self {
        Self { params: Arc::new(PolysynthParams::default()), dsp: Polysynth::new(), last: [f32::NAN; NP] }
    }
}

impl GfxPolysynth {
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

impl Plugin for GfxPolysynth {
    const NAME: &'static str = "GoleM FX Polysynth";
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
        self.params.state.set_output_value(self.dsp.state());
        self.params.note.set_output_value(self.dsp.note_hz());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxPolysynth {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxPolysyn01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Synth];
}

nih_export_vst3!(GfxPolysynth);
