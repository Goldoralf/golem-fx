// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): stereo 2 / 2 first (mono in, stereo out by nature: a stereo input is
//! summed), mono 1 / 1. Linear ranges only (golem contract). Generated from PARAMS (same order and ids).

use crate::{Rotary, Settings, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SpeedP {
    #[id = "stop"]
    #[name = "Stop"]
    Stop,
    #[id = "slow"]
    #[name = "Slow"]
    Slow,
    #[id = "fast"]
    #[name = "Fast"]
    Fast,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum CabinetP {
    #[id = "122"]
    #[name = "122"]
    L122,
    #[id = "147"]
    #[name = "147"]
    L147,
    #[id = "145"]
    #[name = "145"]
    L145,
    #[id = "vibratone"]
    #[name = "Vibratone"]
    Vibratone,
    #[id = "122_rear"]
    #[name = "122 Rear"]
    L122Rear,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// `speed`, `mic`, `spacing`, `depth` are golem's catalogue ids (`rotary` block).
#[derive(Params)]
struct RotaryParams {
    #[id = "speed"]
    speed: EnumParam<SpeedP>,
    #[id = "mic"]
    mic: FloatParam,
    #[id = "spacing"]
    spacing: FloatParam,
    #[id = "depth"]
    depth: FloatParam,
    #[id = "cabinet"]
    cabinet: EnumParam<CabinetP>,
    #[id = "ramp"]
    ramp: FloatParam,
    #[id = "drive"]
    drive: FloatParam,
    #[id = "balance"]
    balance: FloatParam,
    #[id = "horn_slow"]
    horn_slow: FloatParam,
    #[id = "horn_fast"]
    horn_fast: FloatParam,
    #[id = "drum_slow"]
    drum_slow: FloatParam,
    #[id = "drum_fast"]
    drum_fast: FloatParam,
    #[id = "horn_accel"]
    horn_accel: FloatParam,
    #[id = "horn_decel"]
    horn_decel: FloatParam,
    #[id = "drum_accel"]
    drum_accel: FloatParam,
    #[id = "drum_decel"]
    drum_decel: FloatParam,
    #[id = "brake"]
    brake: FloatParam,
    #[id = "stop_angle"]
    stop_angle: FloatParam,
    #[id = "counter"]
    counter: BoolParam,
    #[id = "wobble"]
    wobble: FloatParam,
    #[id = "bass_keep"]
    bass_keep: FloatParam,
    #[id = "level"]
    level: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
}

impl Default for RotaryParams {
    fn default() -> Self {
        Self {
            speed: EnumParam::new("Speed", SpeedP::Slow),
            mic: f(1, " %", 0),
            spacing: f(2, " %", 0),
            depth: f(3, " %", 0),
            cabinet: EnumParam::new("Cabinet", CabinetP::L122),
            ramp: f(5, " %", 0),
            drive: f(6, " %", 0),
            balance: f(7, " %", 0),
            horn_slow: f(8, " Hz", 2),
            horn_fast: f(9, " Hz", 2),
            drum_slow: f(10, " Hz", 2),
            drum_fast: f(11, " Hz", 2),
            horn_accel: f(12, " s", 2),
            horn_decel: f(13, " s", 2),
            drum_accel: f(14, " s", 1),
            drum_decel: f(15, " s", 1),
            brake: f(16, " %", 0),
            stop_angle: f(17, "°", 0),
            counter: BoolParam::new("Counter-rotate", true),
            wobble: f(19, " %", 0),
            bass_keep: f(20, " Hz", 0),
            level: f(21, " dB", 1),
            mix: f(22, " %", 0),
        }
    }
}

impl RotaryParams {
    fn settings(&self) -> Settings {
        Settings {
            v: [
                self.speed.value() as usize as f32,
                self.mic.value(),
                self.spacing.value(),
                self.depth.value(),
                self.cabinet.value() as usize as f32,
                self.ramp.value(),
                self.drive.value(),
                self.balance.value(),
                self.horn_slow.value(),
                self.horn_fast.value(),
                self.drum_slow.value(),
                self.drum_fast.value(),
                self.horn_accel.value(),
                self.horn_decel.value(),
                self.drum_accel.value(),
                self.drum_decel.value(),
                self.brake.value(),
                self.stop_angle.value(),
                if self.counter.value() { 1.0 } else { 0.0 },
                self.wobble.value(),
                self.bass_keep.value(),
                self.level.value(),
                self.mix.value(),
            ],
        }
    }
}

pub struct GfxRotary {
    params: Arc<RotaryParams>,
    dsp: Rotary,
}

impl Default for GfxRotary {
    fn default() -> Self {
        Self { params: Arc::new(RotaryParams::default()), dsp: Rotary::new() }
    }
}

impl Plugin for GfxRotary {
    const NAME: &'static str = "GoleM FX Rotary";
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

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let s = self.params.settings();
        if s != self.dsp.settings() {
            self.dsp.set(s);
        }
        self.dsp.process(buffer.as_slice());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxRotary {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxRotary001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Modulation];
}

nih_export_vst3!(GfxRotary);
