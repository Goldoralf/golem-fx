// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first (a ducker is not stereo by nature, and a block before
//! the amp must not make the lane stereo), stereo 2 / 2 second, each with the key on its own auxiliary
//! bus (1 or 2 channels). Linear ranges only (golem contract). `bypass` is the VST3 bypass parameter.
//! The latency is reported only with `lookahead` (setup only). `gr` and `state` are read-only.

use crate::{Ducker, NP, PARAMS};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

gfx_dsp::sync_param_enum!();

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModeP {
    #[id = "follow"]
    #[name = "Follow"]
    Follow,
    #[id = "trigger"]
    #[name = "Trigger"]
    Trigger,
    #[id = "pump"]
    #[name = "Pump"]
    Pump,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ShapeP {
    #[id = "wide"]
    #[name = "Wide"]
    Wide,
    #[id = "bell"]
    #[name = "Bell"]
    Bell,
    #[id = "low"]
    #[name = "Low"]
    Low,
    #[id = "high"]
    #[name = "High"]
    High,
    #[id = "spectral"]
    #[name = "Spectral"]
    Spectral,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum StereoP {
    #[id = "both"]
    #[name = "Both"]
    Both,
    #[id = "mid"]
    #[name = "Mid"]
    Mid,
    #[id = "side"]
    #[name = "Side"]
    Side,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum LookaheadP {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "2"]
    #[name = "2 ms"]
    Ms2,
    #[id = "5"]
    #[name = "5 ms"]
    Ms5,
    #[id = "10"]
    #[name = "10 ms"]
    Ms10,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

#[derive(Params)]
struct DuckerParams {
    #[id = "mode"]
    mode: EnumParam<ModeP>,
    #[id = "thresh"]
    thresh: FloatParam,
    #[id = "depth"]
    depth: FloatParam,
    #[id = "attack"]
    attack: FloatParam,
    #[id = "hold"]
    hold: FloatParam,
    #[id = "release"]
    release: FloatParam,
    #[id = "shape"]
    shape: EnumParam<ShapeP>,
    #[id = "hpf"]
    hpf: FloatParam,
    #[id = "lpf"]
    lpf: FloatParam,
    #[id = "knee"]
    knee: FloatParam,
    #[id = "hyst"]
    hyst: FloatParam,
    #[id = "freq"]
    freq: FloatParam,
    #[id = "width"]
    width: FloatParam,
    #[id = "spread"]
    spread: FloatParam,
    #[id = "sync"]
    sync: EnumParam<SyncParam>,
    #[id = "length"]
    length: FloatParam,
    #[id = "tension"]
    tension: FloatParam,
    #[id = "offset"]
    offset: FloatParam,
    #[id = "sens"]
    sens: FloatParam,
    #[id = "stereo"]
    stereo: EnumParam<StereoP>,
    #[id = "lookahead"]
    lookahead: EnumParam<LookaheadP>,
    #[id = "force"]
    force: BoolParam,
    #[id = "listen"]
    listen: BoolParam,
    #[id = "bypass"]
    bypass: BoolParam,
    /// Read-only meters (golem contract): the current duck (dB, ≥ 0) and "ducking" (1 while > 1 dB).
    #[id = "gr"]
    gr: FloatParam,
    #[id = "state"]
    state: FloatParam,
}

impl Default for DuckerParams {
    fn default() -> Self {
        Self {
            mode: EnumParam::new("Mode", ModeP::Follow),
            thresh: f(1, " dB", 1),
            depth: f(2, " dB", 1),
            attack: f(3, " ms", 1),
            hold: f(4, " ms", 0),
            release: f(5, " ms", 0),
            shape: EnumParam::new("Band", ShapeP::Wide),
            hpf: f(7, " Hz", 0),
            lpf: f(8, " Hz", 0),
            knee: f(9, " dB", 1),
            hyst: f(10, " dB", 1),
            freq: f(11, " Hz", 0),
            width: f(12, " oct", 2),
            spread: f(13, " dB", 1),
            sync: EnumParam::new("Sync", SyncParam::D4),
            length: f(15, " %", 0),
            tension: f(16, " %", 0),
            offset: f(17, " %", 0),
            sens: f(18, " %", 0),
            stereo: EnumParam::new("Stereo", StereoP::Both),
            lookahead: EnumParam::new("Lookahead", LookaheadP::Off),
            force: BoolParam::new("Duck Now", false),
            listen: BoolParam::new("Listen", false),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            gr: FloatParam::new("Duck", 0.0, FloatRange::Linear { min: 0.0, max: 60.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1))
                .read_only(),
            state: FloatParam::new("Ducking", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 }).read_only(),
        }
    }
}

impl DuckerParams {
    fn values(&self) -> [f32; NP] {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        [
            self.mode.value() as usize as f32,
            self.thresh.value(),
            self.depth.value(),
            self.attack.value(),
            self.hold.value(),
            self.release.value(),
            self.shape.value() as usize as f32,
            self.hpf.value(),
            self.lpf.value(),
            self.knee.value(),
            self.hyst.value(),
            self.freq.value(),
            self.width.value(),
            self.spread.value(),
            self.sync.value() as usize as f32,
            self.length.value(),
            self.tension.value(),
            self.offset.value(),
            self.sens.value(),
            self.stereo.value() as usize as f32,
            self.lookahead.value() as usize as f32,
            b(self.force.value()),
            b(self.listen.value()),
            b(self.bypass.value()),
        ]
    }
}

pub struct GfxDucker {
    params: Arc<DuckerParams>,
    dsp: Ducker,
    last: [f32; NP],
    reported: u32,
}

impl Default for GfxDucker {
    fn default() -> Self {
        Self { params: Arc::new(DuckerParams::default()), dsp: Ducker::new(), last: [f32::NAN; NP], reported: 0 }
    }
}

impl GfxDucker {
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

impl Plugin for GfxDucker {
    const NAME: &'static str = "GoleM FX Ducker";
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

    #[cfg(feature = "gui")]
    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-ducker"))
    }

    fn initialize(&mut self, layout: &AudioIOLayout, config: &BufferConfig, ctx: &mut impl InitContext<Self>) -> bool {
        let channels = layout.main_output_channels.map_or(1, |c| c.get() as usize);
        self.sync(true);
        // the Spectral tables are built here (a few ms), never on the audio thread
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
        self.reported = self.dsp.latency() as u32;
        ctx.set_latency_samples(self.reported);
        true
    }

    fn reset(&mut self) {
        self.sync(true);
        self.dsp.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.sync(false);
        let t = ctx.transport();
        self.dsp.set_transport(TransportInfo { tempo: t.tempo, pos_beats: t.pos_beats(), playing: t.playing });
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
        // `set_latency_samples` only when the value changes (setup only)
        let l = self.dsp.latency() as u32;
        if l != self.reported {
            self.reported = l;
            ctx.set_latency_samples(l);
        }
        self.params.gr.set_output_value(self.dsp.gr_db());
        self.params.state.set_output_value(if self.dsp.is_ducking() { 1.0 } else { 0.0 });
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxDucker {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxDucker001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_vst3!(GfxDucker);
