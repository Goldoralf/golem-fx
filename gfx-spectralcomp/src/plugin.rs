// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first (a block that may sit before the amp must not make
//! the lane stereo), stereo 2 / 2 second (linked detection), each with the key on its own auxiliary
//! bus (1 or 2 channels; two are summed to mono). Linear ranges only (golem contract). `bypass` is
//! the VST3 bypass parameter (the dry, still delayed in Studio). The latency is reported for Studio
//! (N − 1, set-up switches `mode` and `window`). `gr`, `lift_gr` and `dyn_*` are read-only.

use crate::{SpectralComp, NP, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModeP {
    #[id = "live"]
    #[name = "Live"]
    Live,
    #[id = "studio"]
    #[name = "Studio"]
    Studio,
}

/// Samples at 48 kHz (× 2 at 88.2 / 96 kHz, × 4 at 176.4 / 192 kHz). The catalogue's order, 512
/// appended (append only).
#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum WindowP {
    #[id = "1024"]
    #[name = "1024"]
    W1024,
    #[id = "2048"]
    #[name = "2048"]
    W2048,
    #[id = "4096"]
    #[name = "4096"]
    W4096,
    #[id = "512"]
    #[name = "512"]
    W512,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SourceP {
    #[id = "main"]
    Main,
    #[id = "key"]
    Key,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

fn meter(name: &str, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, 0.0, FloatRange::Linear { min, max })
        .with_unit(" dB")
        .with_value_to_string(formatters::v2s_f32_rounded(1))
        .read_only()
}

/// Ids: golem's catalogue's (`spectral-comp`) for `thresh`, `down`, `up`, `center`, `slope`,
/// `attack`, `release`, `window`, `mix`, `out`; gfx-mbcomp's for `knee`, `range`, `lift_range`,
/// `guard`; gfx-gate's for `source`.
#[derive(Params)]
struct SpectralParams {
    #[id = "thresh"]
    thresh: FloatParam,
    #[id = "down"]
    down: FloatParam,
    #[id = "up"]
    up: FloatParam,
    #[id = "slope"]
    slope: FloatParam,
    #[id = "attack"]
    attack: FloatParam,
    #[id = "mode"]
    mode: EnumParam<ModeP>,
    #[id = "center"]
    center: FloatParam,
    #[id = "release"]
    release: FloatParam,
    #[id = "window"]
    window: EnumParam<WindowP>,
    #[id = "res"]
    res: FloatParam,
    #[id = "knee"]
    knee: FloatParam,
    #[id = "range"]
    range: FloatParam,
    #[id = "lift_range"]
    lift_range: FloatParam,
    #[id = "guard"]
    guard: FloatParam,
    #[id = "source"]
    source: EnumParam<SourceP>,
    #[id = "auto_gain"]
    auto_gain: BoolParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "out"]
    out: FloatParam,
    #[id = "link"]
    link: FloatParam,
    #[id = "delta"]
    delta: BoolParam,
    #[id = "bypass"]
    bypass: BoolParam,
    /// Read-only meters (golem contract): the largest octave cut and lift (dB, ≥ 0) and the signed
    /// gain change of each octave 63 Hz – 8 kHz, held 50 ms.
    #[id = "gr"]
    gr: FloatParam,
    #[id = "lift_gr"]
    lift_gr: FloatParam,
    #[id = "dyn_63"]
    dyn_63: FloatParam,
    #[id = "dyn_125"]
    dyn_125: FloatParam,
    #[id = "dyn_250"]
    dyn_250: FloatParam,
    #[id = "dyn_500"]
    dyn_500: FloatParam,
    #[id = "dyn_1k"]
    dyn_1k: FloatParam,
    #[id = "dyn_2k"]
    dyn_2k: FloatParam,
    #[id = "dyn_4k"]
    dyn_4k: FloatParam,
    #[id = "dyn_8k"]
    dyn_8k: FloatParam,
}

impl Default for SpectralParams {
    fn default() -> Self {
        Self {
            thresh: f(0, " dBFS", 1),
            down: f(1, ":1", 2),
            up: f(2, ":1", 2),
            slope: f(3, " dB/oct", 1),
            attack: f(4, " ms", 1),
            mode: EnumParam::new("Mode", ModeP::Live),
            center: f(6, " Hz", 0),
            release: f(7, " ms", 0),
            window: EnumParam::new("Window", WindowP::W1024),
            res: f(9, " st", 1),
            knee: f(10, " dB", 1),
            range: f(11, " dB", 1),
            lift_range: f(12, " dB", 1),
            guard: f(13, " dBFS", 1),
            source: EnumParam::new("Source", SourceP::Main),
            auto_gain: BoolParam::new("Auto Gain", true),
            mix: f(16, " %", 0),
            out: f(17, " dB", 1),
            link: f(18, " %", 0),
            delta: BoolParam::new("Delta", false),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            gr: meter("Gain Reduction", 0.0, 60.0),
            lift_gr: meter("Lift", 0.0, 24.0),
            dyn_63: meter("63 Hz", -60.0, 24.0),
            dyn_125: meter("125 Hz", -60.0, 24.0),
            dyn_250: meter("250 Hz", -60.0, 24.0),
            dyn_500: meter("500 Hz", -60.0, 24.0),
            dyn_1k: meter("1 kHz", -60.0, 24.0),
            dyn_2k: meter("2 kHz", -60.0, 24.0),
            dyn_4k: meter("4 kHz", -60.0, 24.0),
            dyn_8k: meter("8 kHz", -60.0, 24.0),
        }
    }
}

impl SpectralParams {
    fn values(&self) -> [f32; NP] {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        [
            self.thresh.value(),
            self.down.value(),
            self.up.value(),
            self.slope.value(),
            self.attack.value(),
            self.mode.value() as usize as f32,
            self.center.value(),
            self.release.value(),
            self.window.value() as usize as f32,
            self.res.value(),
            self.knee.value(),
            self.range.value(),
            self.lift_range.value(),
            self.guard.value(),
            self.source.value() as usize as f32,
            b(self.auto_gain.value()),
            self.mix.value(),
            self.out.value(),
            self.link.value(),
            b(self.delta.value()),
            b(self.bypass.value()),
        ]
    }
}

pub struct GfxSpectralComp {
    params: Arc<SpectralParams>,
    dsp: SpectralComp,
    last: [f32; NP],
    reported: u32,
}

impl Default for GfxSpectralComp {
    fn default() -> Self {
        Self { params: Arc::new(SpectralParams::default()), dsp: SpectralComp::new(), last: [f32::NAN; NP], reported: 0 }
    }
}

impl GfxSpectralComp {
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

impl Plugin for GfxSpectralComp {
    const NAME: &'static str = "GoleM FX Spectral Comp";
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

    fn initialize(&mut self, layout: &AudioIOLayout, config: &BufferConfig, ctx: &mut impl InitContext<Self>) -> bool {
        let channels = layout.main_output_channels.map_or(1, |c| c.get() as usize);
        self.sync(true);
        // the FFT tables and the Live plan are built here (a few ms), never on the audio thread
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
        self.dsp.reset();
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
        // `set_latency_samples` only when the value changes (`mode` / `window`: set-up only)
        let l = self.dsp.latency() as u32;
        if l != self.reported {
            self.reported = l;
            ctx.set_latency_samples(l);
        }
        self.params.gr.set_output_value(self.dsp.gr_db().clamp(0.0, 60.0));
        self.params.lift_gr.set_output_value(self.dsp.lift_db().clamp(0.0, 24.0));
        let d = self.dsp.dyn_db();
        let p = &self.params;
        for (m, v) in [&p.dyn_63, &p.dyn_125, &p.dyn_250, &p.dyn_500, &p.dyn_1k, &p.dyn_2k, &p.dyn_4k, &p.dyn_8k].into_iter().zip(d) {
            m.set_output_value(v.clamp(-60.0, 24.0));
        }
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxSpectralComp {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxSpecCmp01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_vst3!(GfxSpectralComp);
