// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second. Linear ranges only (golem
//! contract); `ir_a` / `ir_b` are linear `IntParam`s 0 – 65,535, so REAPER's normalised value lands
//! exactly. `bypass` is the VST3 bypass parameter (10 ms crossfade wet → dry). Zero latency.
//!
//! Slots (README §2.1): a slot change posts a load to nih-plug's background thread at once
//! (`execute_background`); the kernel comes back through a lock-free mailbox and is crossfaded in;
//! `loaded_a` / `loaded_b` (read-only meters) read the slot once it is installed, −1 when the slot or
//! its file is missing, −2 when the file is unreadable or its hash differs. `initialize()` loads the
//! slots it holds synchronously on a cache miss, so a render that starts with its slots set is right
//! from its first sample. `ir_files` (persisted) keeps the paths of the IRs that loaded, for a session
//! opened where the index lacks them.

use crate::{builtin, Cab, NP, PARAMS};
use gfx_dsp::ir_engine::{self, Host, Worker};
use gfx_dsp::ir_host::{Job, Loader, Persisted};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::{Arc, RwLock};

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum AlignP {
    #[id = "raw"]
    #[name = "Raw"]
    Raw,
    #[id = "onset"]
    #[name = "Onset"]
    Onset,
    #[id = "correlation"]
    #[name = "Correlation"]
    Correlation,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum NormP {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "peak"]
    #[name = "Peak"]
    Peak,
    #[id = "loudness"]
    #[name = "Loudness"]
    Loudness,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum Slope {
    #[id = "12"]
    #[name = "12 dB"]
    S12,
    #[id = "24"]
    #[name = "24 dB"]
    S24,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum StereoP {
    #[id = "linked"]
    #[name = "Linked"]
    Linked,
    #[id = "split"]
    #[name = "Split"]
    Split,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

fn slot(i: usize) -> IntParam {
    let pi = PARAMS[i];
    IntParam::new(pi.name, pi.default as i32, IntRange::Linear { min: pi.min as i32, max: pi.max as i32 })
}

fn meter(name: &str) -> FloatParam {
    FloatParam::new(name, 0.0, FloatRange::Linear { min: -2.0, max: 65535.0 })
        .with_value_to_string(formatters::v2s_f32_rounded(0))
        .read_only()
}

/// `level` is golem's catalogue id (`cab` block); `ir_a` carries the block's file.
#[derive(Params)]
struct CabParams {
    #[id = "level"]
    level: FloatParam,
    #[id = "blend"]
    blend: FloatParam,
    #[id = "lowcut"]
    lowcut: FloatParam,
    #[id = "highcut"]
    highcut: FloatParam,
    #[id = "align"]
    align: EnumParam<AlignP>,
    #[id = "ir_a"]
    ir_a: IntParam,
    #[id = "ir_b"]
    ir_b: IntParam,
    #[id = "level_a"]
    level_a: FloatParam,
    #[id = "level_b"]
    level_b: FloatParam,
    #[id = "polarity_b"]
    polarity_b: BoolParam,
    #[id = "delay_b"]
    delay_b: FloatParam,
    #[id = "length"]
    length: FloatParam,
    #[id = "fade"]
    fade: FloatParam,
    #[id = "normalize"]
    normalize: EnumParam<NormP>,
    #[id = "min_phase"]
    min_phase: BoolParam,
    #[id = "lowcut_slope"]
    lowcut_slope: EnumParam<Slope>,
    #[id = "highcut_slope"]
    highcut_slope: EnumParam<Slope>,
    #[id = "dry"]
    dry: FloatParam,
    #[id = "bass_keep"]
    bass_keep: FloatParam,
    #[id = "stereo"]
    stereo: EnumParam<StereoP>,
    #[id = "bypass"]
    bypass: BoolParam,
    /// Read-only meters (golem contract): the slot in place, or −1 / −2.
    #[id = "loaded_a"]
    loaded_a: FloatParam,
    #[id = "loaded_b"]
    loaded_b: FloatParam,
    /// `{slot: (path, sha256)}` of the IRs that loaded (fallback when the index lacks a slot).
    #[persist = "ir_files"]
    ir_files: Arc<RwLock<Persisted>>,
}

impl Default for CabParams {
    fn default() -> Self {
        Self {
            level: f(0, " dB", 1),
            blend: f(1, " %", 0),
            lowcut: f(2, " Hz", 0),
            highcut: f(3, " Hz", 0),
            align: EnumParam::new("Align", AlignP::Onset),
            ir_a: slot(5),
            ir_b: slot(6),
            level_a: f(7, " dB", 1),
            level_b: f(8, " dB", 1),
            polarity_b: BoolParam::new("Polarity B", false),
            delay_b: f(10, " ms", 2),
            length: f(11, " ms", 0),
            fade: f(12, " %", 0),
            normalize: EnumParam::new("Normalize", NormP::Loudness),
            min_phase: BoolParam::new("Min Phase", false),
            lowcut_slope: EnumParam::new("Low Cut Slope", Slope::S12),
            highcut_slope: EnumParam::new("High Cut Slope", Slope::S12),
            dry: f(17, " dB", 1),
            bass_keep: f(18, " Hz", 0),
            stereo: EnumParam::new("Stereo", StereoP::Linked),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            loaded_a: meter("Loaded A"),
            loaded_b: meter("Loaded B"),
            ir_files: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl CabParams {
    fn values(&self) -> [f32; NP] {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        [
            self.level.value(),
            self.blend.value(),
            self.lowcut.value(),
            self.highcut.value(),
            self.align.value() as usize as f32,
            self.ir_a.value() as f32,
            self.ir_b.value() as f32,
            self.level_a.value(),
            self.level_b.value(),
            b(self.polarity_b.value()),
            self.delay_b.value(),
            self.length.value(),
            self.fade.value(),
            self.normalize.value() as usize as f32,
            b(self.min_phase.value()),
            self.lowcut_slope.value() as usize as f32,
            self.highcut_slope.value() as usize as f32,
            self.dry.value(),
            self.bass_keep.value(),
            self.stereo.value() as usize as f32,
            b(self.bypass.value()),
        ]
    }
}

/// Work for nih-plug's background thread.
pub enum Task {
    Load(Job),
    Collect,
}

pub struct GfxCab {
    params: Arc<CabParams>,
    dsp: Cab,
    last: [f32; NP],
    host: Host,
    worker: Arc<Worker>,
}

impl Default for GfxCab {
    fn default() -> Self {
        let params = Arc::new(CabParams::default());
        let mut loader = Loader::new(Some(builtin::render));
        loader.persisted = params.ir_files.clone();
        let (host, worker) = ir_engine::pair(loader);
        let mut dsp = Cab::new();
        dsp.core().set_async();
        Self { params, dsp, last: [f32::NAN; NP], host, worker }
    }
}

impl GfxCab {
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

impl Plugin for GfxCab {
    const NAME: &'static str = "GoleM FX Cab";
    const VENDOR: &'static str = "GoleM";
    const URL: &'static str = "https://golem.fm";
    const EMAIL: &'static str = "contact@golem.fm";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // mono first (golem contract), stereo second
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout { main_input_channels: NonZeroU32::new(1), main_output_channels: NonZeroU32::new(1), ..AudioIOLayout::const_default() },
        AudioIOLayout { main_input_channels: NonZeroU32::new(2), main_output_channels: NonZeroU32::new(2), ..AudioIOLayout::const_default() },
    ];

    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = Task;

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn task_executor(&mut self) -> TaskExecutor<Self> {
        let w = self.worker.clone();
        Box::new(move |t| match t {
            Task::Load(job) => w.run(&job),
            Task::Collect => w.collect(),
        })
    }

    fn initialize(&mut self, layout: &AudioIOLayout, config: &BufferConfig, _ctx: &mut impl InitContext<Self>) -> bool {
        let channels = layout.main_output_channels.map_or(1, |c| c.get() as usize);
        self.sync(true);
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
        // the slots it holds, synchronously on a cache miss (no wait anywhere else)
        let want = self.dsp.want();
        if !self.dsp.core_ref().installed_matches(&want) {
            let job = self.dsp.core_ref().job(&want, 0);
            let done = self.worker.run_now(&job);
            self.dsp.core().apply(done, true);
        }
        self.host.settle(want);
        self.worker.collect();
        self.params.loaded_a.set_output_value(self.dsp.loaded(0));
        self.params.loaded_b.set_output_value(self.dsp.loaded(1));
        true
    }

    fn reset(&mut self) {
        self.sync(true);
        self.dsp.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.sync(false);
        let now = self.dsp.clock();
        let returned = self.host.tick(self.dsp.core(), now, |job| ctx.execute_background(Task::Load(job)));
        if returned {
            ctx.execute_background(Task::Collect);
        }
        self.dsp.process(buffer.as_slice());
        self.params.loaded_a.set_output_value(self.dsp.loaded(0));
        self.params.loaded_b.set_output_value(self.dsp.loaded(1));
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxCab {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxCab000001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Filter];
}

nih_export_vst3!(GfxCab);
