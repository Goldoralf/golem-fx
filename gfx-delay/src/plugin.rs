// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): stereo 2 / 2 first, mono 1 / 1. Linear ranges only (golem contract).
//! `bypass` is the VST3 bypass parameter: Ardour drives it instead of deactivating the plugin, so the
//! repeats ring out (trails). Generated from PARAMS (same order and ids).

use crate::{Delay, Settings, PARAMS};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum TimeP {
    #[id = "1/4d"]
    #[name = "1/4•"]
    Q4D,
    #[id = "1/4"]
    #[name = "1/4"]
    Q4,
    #[id = "1/8d"]
    #[name = "1/8•"]
    Q8D,
    #[id = "1/8"]
    #[name = "1/8"]
    Q8,
    #[id = "1/8t"]
    #[name = "1/8T"]
    Q8T,
    #[id = "1/16"]
    #[name = "1/16"]
    Q16,
    #[id = "1/2"]
    #[name = "1/2"]
    Q2,
    #[id = "1/4t"]
    #[name = "1/4T"]
    Q4T,
    #[id = "1/16d"]
    #[name = "1/16•"]
    Q16D,
    #[id = "free"]
    #[name = "Free"]
    Free,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModelP {
    #[id = "digital"]
    #[name = "Digital"]
    Digital,
    #[id = "tape"]
    #[name = "Tape"]
    Tape,
    #[id = "bbd"]
    #[name = "BBD"]
    Bbd,
    #[id = "lofi"]
    #[name = "Lo-Fi"]
    LoFi,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum GlideP {
    #[id = "auto"]
    #[name = "Auto"]
    Auto,
    #[id = "glide"]
    #[name = "Glide"]
    Glide,
    #[id = "crossfade"]
    #[name = "Crossfade"]
    Crossfade,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum TapsP {
    #[id = "single"]
    #[name = "Single"]
    Single,
    #[id = "heads_12"]
    #[name = "Heads 1+2"]
    Heads12,
    #[id = "heads_123"]
    #[name = "Heads 1+2+3"]
    Heads123,
    #[id = "heads_13"]
    #[name = "Heads 1+3"]
    Heads13,
    #[id = "golden"]
    #[name = "Golden"]
    Golden,
    #[id = "triplet_gallop"]
    #[name = "Triplet Gallop"]
    TripletGallop,
    #[id = "dotted_quarter"]
    #[name = "Dotted + Quarter"]
    DottedQuarter,
    #[id = "swing"]
    #[name = "Swing"]
    Swing,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum DirectionP {
    #[id = "forward"]
    #[name = "Forward"]
    Forward,
    #[id = "reverse"]
    #[name = "Reverse"]
    Reverse,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum StereoP {
    #[id = "mono"]
    #[name = "Mono"]
    Mono,
    #[id = "stereo"]
    #[name = "Stereo"]
    Stereo,
    #[id = "ping_pong"]
    #[name = "Ping-Pong"]
    PingPong,
    #[id = "dual"]
    #[name = "Dual"]
    Dual,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `time`, `feedback`, `mix` are golem's catalogue ids (`delay` block).
#[derive(Params)]
struct DelayParams {
    #[id = "time"]
    time: EnumParam<TimeP>,
    #[id = "feedback"]
    feedback: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "model"]
    model: EnumParam<ModelP>,
    #[id = "time_ms"]
    time_ms: FloatParam,
    #[id = "glide"]
    glide: EnumParam<GlideP>,
    #[id = "glide_ms"]
    glide_ms: FloatParam,
    #[id = "low_cut"]
    low_cut: FloatParam,
    #[id = "high_cut"]
    high_cut: FloatParam,
    #[id = "drive"]
    drive: FloatParam,
    #[id = "mod"]
    modulation: FloatParam,
    #[id = "diffusion"]
    diffusion: FloatParam,
    #[id = "taps"]
    taps: EnumParam<TapsP>,
    #[id = "direction"]
    direction: EnumParam<DirectionP>,
    #[id = "loop_pitch"]
    loop_pitch: FloatParam,
    #[id = "loop_shift"]
    loop_shift: FloatParam,
    #[id = "age"]
    age: FloatParam,
    #[id = "swell"]
    swell: FloatParam,
    #[id = "sweep"]
    sweep: FloatParam,
    #[id = "duck"]
    duck: FloatParam,
    #[id = "duck_thresh"]
    duck_thresh: FloatParam,
    #[id = "duck_release"]
    duck_release: FloatParam,
    #[id = "stereo"]
    stereo: EnumParam<StereoP>,
    #[id = "ratio"]
    ratio: FloatParam,
    #[id = "width"]
    width: FloatParam,
    #[id = "kill_dry"]
    kill_dry: BoolParam,
    #[id = "hold"]
    hold: BoolParam,
    #[id = "bypass"]
    bypass: BoolParam,
    #[id = "trails"]
    trails: BoolParam,
}

impl Default for DelayParams {
    fn default() -> Self {
        Self {
            time: EnumParam::new("Time", TimeP::Q8D),
            feedback: f(1, " %", 0),
            mix: f(2, " %", 0),
            model: EnumParam::new("Model", ModelP::Digital),
            time_ms: f(4, " ms", 0),
            glide: EnumParam::new("Time Change", GlideP::Auto),
            glide_ms: f(6, " ms", 0),
            low_cut: f(7, " Hz", 0),
            high_cut: f(8, " Hz", 0),
            drive: f(9, " %", 0),
            modulation: f(10, " %", 0),
            diffusion: f(11, " %", 0),
            taps: EnumParam::new("Taps", TapsP::Single),
            direction: EnumParam::new("Direction", DirectionP::Forward),
            loop_pitch: f(14, " st", 2),
            loop_shift: f(15, " %", 0),
            age: f(16, " %", 0),
            swell: f(17, " ms", 0),
            sweep: f(18, " %", 0),
            duck: f(19, " dB", 1),
            duck_thresh: f(20, " dB", 1),
            duck_release: f(21, " ms", 0),
            stereo: EnumParam::new("Stereo", StereoP::Mono),
            ratio: f(23, "×", 2),
            width: f(24, " %", 0),
            kill_dry: BoolParam::new("Kill Dry", false),
            hold: BoolParam::new("Hold", false),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            trails: BoolParam::new("Trails", true),
        }
    }
}

impl DelayParams {
    fn settings(&self) -> Settings {
        Settings {
            v: [
                self.time.value() as usize as f32,
                self.feedback.value(),
                self.mix.value(),
                self.model.value() as usize as f32,
                self.time_ms.value(),
                self.glide.value() as usize as f32,
                self.glide_ms.value(),
                self.low_cut.value(),
                self.high_cut.value(),
                self.drive.value(),
                self.modulation.value(),
                self.diffusion.value(),
                self.taps.value() as usize as f32,
                self.direction.value() as usize as f32,
                self.loop_pitch.value(),
                self.loop_shift.value(),
                self.age.value(),
                self.swell.value(),
                self.sweep.value(),
                self.duck.value(),
                self.duck_thresh.value(),
                self.duck_release.value(),
                self.stereo.value() as usize as f32,
                self.ratio.value(),
                self.width.value(),
                if self.kill_dry.value() { 1.0 } else { 0.0 },
                if self.hold.value() { 1.0 } else { 0.0 },
                if self.bypass.value() { 1.0 } else { 0.0 },
                if self.trails.value() { 1.0 } else { 0.0 },
            ],
        }
    }
}

pub struct GfxDelay {
    params: Arc<DelayParams>,
    dsp: Delay,
}

impl Default for GfxDelay {
    fn default() -> Self {
        Self { params: Arc::new(DelayParams::default()), dsp: Delay::new() }
    }
}

impl Plugin for GfxDelay {
    const NAME: &'static str = "GoleM FX Delay";
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
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-delay"))
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

impl Vst3Plugin for GfxDelay {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxDelay0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Delay];
}

nih_export_vst3!(GfxDelay);
