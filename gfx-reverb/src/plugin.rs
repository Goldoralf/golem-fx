// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): stereo 2 / 2 first, mono 1 / 1. Linear ranges only (golem contract).
//! `bypass` is the VST3 bypass parameter: the tail rings out (trails). Generated from PARAMS.

use crate::{Reverb, Settings, PARAMS};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

gfx_dsp::sync_param_enum!();

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum AlgoP {
    #[id = "hall"]
    #[name = "Hall"]
    Hall,
    #[id = "plate"]
    #[name = "Plate"]
    Plate,
    #[id = "room"]
    #[name = "Room"]
    Room,
    #[id = "chamber"]
    #[name = "Chamber"]
    Chamber,
    #[id = "cathedral"]
    #[name = "Cathedral"]
    Cathedral,
    #[id = "ambience"]
    #[name = "Ambience"]
    Ambience,
    #[id = "spring"]
    #[name = "Spring"]
    Spring,
    #[id = "bloom"]
    #[name = "Bloom"]
    Bloom,
    #[id = "cloud"]
    #[name = "Cloud"]
    Cloud,
    #[id = "gated"]
    #[name = "Gated"]
    Gated,
    #[id = "reverse"]
    #[name = "Reverse"]
    Reverse,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum EraP {
    #[id = "modern"]
    #[name = "Modern"]
    Modern,
    #[id = "80s"]
    #[name = "80s"]
    E80s,
    #[id = "70s"]
    #[name = "70s"]
    E70s,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SpringsP {
    #[id = "1"]
    #[name = "1"]
    S1,
    #[id = "2"]
    #[name = "2"]
    S2,
    #[id = "3"]
    #[name = "3"]
    S3,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum FreezeP {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "freeze"]
    #[name = "Freeze"]
    Freeze,
    #[id = "infinite"]
    #[name = "Infinite"]
    Infinite,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `mix`, `decay`, `size`, `predelay`, `hicut` are golem's catalogue ids (`reverb`, `reverb-plate`,
/// `reverb-room`); golem pins `algo` per block.
#[derive(Params)]
struct ReverbParams {
    #[id = "mix"]
    mix: FloatParam,
    #[id = "decay"]
    decay: FloatParam,
    #[id = "size"]
    size: FloatParam,
    #[id = "predelay"]
    predelay: FloatParam,
    #[id = "hicut"]
    hicut: FloatParam,
    #[id = "algo"]
    algo: EnumParam<AlgoP>,
    #[id = "lowcut"]
    lowcut: FloatParam,
    #[id = "bass_mult"]
    bass_mult: FloatParam,
    #[id = "bass_xover"]
    bass_xover: FloatParam,
    #[id = "mod"]
    modulation: FloatParam,
    #[id = "diffusion"]
    diffusion: FloatParam,
    #[id = "early"]
    early: FloatParam,
    #[id = "width"]
    width: FloatParam,
    #[id = "era"]
    era: EnumParam<EraP>,
    #[id = "mod_rate"]
    mod_rate: FloatParam,
    #[id = "high_mult"]
    high_mult: FloatParam,
    #[id = "wet_low"]
    wet_low: FloatParam,
    #[id = "wet_high"]
    wet_high: FloatParam,
    #[id = "swell"]
    swell: FloatParam,
    #[id = "drive"]
    drive: FloatParam,
    #[id = "springs"]
    springs: EnumParam<SpringsP>,
    #[id = "sync"]
    sync: EnumParam<SyncParam>,
    #[id = "shimmer"]
    shimmer: FloatParam,
    #[id = "shimmer_pitch"]
    shimmer_pitch: FloatParam,
    #[id = "duck"]
    duck: FloatParam,
    #[id = "duck_sens"]
    duck_sens: FloatParam,
    #[id = "duck_release"]
    duck_release: FloatParam,
    #[id = "freeze"]
    freeze: EnumParam<FreezeP>,
    #[id = "kill_dry"]
    kill_dry: BoolParam,
    #[id = "bypass"]
    bypass: BoolParam,
    #[id = "trails"]
    trails: BoolParam,
}

impl Default for ReverbParams {
    fn default() -> Self {
        Self {
            mix: f(0, " %", 0),
            decay: f(1, " s", 2),
            size: f(2, " m", 0),
            predelay: f(3, " ms", 0),
            hicut: f(4, " Hz", 0),
            algo: EnumParam::new("Algorithm", AlgoP::Hall),
            lowcut: f(6, " Hz", 0),
            bass_mult: f(7, "×", 2),
            bass_xover: f(8, " Hz", 0),
            modulation: f(9, " %", 0),
            diffusion: f(10, " %", 0),
            early: f(11, " %", 0),
            width: f(12, " %", 0),
            era: EnumParam::new("Era", EraP::Modern),
            mod_rate: f(14, " %", 0),
            high_mult: f(15, "×", 2),
            wet_low: f(16, " %", 0),
            wet_high: f(17, " %", 0),
            swell: f(18, " ms", 0),
            drive: f(19, " %", 0),
            springs: EnumParam::new("Springs", SpringsP::S2),
            sync: EnumParam::new("Sync", SyncParam::Off),
            shimmer: f(22, " %", 0),
            shimmer_pitch: f(23, " st", 1),
            duck: f(24, " dB", 1),
            duck_sens: f(25, " %", 0),
            duck_release: f(26, " ms", 0),
            freeze: EnumParam::new("Freeze", FreezeP::Off),
            kill_dry: BoolParam::new("Kill Dry", false),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            trails: BoolParam::new("Trails", true),
        }
    }
}

impl ReverbParams {
    fn settings(&self) -> Settings {
        Settings {
            v: [
                self.mix.value(),
                self.decay.value(),
                self.size.value(),
                self.predelay.value(),
                self.hicut.value(),
                self.algo.value() as usize as f32,
                self.lowcut.value(),
                self.bass_mult.value(),
                self.bass_xover.value(),
                self.modulation.value(),
                self.diffusion.value(),
                self.early.value(),
                self.width.value(),
                self.era.value() as usize as f32,
                self.mod_rate.value(),
                self.high_mult.value(),
                self.wet_low.value(),
                self.wet_high.value(),
                self.swell.value(),
                self.drive.value(),
                self.springs.value() as usize as f32,
                self.sync.value() as usize as f32,
                self.shimmer.value(),
                self.shimmer_pitch.value(),
                self.duck.value(),
                self.duck_sens.value(),
                self.duck_release.value(),
                self.freeze.value() as usize as f32,
                if self.kill_dry.value() { 1.0 } else { 0.0 },
                if self.bypass.value() { 1.0 } else { 0.0 },
                if self.trails.value() { 1.0 } else { 0.0 },
            ],
        }
    }
}

pub struct GfxReverb {
    params: Arc<ReverbParams>,
    dsp: Reverb,
}

impl Default for GfxReverb {
    fn default() -> Self {
        Self { params: Arc::new(ReverbParams::default()), dsp: Reverb::new() }
    }
}

impl Plugin for GfxReverb {
    const NAME: &'static str = "GoleM FX Reverb";
    const VENDOR: &'static str = "GoleM";
    const URL: &'static str = "https://golem.fm";
    const EMAIL: &'static str = "contact@golem.fm";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        // stereo by nature: 2 / 2 first, so golem turns the lane stereo here (a mono DI arrives on both inputs)
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
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

impl Vst3Plugin for GfxReverb {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxReverb001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Reverb];
}

nih_export_vst3!(GfxReverb);
