// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): stereo 2 / 2 first, mono 1 / 1 (stereo by nature: golem turns the lane
//! stereo here, and a mono DI arrives on both inputs). Zero latency. Linear ranges only (golem
//! contract). `freeze` is the footswitch's parameter; `bypass` is the VST3 bypass parameter (with
//! `trails` the cloud finishes and a hold releases). `active` and `state` are read-only meters
//! (golem's fork). Generated from PARAMS.

use crate::{Grains, NP, PARAMS};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

gfx_dsp::sync_param_enum!();

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModeP {
    #[id = "cloud"]
    #[name = "Cloud"]
    Cloud,
    #[id = "rhythm"]
    #[name = "Rhythm"]
    Rhythm,
    #[id = "strum"]
    #[name = "Strum"]
    Strum,
    #[id = "phrase"]
    #[name = "Phrase"]
    Phrase,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ScaleP {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "chromatic"]
    #[name = "Chromatic"]
    Chromatic,
    #[id = "octaves"]
    #[name = "Octaves"]
    Octaves,
    #[id = "fifths"]
    #[name = "Fifths"]
    Fifths,
    #[id = "major-triad"]
    #[name = "Major Triad"]
    MajorTriad,
    #[id = "minor-triad"]
    #[name = "Minor Triad"]
    MinorTriad,
    #[id = "major"]
    #[name = "Major"]
    Major,
    #[id = "minor"]
    #[name = "Minor"]
    Minor,
    #[id = "major-pent"]
    #[name = "Major Pent"]
    MajorPent,
    #[id = "minor-pent"]
    #[name = "Minor Pent"]
    MinorPent,
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
struct GrainsParams {
    #[id = "freeze"]
    freeze: BoolParam,
    #[id = "grains"]
    grains: IntParam,
    #[id = "pos"]
    pos: FloatParam,
    #[id = "size"]
    size: FloatParam,
    #[id = "pitch"]
    pitch: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "mode"]
    mode: EnumParam<ModeP>,
    #[id = "length"]
    length: FloatParam,
    #[id = "length_sync"]
    length_sync: EnumParam<SyncParam>,
    #[id = "spray"]
    spray: FloatParam,
    #[id = "scan"]
    scan: FloatParam,
    #[id = "regular"]
    regular: FloatParam,
    #[id = "chance"]
    chance: FloatParam,
    #[id = "shape"]
    shape: FloatParam,
    #[id = "skew"]
    skew: FloatParam,
    #[id = "reverse"]
    reverse: FloatParam,
    #[id = "pitch_rand"]
    pitch_rand: FloatParam,
    #[id = "scale"]
    scale: EnumParam<ScaleP>,
    #[id = "detune"]
    detune: FloatParam,
    #[id = "glide"]
    glide: FloatParam,
    #[id = "spread"]
    spread: FloatParam,
    #[id = "sync"]
    sync: EnumParam<SyncParam>,
    #[id = "seed"]
    seed: IntParam,
    #[id = "skip"]
    skip: FloatParam,
    #[id = "live"]
    live: FloatParam,
    #[id = "decay"]
    decay: FloatParam,
    #[id = "feedback"]
    feedback: FloatParam,
    #[id = "low_cut"]
    low_cut: FloatParam,
    #[id = "high_cut"]
    high_cut: FloatParam,
    #[id = "kill_dry"]
    kill_dry: BoolParam,
    #[id = "trails"]
    trails: BoolParam,
    #[id = "bypass"]
    bypass: BoolParam,
    /// Read-only meters (golem contract): voices sounding (128 = full: new grains are dropped), and
    /// 0 Live · 1 Held · 2 Releasing (golem's footswitch LED).
    #[id = "active"]
    active: FloatParam,
    #[id = "state"]
    state: FloatParam,
}

impl Default for GrainsParams {
    fn default() -> Self {
        Self {
            freeze: BoolParam::new("Freeze", false),
            grains: int(1),
            // "% · ms" is golem's display; the plugin shows the fraction
            pos: f(2, "", 3),
            size: f(3, " ms", 1),
            pitch: f(4, " st", 2),
            mix: f(5, " %", 0),
            mode: EnumParam::new("Mode", ModeP::Cloud),
            length: f(7, " ms", 0),
            length_sync: EnumParam::new("Length Sync", SyncParam::Off),
            spray: f(9, " %", 1),
            scan: f(10, " %", 0),
            regular: f(11, " %", 0),
            chance: f(12, " %", 0),
            shape: f(13, " %", 0),
            skew: f(14, " %", 0),
            reverse: f(15, " %", 0),
            pitch_rand: f(16, " st", 1),
            scale: EnumParam::new("Scale", ScaleP::Off),
            detune: f(18, " ct", 1),
            glide: f(19, " st", 1),
            spread: f(20, " %", 0),
            sync: EnumParam::new("Sync", SyncParam::Off),
            seed: int(22),
            skip: f(23, " %", 0),
            live: f(24, " %", 0),
            decay: f(25, " s", 2),
            feedback: f(26, " %", 0),
            low_cut: f(27, " Hz", 0),
            high_cut: f(28, " Hz", 0),
            kill_dry: BoolParam::new("Kill Dry", false),
            trails: BoolParam::new("Trails", true),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            active: FloatParam::new("Grains (now)", 0.0, FloatRange::Linear { min: 0.0, max: 128.0 }).read_only(),
            state: FloatParam::new("State", 0.0, FloatRange::Linear { min: 0.0, max: 2.0 }).read_only(),
        }
    }
}

impl GrainsParams {
    fn values(&self) -> [f32; NP] {
        [
            b(self.freeze.value()),
            self.grains.value() as f32,
            self.pos.value(),
            self.size.value(),
            self.pitch.value(),
            self.mix.value(),
            self.mode.value() as usize as f32,
            self.length.value(),
            self.length_sync.value() as usize as f32,
            self.spray.value(),
            self.scan.value(),
            self.regular.value(),
            self.chance.value(),
            self.shape.value(),
            self.skew.value(),
            self.reverse.value(),
            self.pitch_rand.value(),
            self.scale.value() as usize as f32,
            self.detune.value(),
            self.glide.value(),
            self.spread.value(),
            self.sync.value() as usize as f32,
            self.seed.value() as f32,
            self.skip.value(),
            self.live.value(),
            self.decay.value(),
            self.feedback.value(),
            self.low_cut.value(),
            self.high_cut.value(),
            b(self.kill_dry.value()),
            b(self.trails.value()),
            b(self.bypass.value()),
        ]
    }
}

pub struct GfxGrains {
    params: Arc<GrainsParams>,
    dsp: Grains,
    last: [f32; NP],
}

impl Default for GfxGrains {
    fn default() -> Self {
        Self { params: Arc::new(GrainsParams::default()), dsp: Grains::new(), last: [f32::NAN; NP] }
    }
}

impl GfxGrains {
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

impl Plugin for GfxGrains {
    const NAME: &'static str = "GoleM FX Grains";
    const VENDOR: &'static str = "GoleM";
    const URL: &'static str = "https://golem.fm";
    const EMAIL: &'static str = "contact@golem.fm";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        // stereo by nature: 2 / 2 first, so golem turns the lane stereo here
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

    fn initialize(&mut self, layout: &AudioIOLayout, config: &BufferConfig, ctx: &mut impl InitContext<Self>) -> bool {
        let channels = layout.main_output_channels.map_or(1, |c| c.get() as usize);
        self.sync(true);
        // the buffers (17 MB at 48 kHz, stereo) are allocated here, not on enable
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
        ctx.set_latency_samples(0);
        true
    }

    fn reset(&mut self) {
        self.sync(true);
        self.dsp.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.sync(false);
        let t = ctx.transport();
        self.dsp.set_transport(TransportInfo { tempo: t.tempo, pos_beats: t.pos_beats(), playing: t.playing });
        self.dsp.process(buffer.as_slice());
        self.params.active.set_output_value(self.dsp.active() as f32);
        self.params.state.set_output_value(self.dsp.state() as f32);
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxGrains {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxGrains001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Delay];
}

nih_export_vst3!(GfxGrains);
