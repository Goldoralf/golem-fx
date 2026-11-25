// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2. Linear ranges only (golem contract).

use crate::{Settings, Wah, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

/// The catalogue's seven first, in its order; then the added voices.
#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModelParam {
    #[id = "colorsound"]
    Colorsound,
    #[id = "dallas"]
    Dallas,
    #[id = "foxx"]
    Foxx,
    #[id = "jen"]
    Jen,
    #[id = "maestro"]
    Maestro,
    #[id = "selmer"]
    Selmer,
    #[id = "vox_v847"]
    #[name = "Vox V847"]
    VoxV847,
    #[id = "gcb_95"]
    #[name = "GCB-95"]
    Gcb95,
    #[id = "classic"]
    Classic,
    #[id = "clyde"]
    Clyde,
    #[id = "bass_105q"]
    #[name = "Bass 105Q"]
    Bass105q,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum Mode {
    #[id = "manual"]
    #[name = "manual"]
    Manual,
    #[id = "auto"]
    #[name = "auto"]
    Auto,
    #[id = "alien"]
    #[name = "alien"]
    Alien,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum Taper {
    #[id = "model"]
    Model,
    #[id = "linear"]
    Linear,
    #[id = "log"]
    Log,
    #[id = "reverse_log"]
    #[name = "Reverse Log"]
    ReverseLog,
    #[id = "s"]
    S,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `wah`, `volume`, `model`, `freq`, `mix`, `mode` are golem's catalogue ids (`wah`, `wah-color`).
#[derive(Params)]
struct WahParams {
    #[id = "wah"]
    wah: FloatParam,
    #[id = "volume"]
    volume: FloatParam,
    #[id = "model"]
    model: EnumParam<ModelParam>,
    #[id = "freq"]
    freq: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "mode"]
    mode: EnumParam<Mode>,
    #[id = "q"]
    q: FloatParam,
    #[id = "range"]
    range: FloatParam,
    #[id = "taper"]
    taper: EnumParam<Taper>,
    #[id = "heel"]
    heel: FloatParam,
    #[id = "toe"]
    toe: FloatParam,
    #[id = "drive"]
    drive: FloatParam,
    #[id = "auto_engage"]
    auto_engage: BoolParam,
    #[id = "engage_pos"]
    engage_pos: FloatParam,
    #[id = "engage_wait"]
    engage_wait: FloatParam,
    #[id = "env_sens"]
    env_sens: FloatParam,
    #[id = "env_att"]
    env_att: FloatParam,
    #[id = "env_rel"]
    env_rel: FloatParam,
}

impl Default for WahParams {
    fn default() -> Self {
        Self {
            wah: f(0, "", 3),
            volume: f(1, " dB", 1),
            model: EnumParam::new("Model", ModelParam::Gcb95),
            freq: f(3, " BPM", 0),
            mix: f(4, " %", 0),
            mode: EnumParam::new("Mode", Mode::Manual),
            q: f(6, " %", 0),
            range: f(7, " %", 0),
            taper: EnumParam::new("Taper", Taper::Model),
            heel: f(9, " %", 0),
            toe: f(10, " %", 0),
            drive: f(11, " %", 0),
            auto_engage: BoolParam::new("Auto Engage", false),
            engage_pos: f(13, " %", 1),
            engage_wait: f(14, " ms", 0),
            env_sens: f(15, " %", 0),
            env_att: f(16, " ms", 1),
            env_rel: f(17, " ms", 0),
        }
    }
}

impl WahParams {
    fn settings(&self) -> Settings {
        Settings {
            wah: self.wah.value(),
            volume: self.volume.value(),
            model: self.model.value() as usize,
            freq: self.freq.value(),
            mix: self.mix.value(),
            mode: self.mode.value() as u32,
            q: self.q.value(),
            range: self.range.value(),
            taper: self.taper.value() as u32,
            heel: self.heel.value(),
            toe: self.toe.value(),
            drive: self.drive.value(),
            auto_engage: self.auto_engage.value(),
            engage_pos: self.engage_pos.value(),
            engage_wait: self.engage_wait.value(),
            env_sens: self.env_sens.value(),
            env_att: self.env_att.value(),
            env_rel: self.env_rel.value(),
        }
    }
}

pub struct GfxWah {
    params: Arc<WahParams>,
    dsp: Wah,
}

impl Default for GfxWah {
    fn default() -> Self {
        Self { params: Arc::new(WahParams::default()), dsp: Wah::new() }
    }
}

impl Plugin for GfxWah {
    const NAME: &'static str = "GoleM FX Wah";
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
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-wah"))
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

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let s = self.params.settings();
        if s != self.dsp.settings() {
            self.dsp.set(s);
        }
        self.dsp.process(buffer.as_slice());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxWah {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxWah000001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Filter];
}

nih_export_vst3!(GfxWah);
