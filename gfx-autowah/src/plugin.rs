// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2. Linear ranges only (golem contract).
//! The host transport (tempo, position) is passed on for the synced LFO.

use crate::{Autowah, FilterType, Settings, PARAMS};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum TypeParam {
    #[id = "lowpass"]
    #[name = "Low-pass"]
    LowPass,
    #[id = "bandpass"]
    #[name = "Band-pass"]
    BandPass,
    #[id = "highpass"]
    #[name = "High-pass"]
    HighPass,
    #[id = "wah"]
    Wah,
    #[id = "bandpass_dry"]
    #[name = "Band-pass + Dry"]
    BandPassDry,
    #[id = "vowel"]
    Vowel,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum Poles {
    #[id = "2"]
    #[name = "2"]
    Two,
    #[id = "4"]
    #[name = "4"]
    Four,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum Direction {
    #[id = "up"]
    Up,
    #[id = "down"]
    Down,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum Response {
    #[id = "fast"]
    Fast,
    #[id = "smooth"]
    Smooth,
    #[id = "opto"]
    Opto,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum Source {
    #[id = "envelope"]
    Envelope,
    #[id = "lfo"]
    #[name = "LFO"]
    Lfo,
    #[id = "sh"]
    #[name = "S&H"]
    SampleHold,
    #[id = "envelope_lfo"]
    #[name = "Envelope + LFO"]
    EnvelopeLfo,
}

/// The contract's shared sync list.
#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SyncParam {
    #[id = "off"]
    Off,
    #[id = "4/1"]
    #[name = "4/1"]
    W4,
    #[id = "2/1"]
    #[name = "2/1"]
    W2,
    #[id = "1/1"]
    #[name = "1/1"]
    W1,
    #[id = "1/2"]
    #[name = "1/2"]
    D2,
    #[id = "1/2d"]
    #[name = "1/2•"]
    D2Dot,
    #[id = "1/2t"]
    #[name = "1/2T"]
    D2T,
    #[id = "1/4"]
    #[name = "1/4"]
    D4,
    #[id = "1/4d"]
    #[name = "1/4•"]
    D4Dot,
    #[id = "1/4t"]
    #[name = "1/4T"]
    D4T,
    #[id = "1/8"]
    #[name = "1/8"]
    D8,
    #[id = "1/8d"]
    #[name = "1/8•"]
    D8Dot,
    #[id = "1/8t"]
    #[name = "1/8T"]
    D8T,
    #[id = "1/16"]
    #[name = "1/16"]
    D16,
    #[id = "1/16d"]
    #[name = "1/16•"]
    D16Dot,
    #[id = "1/16t"]
    #[name = "1/16T"]
    D16T,
    #[id = "1/32"]
    #[name = "1/32"]
    D32,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `wah` is golem's catalogue id (`autowah` block): the sensitivity.
#[derive(Params)]
struct AutowahParams {
    #[id = "wah"]
    wah: FloatParam,
    #[id = "type"]
    ftype: EnumParam<TypeParam>,
    #[id = "poles"]
    poles: EnumParam<Poles>,
    #[id = "direction"]
    direction: EnumParam<Direction>,
    #[id = "low"]
    low: FloatParam,
    #[id = "high"]
    high: FloatParam,
    #[id = "peak"]
    peak: FloatParam,
    #[id = "attack"]
    attack: FloatParam,
    #[id = "release"]
    release: FloatParam,
    #[id = "response"]
    response: EnumParam<Response>,
    #[id = "drive"]
    drive: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "level"]
    level: FloatParam,
    #[id = "bass_keep"]
    bass_keep: FloatParam,
    #[id = "source"]
    source: EnumParam<Source>,
    #[id = "rate"]
    rate: FloatParam,
    #[id = "sync"]
    sync: EnumParam<SyncParam>,
}

impl Default for AutowahParams {
    fn default() -> Self {
        Self {
            wah: f(0, "", 2),
            ftype: EnumParam::new("Type", TypeParam::BandPass),
            poles: EnumParam::new("Poles", Poles::Two),
            direction: EnumParam::new("Direction", Direction::Up),
            low: f(4, " Hz", 0),
            high: f(5, " Hz", 0),
            peak: f(6, "", 1),
            attack: f(7, " ms", 1),
            release: f(8, " ms", 0),
            response: EnumParam::new("Response", Response::Fast),
            drive: f(10, " dB", 1),
            mix: f(11, " %", 0),
            level: f(12, " dB", 1),
            bass_keep: f(13, " Hz", 0),
            source: EnumParam::new("Source", Source::Envelope),
            rate: f(15, " Hz", 2),
            sync: EnumParam::new("Sync", SyncParam::Off),
        }
    }
}

impl AutowahParams {
    fn settings(&self) -> Settings {
        Settings {
            wah: self.wah.value(),
            ftype: FilterType::ALL[self.ftype.value() as usize],
            four_poles: self.poles.value() == Poles::Four,
            down: self.direction.value() == Direction::Down,
            low: self.low.value(),
            high: self.high.value(),
            peak: self.peak.value(),
            attack: self.attack.value(),
            release: self.release.value(),
            response: self.response.value() as u32,
            drive: self.drive.value(),
            mix: self.mix.value(),
            level: self.level.value(),
            bass_keep: self.bass_keep.value(),
            source: self.source.value() as u32,
            rate: self.rate.value(),
            sync: self.sync.value() as u32,
        }
    }
}

pub struct GfxAutowah {
    params: Arc<AutowahParams>,
    dsp: Autowah,
}

impl Default for GfxAutowah {
    fn default() -> Self {
        Self { params: Arc::new(AutowahParams::default()), dsp: Autowah::new() }
    }
}

impl Plugin for GfxAutowah {
    const NAME: &'static str = "GoleM FX Autowah";
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

    fn initialize(&mut self, layout: &AudioIOLayout, config: &BufferConfig, _ctx: &mut impl InitContext<Self>) -> bool {
        let channels = layout.main_output_channels.map_or(1, |c| c.get() as usize);
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
        self.dsp.set(self.params.settings());
        true
    }

    fn reset(&mut self) {
        self.dsp.set(self.params.settings());
        self.dsp.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let s = self.params.settings();
        if s != self.dsp.settings() {
            self.dsp.set(s);
        }
        let t = ctx.transport();
        self.dsp.set_transport(TransportInfo { tempo: t.tempo, pos_beats: t.pos_beats(), playing: t.playing });
        self.dsp.process(buffer.as_slice());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxAutowah {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxAutowah01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Filter];
}

nih_export_vst3!(GfxAutowah);
