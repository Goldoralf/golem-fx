// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2. Linear ranges only (golem contract).
//! `bypass` is the VST3 bypass parameter: engaging restarts the pattern. `pattern`, `steps`, `accent` are
//! integers (65,535 is exact in a 64-bit normalised value). `steps_count`, `picks_count` are read-only.

use crate::{Killswitch, Settings, PARAMS};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SyncKs {
    #[id = "free"]
    #[name = "Free"]
    Free,
    #[id = "1/4"]
    #[name = "1/4"]
    Q4,
    #[id = "1/8"]
    #[name = "1/8"]
    Q8,
    #[id = "1/8t"]
    #[name = "1/8T"]
    Q8T,
    #[id = "1/16"]
    #[name = "1/16"]
    Q16,
    #[id = "1/16t"]
    #[name = "1/16T"]
    Q16T,
    #[id = "1/32"]
    #[name = "1/32"]
    Q32,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum TriggerP {
    #[id = "engage"]
    #[name = "Engage"]
    Engage,
    #[id = "pick"]
    #[name = "Pick"]
    Pick,
    #[id = "beat"]
    #[name = "Beat"]
    Beat,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ShapeP {
    #[id = "gate"]
    #[name = "Gate"]
    Gate,
    #[id = "sine"]
    #[name = "Sine"]
    Sine,
    #[id = "triangle"]
    #[name = "Triangle"]
    Triangle,
    #[id = "ramp_up"]
    #[name = "Ramp Up"]
    RampUp,
    #[id = "ramp_down"]
    #[name = "Ramp Down"]
    RampDown,
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

/// `depth`, `length`, `sync`, `rate`, `trigger`, `sens`, `smooth` are golem's catalogue ids (`killswitch`).
#[derive(Params)]
struct KsParams {
    #[id = "depth"]
    depth: FloatParam,
    #[id = "length"]
    length: FloatParam,
    #[id = "sync"]
    sync: EnumParam<SyncKs>,
    #[id = "rate"]
    rate: FloatParam,
    #[id = "trigger"]
    trigger: EnumParam<TriggerP>,
    #[id = "sens"]
    sens: FloatParam,
    #[id = "smooth"]
    smooth: FloatParam,
    #[id = "shape"]
    shape: EnumParam<ShapeP>,
    #[id = "pattern"]
    pattern: IntParam,
    #[id = "steps"]
    steps: IntParam,
    #[id = "accent"]
    accent: IntParam,
    #[id = "swing"]
    swing: FloatParam,
    #[id = "offset"]
    offset: FloatParam,
    #[id = "kill"]
    kill: BoolParam,
    #[id = "restart"]
    restart: BoolParam,
    #[id = "width"]
    width: FloatParam,
    #[id = "bypass"]
    bypass: BoolParam,
    #[id = "steps_count"]
    steps_count: FloatParam,
    #[id = "picks_count"]
    picks_count: FloatParam,
}

impl Default for KsParams {
    fn default() -> Self {
        Self {
            depth: f(0, " %", 0),
            length: f(1, " %", 0),
            sync: EnumParam::new("Sync", SyncKs::Q16),
            rate: f(3, " Hz", 1),
            trigger: EnumParam::new("Trigger", TriggerP::Engage),
            sens: f(5, " %", 0),
            smooth: f(6, " ms", 1),
            shape: EnumParam::new("Shape", ShapeP::Gate),
            pattern: int(8),
            steps: int(9),
            accent: int(10),
            swing: f(11, " %", 0),
            offset: f(12, " %", 0),
            kill: BoolParam::new("Kill", false),
            restart: BoolParam::new("Restart", false),
            width: f(15, " %", 0),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            steps_count: FloatParam::new("Steps Count", 0.0, FloatRange::Linear { min: 0.0, max: 1.0e9 }).read_only(),
            picks_count: FloatParam::new("Picks Count", 0.0, FloatRange::Linear { min: 0.0, max: 1.0e9 }).read_only(),
        }
    }
}

impl KsParams {
    fn settings(&self) -> Settings {
        Settings {
            v: [
                self.depth.value(),
                self.length.value(),
                self.sync.value() as usize as f32,
                self.rate.value(),
                self.trigger.value() as usize as f32,
                self.sens.value(),
                self.smooth.value(),
                self.shape.value() as usize as f32,
                self.pattern.value() as f32,
                self.steps.value() as f32,
                self.accent.value() as f32,
                self.swing.value(),
                self.offset.value(),
                if self.kill.value() { 1.0 } else { 0.0 },
                if self.restart.value() { 1.0 } else { 0.0 },
                self.width.value(),
                if self.bypass.value() { 1.0 } else { 0.0 },
            ],
        }
    }
}

pub struct GfxKillswitch {
    params: Arc<KsParams>,
    dsp: Killswitch,
}

impl Default for GfxKillswitch {
    fn default() -> Self {
        Self { params: Arc::new(KsParams::default()), dsp: Killswitch::new() }
    }
}

impl Plugin for GfxKillswitch {
    const NAME: &'static str = "GoleM FX Killswitch";
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
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-killswitch"))
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
        let (steps, picks) = self.dsp.counters();
        self.params.steps_count.set_output_value(steps as f32);
        self.params.picks_count.set_output_value(picks as f32);
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxKillswitch {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxKillsw001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Modulation];
}

nih_export_vst3!(GfxKillswitch);
