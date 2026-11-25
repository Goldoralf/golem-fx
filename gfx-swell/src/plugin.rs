// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second, each with the key on its own
//! auxiliary bus (the main layout stays 1 / 1, as the Golem Engine requires). Linear ranges only (golem
//! contract). `bypass` is the VST3 bypass parameter: engaging starts Closed, so a ringing chord swells
//! in. The latency is reported only with `lookahead` (setup only). `state`, `gr`, `onsets_count` are
//! read-only.

use crate::{Swell, NP, PARAMS};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

gfx_dsp::sync_param_enum!();

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum CurveP {
    #[id = "square"]
    #[name = "Square"]
    Square,
    #[id = "linear"]
    #[name = "Linear"]
    Linear,
    #[id = "s-curve"]
    #[name = "S-Curve"]
    SCurve,
    #[id = "db"]
    #[name = "dB"]
    Db,
    #[id = "pedal"]
    #[name = "Pedal"]
    Pedal,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SourceP {
    #[id = "main"]
    #[name = "Main"]
    Main,
    #[id = "key"]
    #[name = "Key"]
    Key,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum LookaheadP {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "3"]
    #[name = "3 ms"]
    Ms3,
    #[id = "10"]
    #[name = "10 ms"]
    Ms10,
    #[id = "25"]
    #[name = "25 ms"]
    Ms25,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

#[derive(Params)]
struct SwellParams {
    #[id = "rise"]
    rise: FloatParam,
    #[id = "sync"]
    sync: EnumParam<SyncParam>,
    #[id = "sens"]
    sens: FloatParam,
    #[id = "legato"]
    legato: FloatParam,
    #[id = "curve"]
    curve: EnumParam<CurveP>,
    #[id = "depth"]
    depth: FloatParam,
    #[id = "dip"]
    dip: FloatParam,
    #[id = "strum"]
    strum: FloatParam,
    #[id = "decay"]
    decay: FloatParam,
    #[id = "reset"]
    reset: FloatParam,
    #[id = "release"]
    release: FloatParam,
    #[id = "thresh"]
    thresh: FloatParam,
    #[id = "source"]
    source: EnumParam<SourceP>,
    #[id = "lookahead"]
    lookahead: EnumParam<LookaheadP>,
    #[id = "fire"]
    fire: BoolParam,
    #[id = "level"]
    level: FloatParam,
    #[id = "bypass"]
    bypass: BoolParam,
    /// Read-only meters (golem contract): 0 Idle · 1 Rise · 2 Hold · 3 Decay; attenuation in dB (≥ 0);
    /// swells started since reset (debug witness).
    #[id = "state"]
    state: FloatParam,
    #[id = "gr"]
    gr: FloatParam,
    #[id = "onsets_count"]
    onsets_count: FloatParam,
}

impl Default for SwellParams {
    fn default() -> Self {
        Self {
            rise: f(0, " ms", 0),
            sync: EnumParam::new("Sync", SyncParam::Off),
            sens: f(2, " %", 0),
            legato: f(3, " %", 0),
            curve: EnumParam::new("Curve", CurveP::Square),
            depth: f(5, " %", 0),
            dip: f(6, " %", 0),
            strum: f(7, " ms", 0),
            decay: f(8, " ms", 0),
            reset: f(9, " dB", 1),
            release: f(10, " ms", 0),
            thresh: f(11, " dB", 1),
            source: EnumParam::new("Detector Source", SourceP::Main),
            lookahead: EnumParam::new("Lookahead", LookaheadP::Off),
            fire: BoolParam::new("Fire", false),
            level: f(15, " dB", 1),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            state: FloatParam::new("Swell State", 0.0, FloatRange::Linear { min: 0.0, max: 3.0 }).read_only(),
            gr: FloatParam::new("Gain Reduction", 0.0, FloatRange::Linear { min: 0.0, max: 90.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1))
                .read_only(),
            onsets_count: FloatParam::new("Onsets Count", 0.0, FloatRange::Linear { min: 0.0, max: 1.0e9 }).read_only(),
        }
    }
}

impl SwellParams {
    fn values(&self) -> [f32; NP] {
        [
            self.rise.value(),
            self.sync.value() as usize as f32,
            self.sens.value(),
            self.legato.value(),
            self.curve.value() as usize as f32,
            self.depth.value(),
            self.dip.value(),
            self.strum.value(),
            self.decay.value(),
            self.reset.value(),
            self.release.value(),
            self.thresh.value(),
            self.source.value() as usize as f32,
            self.lookahead.value() as usize as f32,
            if self.fire.value() { 1.0 } else { 0.0 },
            self.level.value(),
            if self.bypass.value() { 1.0 } else { 0.0 },
        ]
    }
}

pub struct GfxSwell {
    params: Arc<SwellParams>,
    dsp: Swell,
    last: [f32; NP],
    reported: u32,
}

impl Default for GfxSwell {
    fn default() -> Self {
        Self { params: Arc::new(SwellParams::default()), dsp: Swell::new(), last: [f32::NAN; NP], reported: 0 }
    }
}

impl GfxSwell {
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

impl Plugin for GfxSwell {
    const NAME: &'static str = "GoleM FX Swell";
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
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-swell"))
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
        let l = self.dsp.latency() as u32;
        if l != self.reported {
            self.reported = l;
            ctx.set_latency_samples(l);
        }
        self.params.state.set_output_value(self.dsp.state() as u32 as f32);
        self.params.gr.set_output_value(self.dsp.gr_db());
        self.params.onsets_count.set_output_value(self.dsp.onsets_count() as f32);
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxSwell {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxSwell0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_vst3!(GfxSwell);
