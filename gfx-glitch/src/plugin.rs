// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): stereo 2 / 2 first, mono 1 / 1 (stereo by nature: golem turns the lane
//! stereo). Linear ranges only (golem contract). `bypass` is the VST3 bypass parameter: with `trails`
//! the wet rings out. The latency is reported only with `align` (setup only). Generated from PARAMS.

use crate::{Glitch, PARAMS};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

gfx_dsp::sync_param_enum!();

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum FxP {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "low-slope"]
    #[name = "Low Slope"]
    LowSlope,
    #[id = "high-slope"]
    #[name = "High Slope"]
    HighSlope,
    #[id = "band-keep"]
    #[name = "Band Keep"]
    BandKeep,
    #[id = "band-cut"]
    #[name = "Band Cut"]
    BandCut,
    #[id = "band-lift"]
    #[name = "Band Lift"]
    BandLift,
    #[id = "tilt"]
    #[name = "Tilt"]
    Tilt,
    #[id = "quiet-cut"]
    #[name = "Quiet Cut"]
    QuietCut,
    #[id = "loud-cut"]
    #[name = "Loud Cut"]
    LoudCut,
    #[id = "sharpen"]
    #[name = "Sharpen"]
    Sharpen,
    #[id = "thin"]
    #[name = "Thin"]
    Thin,
    #[id = "peak-only"]
    #[name = "Peak Only"]
    PeakOnly,
    #[id = "hold"]
    #[name = "Hold"]
    Hold,
    #[id = "scatter-hold"]
    #[name = "Scatter Hold"]
    ScatterHold,
    #[id = "stutter-hold"]
    #[name = "Stutter Hold"]
    StutterHold,
    #[id = "attack-hold"]
    #[name = "Attack Hold"]
    AttackHold,
    #[id = "ring-hold"]
    #[name = "Ring Hold"]
    RingHold,
    #[id = "haze-hold"]
    #[name = "Haze Hold"]
    HazeHold,
    #[id = "spread-region"]
    #[name = "Spread Region"]
    SpreadRegion,
    #[id = "squeeze-region"]
    #[name = "Squeeze Region"]
    SqueezeRegion,
    #[id = "slide-region"]
    #[name = "Slide Region"]
    SlideRegion,
    #[id = "blur"]
    #[name = "Blur"]
    Blur,
    #[id = "crush"]
    #[name = "Crush"]
    Crush,
    #[id = "corrupt"]
    #[name = "Corrupt"]
    Corrupt,
    #[id = "sparkle"]
    #[name = "Sparkle"]
    Sparkle,
    #[id = "bin-clip"]
    #[name = "Bin Clip"]
    BinClip,
    #[id = "flip"]
    #[name = "Flip"]
    Flip,
    #[id = "phase-swap"]
    #[name = "Phase Swap"]
    PhaseSwap,
    #[id = "clump"]
    #[name = "Clump"]
    Clump,
    #[id = "shuffle"]
    #[name = "Shuffle"]
    Shuffle,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SizeP {
    #[id = "512"]
    #[name = "512"]
    S512,
    #[id = "1024"]
    #[name = "1024"]
    S1024,
    #[id = "2048"]
    #[name = "2048"]
    S2048,
    #[id = "4096"]
    #[name = "4096"]
    S4096,
    #[id = "8192"]
    #[name = "8192"]
    S8192,
    #[id = "16384"]
    #[name = "16384"]
    S16384,
    #[id = "32768"]
    #[name = "32768"]
    S32768,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum WindowP {
    #[id = "smooth"]
    #[name = "Smooth"]
    Smooth,
    #[id = "raw"]
    #[name = "Raw"]
    Raw,
    #[id = "ringing"]
    #[name = "Ringing"]
    Ringing,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ShapeP {
    #[id = "sine"]
    #[name = "Sine"]
    Sine,
    #[id = "triangle"]
    #[name = "Triangle"]
    Triangle,
    #[id = "saw-up"]
    #[name = "Saw Up"]
    SawUp,
    #[id = "saw-down"]
    #[name = "Saw Down"]
    SawDown,
    #[id = "square"]
    #[name = "Square"]
    Square,
    #[id = "random-steps"]
    #[name = "Random Steps"]
    RandomSteps,
    #[id = "random-smooth"]
    #[name = "Random Smooth"]
    RandomSmooth,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum DestP {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "amount-a"]
    #[name = "Amount A"]
    AmountA,
    #[id = "amount-b"]
    #[name = "Amount B"]
    AmountB,
    #[id = "amount-c"]
    #[name = "Amount C"]
    AmountC,
    #[id = "all-amounts"]
    #[name = "All Amounts"]
    AllAmounts,
    #[id = "sustain"]
    #[name = "Sustain"]
    Sustain,
    #[id = "pitch"]
    #[name = "Pitch"]
    Pitch,
    #[id = "spread"]
    #[name = "Spread"]
    Spread,
    #[id = "drift"]
    #[name = "Drift"]
    Drift,
    #[id = "region-a"]
    #[name = "Region A"]
    RegionA,
    #[id = "region-b"]
    #[name = "Region B"]
    RegionB,
    #[id = "region-c"]
    #[name = "Region C"]
    RegionC,
    #[id = "density"]
    #[name = "Density"]
    Density,
    #[id = "tone"]
    #[name = "Tone"]
    Tone,
    #[id = "wet"]
    #[name = "Wet"]
    Wet,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

#[derive(Params)]
struct GlitchParams {
    #[id = "fx_a"]
    fx_a: EnumParam<FxP>,
    #[id = "amt_a"]
    amt_a: FloatParam,
    #[id = "sustain"]
    sustain: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "size"]
    size: EnumParam<SizeP>,
    #[id = "window"]
    window: EnumParam<WindowP>,
    #[id = "trim"]
    trim: FloatParam,
    #[id = "cloud"]
    cloud: FloatParam,
    #[id = "damp"]
    damp: FloatParam,
    #[id = "pitch"]
    pitch: FloatParam,
    #[id = "spread"]
    spread: FloatParam,
    #[id = "drift"]
    drift: FloatParam,
    #[id = "lo_a"]
    lo_a: FloatParam,
    #[id = "hi_a"]
    hi_a: FloatParam,
    #[id = "fx_b"]
    fx_b: EnumParam<FxP>,
    #[id = "amt_b"]
    amt_b: FloatParam,
    #[id = "lo_b"]
    lo_b: FloatParam,
    #[id = "hi_b"]
    hi_b: FloatParam,
    #[id = "fx_c"]
    fx_c: EnumParam<FxP>,
    #[id = "amt_c"]
    amt_c: FloatParam,
    #[id = "lo_c"]
    lo_c: FloatParam,
    #[id = "hi_c"]
    hi_c: FloatParam,
    #[id = "density"]
    density: FloatParam,
    #[id = "lfo_shape"]
    lfo_shape: EnumParam<ShapeP>,
    #[id = "rate"]
    rate: FloatParam,
    #[id = "sync"]
    sync: EnumParam<SyncParam>,
    #[id = "lfo_depth"]
    lfo_depth: FloatParam,
    #[id = "lfo_dest"]
    lfo_dest: EnumParam<DestP>,
    #[id = "env_sens"]
    env_sens: FloatParam,
    #[id = "env_att"]
    env_att: FloatParam,
    #[id = "env_rel"]
    env_rel: FloatParam,
    #[id = "env_depth"]
    env_depth: FloatParam,
    #[id = "env_dest"]
    env_dest: EnumParam<DestP>,
    #[id = "wet"]
    wet: FloatParam,
    #[id = "hpf"]
    hpf: FloatParam,
    #[id = "lpf"]
    lpf: FloatParam,
    #[id = "width"]
    width: FloatParam,
    #[id = "align"]
    align: BoolParam,
    #[id = "trails"]
    trails: BoolParam,
    #[id = "bypass"]
    bypass: BoolParam,
}

impl Default for GlitchParams {
    fn default() -> Self {
        Self {
            fx_a: EnumParam::new("Effect", FxP::RingHold),
            amt_a: f(1, " %", 0),
            sustain: f(2, " ms", 0),
            mix: f(3, " %", 0),
            size: EnumParam::new("Size", SizeP::S2048),
            window: EnumParam::new("Window", WindowP::Smooth),
            trim: f(6, " dB", 1),
            cloud: f(7, " %", 0),
            damp: f(8, " %", 0),
            pitch: f(9, " st", 1),
            spread: f(10, " %", 0),
            drift: f(11, " Hz", 1),
            lo_a: f(12, " oct", 2),
            hi_a: f(13, " oct", 2),
            fx_b: EnumParam::new("Effect B", FxP::Off),
            amt_b: f(15, " %", 0),
            lo_b: f(16, " oct", 2),
            hi_b: f(17, " oct", 2),
            fx_c: EnumParam::new("Effect C", FxP::Off),
            amt_c: f(19, " %", 0),
            lo_c: f(20, " oct", 2),
            hi_c: f(21, " oct", 2),
            density: f(22, " %", 0),
            lfo_shape: EnumParam::new("LFO Shape", ShapeP::Sine),
            rate: f(24, " Hz", 2),
            sync: EnumParam::new("LFO Sync", SyncParam::Off),
            lfo_depth: f(26, " %", 0),
            lfo_dest: EnumParam::new("LFO Destination", DestP::Off),
            env_sens: f(28, " %", 0),
            env_att: f(29, " ms", 0),
            env_rel: f(30, " ms", 0),
            env_depth: f(31, " %", 0),
            env_dest: EnumParam::new("Envelope Destination", DestP::Off),
            wet: f(33, " dB", 1),
            hpf: f(34, " Hz", 0),
            lpf: f(35, " Hz", 0),
            width: f(36, " %", 0),
            align: BoolParam::new("Align Dry", false),
            trails: BoolParam::new("Trails", true),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
        }
    }
}

impl GlitchParams {
    fn values(&self) -> [f32; crate::NP] {
        [
            self.fx_a.value() as usize as f32,
            self.amt_a.value(),
            self.sustain.value(),
            self.mix.value(),
            self.size.value() as usize as f32,
            self.window.value() as usize as f32,
            self.trim.value(),
            self.cloud.value(),
            self.damp.value(),
            self.pitch.value(),
            self.spread.value(),
            self.drift.value(),
            self.lo_a.value(),
            self.hi_a.value(),
            self.fx_b.value() as usize as f32,
            self.amt_b.value(),
            self.lo_b.value(),
            self.hi_b.value(),
            self.fx_c.value() as usize as f32,
            self.amt_c.value(),
            self.lo_c.value(),
            self.hi_c.value(),
            self.density.value(),
            self.lfo_shape.value() as usize as f32,
            self.rate.value(),
            self.sync.value() as usize as f32,
            self.lfo_depth.value(),
            self.lfo_dest.value() as usize as f32,
            self.env_sens.value(),
            self.env_att.value(),
            self.env_rel.value(),
            self.env_depth.value(),
            self.env_dest.value() as usize as f32,
            self.wet.value(),
            self.hpf.value(),
            self.lpf.value(),
            self.width.value(),
            if self.align.value() { 1.0 } else { 0.0 },
            if self.trails.value() { 1.0 } else { 0.0 },
            if self.bypass.value() { 1.0 } else { 0.0 },
        ]
    }
}

pub struct GfxGlitch {
    params: Arc<GlitchParams>,
    dsp: Glitch,
    last: [f32; crate::NP],
    reported: u32,
}

impl Default for GfxGlitch {
    fn default() -> Self {
        Self { params: Arc::new(GlitchParams::default()), dsp: Glitch::new(), last: [f32::NAN; crate::NP], reported: 0 }
    }
}

impl GfxGlitch {
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

impl Plugin for GfxGlitch {
    const NAME: &'static str = "GoleM FX Glitch";
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

    #[cfg(feature = "gui")]
    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-glitch"))
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
        let t = ctx.transport();
        self.dsp.set_transport(TransportInfo { tempo: t.tempo, pos_beats: t.pos_beats(), playing: t.playing });
        self.dsp.process(buffer.as_slice());
        let l = self.dsp.latency() as u32;
        if l != self.reported {
            self.reported = l;
            ctx.set_latency_samples(l);
        }
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxGlitch {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxGlitch001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Spatial];
}

nih_export_vst3!(GfxGlitch);
