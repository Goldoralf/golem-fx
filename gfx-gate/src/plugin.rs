// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second, each with the key on its own
//! auxiliary bus (the main layout stays 1 / 1, as the Golem Engine requires).

use crate::{Gate, Settings};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum Hum {
    #[id = "off"]
    Off,
    #[id = "50"]
    #[name = "50 Hz"]
    Hz50,
    #[id = "60"]
    #[name = "60 Hz"]
    Hz60,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum Source {
    #[id = "main"]
    Main,
    #[id = "key"]
    Key,
}

/// Linear ranges only: golem writes REAPER's normalized value as (v − min) / (max − min)
/// (golem contract §Parameters), so any skew would land on the wrong value.
fn lin(min: f32, max: f32) -> FloatRange {
    FloatRange::Linear { min, max }
}

fn ms(name: &str, default: f32, min: f32, max: f32, _centre: f32) -> FloatParam {
    FloatParam::new(name, default, lin(min, max))
        .with_unit(" ms")
        .with_value_to_string(formatters::v2s_f32_rounded(1))
}

fn db(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min, max })
        .with_unit(" dB")
        .with_value_to_string(formatters::v2s_f32_rounded(1))
}

fn hz(name: &str, default: f32, min: f32, max: f32, _centre: f32) -> FloatParam {
    FloatParam::new(name, default, lin(min, max))
        .with_unit(" Hz")
        .with_value_to_string(formatters::v2s_f32_rounded(0))
}

/// Ids and units are golem's catalogue's (`gate` block) for the first seven.
#[derive(Params)]
struct GateParams {
    #[id = "thresh"]
    thresh: FloatParam,
    #[id = "attack"]
    attack: FloatParam,
    #[id = "hold"]
    hold: FloatParam,
    #[id = "release"]
    release: FloatParam,
    #[id = "hpf"]
    hpf: FloatParam,
    #[id = "lpf"]
    lpf: FloatParam,
    #[id = "hyst"]
    hyst: FloatParam,
    #[id = "range"]
    range: FloatParam,
    #[id = "ratio"]
    ratio: FloatParam,
    #[id = "knee"]
    knee: FloatParam,
    #[id = "follow"]
    follow: BoolParam,
    #[id = "damp"]
    damp: FloatParam,
    #[id = "hum"]
    hum: EnumParam<Hum>,
    #[id = "source"]
    source: EnumParam<Source>,
    #[id = "listen"]
    listen: BoolParam,
}

impl Default for GateParams {
    fn default() -> Self {
        Self {
            thresh: db("Threshold", -50.0, -90.0, 0.0),
            attack: ms("Attack", 3.0, 0.0, 100.0, 5.0),
            hold: ms("Hold", 20.0, 0.0, 1000.0, 50.0),
            release: ms("Release", 150.0, 5.0, 2000.0, 150.0),
            hpf: hz("Sidechain Low Cut", 0.0, 0.0, 2000.0, 120.0),
            lpf: hz("Sidechain High Cut", 20000.0, 500.0, 22000.0, 5000.0),
            hyst: db("Hysteresis", 0.0, -24.0, 0.0),
            range: db("Range", -90.0, -90.0, 0.0),
            ratio: FloatParam::new("Ratio", 100.0, lin(1.0, 100.0))
                .with_value_to_string(Arc::new(|v| if v >= 99.5 { "inf".into() } else { format!("{v:.1}:1") })),
            knee: db("Knee", 0.0, 0.0, 24.0),
            follow: BoolParam::new("Follow Decay", false),
            damp: FloatParam::new("Tail Damp", 0.0, FloatRange::Linear { min: 0.0, max: 100.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_rounded(0)),
            hum: EnumParam::new("Hum Filter", Hum::Off),
            source: EnumParam::new("Detector Source", Source::Main),
            listen: BoolParam::new("Listen", false),
        }
    }
}

impl GateParams {
    fn settings(&self) -> Settings {
        Settings {
            thresh: self.thresh.value(),
            attack: self.attack.value(),
            hold: self.hold.value(),
            release: self.release.value(),
            hpf: self.hpf.value(),
            lpf: self.lpf.value(),
            hyst: self.hyst.value(),
            range: self.range.value(),
            ratio: self.ratio.value(),
            knee: self.knee.value(),
            follow: self.follow.value(),
            damp: self.damp.value(),
            hum: match self.hum.value() {
                Hum::Off => 0,
                Hum::Hz50 => 1,
                Hum::Hz60 => 2,
            },
            source: match self.source.value() {
                Source::Main => 0,
                Source::Key => 1,
            },
            listen: self.listen.value(),
        }
    }
}

pub struct GfxGate {
    params: Arc<GateParams>,
    dsp: Gate,
}

impl Default for GfxGate {
    fn default() -> Self {
        Self { params: Arc::new(GateParams::default()), dsp: Gate::new() }
    }
}

fn same(a: &Settings, b: &Settings) -> bool {
    a.thresh == b.thresh
        && a.attack == b.attack
        && a.hold == b.hold
        && a.release == b.release
        && a.hpf == b.hpf
        && a.lpf == b.lpf
        && a.hyst == b.hyst
        && a.range == b.range
        && a.ratio == b.ratio
        && a.knee == b.knee
        && a.follow == b.follow
        && a.damp == b.damp
        && a.hum == b.hum
        && a.source == b.source
        && a.listen == b.listen
}

impl Plugin for GfxGate {
    const NAME: &'static str = "GoleM FX Gate";
    const VENDOR: &'static str = "GoleM";
    const URL: &'static str = "https://golem.fm";
    const EMAIL: &'static str = "contact@golem.fm";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            aux_input_ports: &[new_nonzero_u32(1)],
            names: PortNames { aux_inputs: &["Key"], ..PortNames::const_default() },
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[new_nonzero_u32(2)],
            names: PortNames { aux_inputs: &["Key"], ..PortNames::const_default() },
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

    fn process(&mut self, buffer: &mut Buffer, aux: &mut AuxiliaryBuffers, _ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let s = self.params.settings();
        if !same(&s, &self.dsp.settings()) {
            self.dsp.set(s);
        }
        let key = aux.inputs.first().map(|b| b.as_slice_immutable());
        match key {
            Some(k) if !k.is_empty() => {
                let k0: &[f32] = &k[0][..];
                let k1: &[f32] = &k[k.len().min(2) - 1][..];
                let keys = [k0, k1];
                self.dsp.process_with_key(buffer.as_slice(), Some(&keys[..k.len().min(2)]));
            }
            _ => self.dsp.process_with_key(buffer.as_slice(), None),
        }
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxGate {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxGate00001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_vst3!(GfxGate);
