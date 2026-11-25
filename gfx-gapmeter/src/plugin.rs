// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 (golem picks at insert).
//!
//! The sender thread is started at initialisation, never on the audio thread. It reads the probe's
//! atomics every 10 ms and sends OSC over UDP to 127.0.0.1:`port`: gap events as they come, a heartbeat
//! every ~50 ms. It stops when the plugin is dropped.

use crate::{sender, GapMeter, Settings, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::JoinHandle;

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

#[derive(Params)]
struct GapParams {
    #[id = "slot"]
    slot: IntParam,
    #[id = "thresh"]
    thresh: FloatParam,
    #[id = "min_gap"]
    min_gap: FloatParam,
    #[id = "port"]
    port: FloatParam,
    #[id = "tone"]
    tone: BoolParam,
    #[id = "tone_freq"]
    tone_freq: FloatParam,
    #[id = "tone_level"]
    tone_level: FloatParam,
}

impl Default for GapParams {
    fn default() -> Self {
        Self {
            slot: IntParam::new("Slot", 0, IntRange::Linear { min: 0, max: 7 }),
            thresh: f(1, " dB", 1),
            min_gap: f(2, " ms", 1),
            port: f(3, "", 0),
            tone: BoolParam::new("Test Tone", false),
            tone_freq: f(5, " Hz", 0),
            tone_level: f(6, " dB", 1),
        }
    }
}

impl GapParams {
    fn settings(&self) -> Settings {
        Settings {
            v: [
                self.slot.value() as f32,
                self.thresh.value(),
                self.min_gap.value(),
                self.port.value(),
                if self.tone.value() { 1.0 } else { 0.0 },
                self.tone_freq.value(),
                self.tone_level.value(),
            ],
        }
    }
}

pub struct GfxGapmeter {
    params: Arc<GapParams>,
    dsp: GapMeter,
    thread: Option<JoinHandle<()>>,
}

impl Default for GfxGapmeter {
    fn default() -> Self {
        Self { params: Arc::new(GapParams::default()), dsp: GapMeter::new(), thread: None }
    }
}

impl Drop for GfxGapmeter {
    fn drop(&mut self) {
        self.dsp.shared.quit.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Plugin for GfxGapmeter {
    const NAME: &'static str = "GoleM FX Gapmeter";
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
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-gapmeter"))
    }

    fn initialize(&mut self, layout: &AudioIOLayout, config: &BufferConfig, _ctx: &mut impl InitContext<Self>) -> bool {
        let channels = layout.main_output_channels.map_or(1, |c| c.get() as usize);
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
        self.dsp.set(self.params.settings());
        if self.thread.is_none() {
            let shared = self.dsp.shared.clone();
            self.thread = std::thread::Builder::new().name("gfx-gapmeter-osc".into()).spawn(move || sender(shared)).ok();
        }
        true
    }

    fn reset(&mut self) {
        self.dsp.set(self.params.settings());
        self.dsp.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let s = self.params.settings();
        if s != self.dsp.settings() {
            self.dsp.set(s);
        }
        self.dsp.process(buffer.as_slice());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxGapmeter {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxGapmeter1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Analyzer];
}

nih_export_vst3!(GfxGapmeter);
