// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): stereo 2 / 2 first (golem turns the lane stereo here, a mono DI
//! arrives on both inputs), mono 1 / 1 second. Linear ranges only (golem contract); `ir_a` is a linear
//! `IntParam` 0 – 65,535. `bypass` is the VST3 bypass parameter; with `trails` on the tail rings out
//! (§Trails of the contract). Zero latency.
//!
//! The slot plumbing is gfx-cab's (gfx-dsp `ir_engine`, README §2.1): a slot change loads on nih-plug's background thread
//! and is crossfaded in; `loaded_a` (read-only) reads the slot once installed, −1 missing, −2
//! unreadable or hash mismatch; `initialize()` loads synchronously on a cache miss; `ir_files`
//! (persisted) keeps the paths of the IRs that loaded.

use crate::{IrFx, NP, PARAMS};
use gfx_dsp::ir_engine::{self, Host, Worker};
use gfx_dsp::ir_host::{Job, Loader, Persisted};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::{Arc, RwLock};

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

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `wet` and `dry` are golem's catalogue ids (`ir-fx` block); `ir_a` carries the block's file.
#[derive(Params)]
struct IrFxParams {
    #[id = "wet"]
    wet: FloatParam,
    #[id = "dry"]
    dry: FloatParam,
    #[id = "lowcut"]
    lowcut: FloatParam,
    #[id = "highcut"]
    highcut: FloatParam,
    #[id = "ir_a"]
    ir_a: IntParam,
    #[id = "predelay"]
    predelay: FloatParam,
    #[id = "length"]
    length: FloatParam,
    #[id = "fade"]
    fade: FloatParam,
    #[id = "normalize"]
    normalize: EnumParam<NormP>,
    #[id = "reverse"]
    reverse: BoolParam,
    #[id = "width"]
    width: FloatParam,
    #[id = "lowcut_slope"]
    lowcut_slope: EnumParam<Slope>,
    #[id = "highcut_slope"]
    highcut_slope: EnumParam<Slope>,
    #[id = "trails"]
    trails: BoolParam,
    #[id = "bypass"]
    bypass: BoolParam,
    /// Read-only meter (golem contract): the slot in place, or −1 / −2.
    #[id = "loaded_a"]
    loaded_a: FloatParam,
    /// `{slot: (path, sha256)}` of the IRs that loaded (fallback when the index lacks a slot).
    #[persist = "ir_files"]
    ir_files: Arc<RwLock<Persisted>>,
}

impl Default for IrFxParams {
    fn default() -> Self {
        let pi = PARAMS[4];
        Self {
            wet: f(0, " dB", 1),
            dry: f(1, " dB", 1),
            lowcut: f(2, " Hz", 0),
            highcut: f(3, " Hz", 0),
            ir_a: IntParam::new(pi.name, 0, IntRange::Linear { min: 0, max: 65535 }),
            predelay: f(5, " ms", 1),
            length: f(6, " ms", 0),
            fade: f(7, " %", 0),
            normalize: EnumParam::new("Normalize", NormP::Loudness),
            reverse: BoolParam::new("Reverse", false),
            width: f(10, " %", 0),
            lowcut_slope: EnumParam::new("Low Cut Slope", Slope::S12),
            highcut_slope: EnumParam::new("High Cut Slope", Slope::S12),
            trails: BoolParam::new("Trails", true),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            loaded_a: FloatParam::new("Loaded", 0.0, FloatRange::Linear { min: -2.0, max: 65535.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(0))
                .read_only(),
            ir_files: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl IrFxParams {
    fn values(&self) -> [f32; NP] {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        [
            self.wet.value(),
            self.dry.value(),
            self.lowcut.value(),
            self.highcut.value(),
            self.ir_a.value() as f32,
            self.predelay.value(),
            self.length.value(),
            self.fade.value(),
            self.normalize.value() as usize as f32,
            b(self.reverse.value()),
            self.width.value(),
            self.lowcut_slope.value() as usize as f32,
            self.highcut_slope.value() as usize as f32,
            b(self.trails.value()),
            b(self.bypass.value()),
        ]
    }
}

/// Work for nih-plug's background thread.
pub enum Task {
    Load(Job),
    Collect,
}

pub struct GfxIrFx {
    params: Arc<IrFxParams>,
    dsp: IrFx,
    last: [f32; NP],
    host: Host,
    worker: Arc<Worker>,
}

impl Default for GfxIrFx {
    fn default() -> Self {
        let params = Arc::new(IrFxParams::default());
        let mut loader = Loader::new(None);
        loader.persisted = params.ir_files.clone();
        let (host, worker) = ir_engine::pair(loader);
        let mut dsp = IrFx::new();
        dsp.core().set_async();
        Self { params, dsp, last: [f32::NAN; NP], host, worker }
    }
}

impl GfxIrFx {
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

impl Plugin for GfxIrFx {
    const NAME: &'static str = "GoleM FX IR FX";
    const VENDOR: &'static str = "GoleM";
    const URL: &'static str = "https://golem.fm";
    const EMAIL: &'static str = "contact@golem.fm";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // stereo by nature: 2 / 2 first, mono second
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout { main_input_channels: NonZeroU32::new(2), main_output_channels: NonZeroU32::new(2), ..AudioIOLayout::const_default() },
        AudioIOLayout { main_input_channels: NonZeroU32::new(1), main_output_channels: NonZeroU32::new(1), ..AudioIOLayout::const_default() },
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
        let channels = layout.main_output_channels.map_or(2, |c| c.get() as usize);
        self.sync(true);
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
        let want = self.dsp.want();
        if !self.dsp.core_ref().installed_matches(&want) {
            let job = self.dsp.core_ref().job(&want, 0);
            let done = self.worker.run_now(&job);
            self.dsp.core().apply(done, true);
        }
        self.host.settle(want);
        self.worker.collect();
        self.params.loaded_a.set_output_value(self.dsp.loaded());
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
        self.params.loaded_a.set_output_value(self.dsp.loaded());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxIrFx {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxIrFx00001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Reverb];
}

nih_export_vst3!(GfxIrFx);
