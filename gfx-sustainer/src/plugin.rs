// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second (the voice runs on the mono sum
//! and is added to both channels: not stereo by nature), each with the key on its own auxiliary bus
//! (`source` Key: the clean DI when the block sits after an amp). Linear ranges only (golem contract).
//! `bypass` is the VST3 bypass parameter: with `trails` the bloom releases over `release`. Zero
//! latency. Read-only meters `state`, `note`, `harm`, `lock`. Generated from PARAMS (same order and
//! ids).

use crate::{Sustainer, NP, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum HarmonicP {
    #[id = "sub"]
    #[name = "Sub"]
    Sub,
    #[id = "fundamental"]
    #[name = "Fundamental"]
    Fundamental,
    #[id = "octave"]
    #[name = "Octave"]
    Octave,
    #[id = "octave-fifth"]
    #[name = "Octave + Fifth"]
    OctaveFifth,
    #[id = "two-octaves"]
    #[name = "Two Octaves"]
    TwoOctaves,
    #[id = "two-octaves-third"]
    #[name = "Two Octaves + Third"]
    TwoOctavesThird,
    #[id = "natural-low"]
    #[name = "Natural Low"]
    NaturalLow,
    #[id = "natural-high"]
    #[name = "Natural High"]
    NaturalHigh,
    #[id = "climb"]
    #[name = "Climb"]
    Climb,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModeP {
    #[id = "feedback"]
    #[name = "Feedback"]
    Feedback,
    #[id = "sustain"]
    #[name = "Sustain"]
    Sustain,
    #[id = "both"]
    #[name = "Both"]
    Both,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum TriggerP {
    #[id = "auto"]
    #[name = "Auto"]
    Auto,
    #[id = "switch"]
    #[name = "Switch"]
    Switch,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ChordP {
    #[id = "lowest"]
    #[name = "Lowest"]
    Lowest,
    #[id = "highest"]
    #[name = "Highest"]
    Highest,
    #[id = "strongest"]
    #[name = "Strongest"]
    Strongest,
    #[id = "all"]
    #[name = "All"]
    All,
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

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

#[derive(Params)]
struct SustainerParams {
    #[id = "harmonic"]
    harmonic: EnumParam<HarmonicP>,
    #[id = "delay"]
    delay: FloatParam,
    #[id = "rise"]
    rise: FloatParam,
    #[id = "bloom"]
    bloom: FloatParam,
    #[id = "dry"]
    dry: FloatParam,
    #[id = "mode"]
    mode: EnumParam<ModeP>,
    #[id = "trigger"]
    trigger: EnumParam<TriggerP>,
    #[id = "feed"]
    feed: BoolParam,
    #[id = "distance"]
    distance: FloatParam,
    #[id = "drift"]
    drift: FloatParam,
    #[id = "chord"]
    chord: EnumParam<ChordP>,
    #[id = "carry"]
    carry: BoolParam,
    #[id = "release"]
    release: FloatParam,
    #[id = "mute_sens"]
    mute_sens: FloatParam,
    #[id = "max_time"]
    max_time: FloatParam,
    #[id = "duck"]
    duck: FloatParam,
    #[id = "color"]
    color: FloatParam,
    #[id = "tone"]
    tone: FloatParam,
    #[id = "lowest"]
    lowest: FloatParam,
    #[id = "thresh"]
    thresh: FloatParam,
    #[id = "source"]
    source: EnumParam<SourceP>,
    #[id = "bypass"]
    bypass: BoolParam,
    #[id = "trails"]
    trails: BoolParam,
    /// Read-only meters (golem contract): 0 Idle · 1 Wait · 2 Rise · 3 Hold · 4 Release of the loudest
    /// voice; its tracked f0 (Hz, 0 = none); the sounding mode (0.5, 1 … 6; 1 in Sustain); 1 = the
    /// string is followed, 0 = its pitch is held.
    #[id = "state"]
    state: FloatParam,
    #[id = "note"]
    note: FloatParam,
    #[id = "harm"]
    harm: FloatParam,
    #[id = "lock"]
    lock: FloatParam,
}

impl Default for SustainerParams {
    fn default() -> Self {
        Self {
            harmonic: EnumParam::new("Harmonic", HarmonicP::Octave),
            delay: f(1, " ms", 0),
            rise: f(2, " ms", 0),
            bloom: f(3, " dB", 1),
            dry: f(4, " dB", 1),
            mode: EnumParam::new("Mode", ModeP::Feedback),
            trigger: EnumParam::new("Trigger", TriggerP::Auto),
            feed: BoolParam::new("Feed", false),
            distance: f(8, " m", 2),
            drift: f(9, " %", 0),
            chord: EnumParam::new("Chord", ChordP::Strongest),
            carry: BoolParam::new("Legato Carry", true),
            release: f(12, " ms", 0),
            mute_sens: f(13, " %", 0),
            max_time: f(14, " s", 1),
            duck: f(15, " %", 0),
            color: f(16, " %", 0),
            tone: f(17, " %", 0),
            lowest: f(18, " Hz", 0),
            thresh: f(19, " dB", 1),
            source: EnumParam::new("Detector Source", SourceP::Main),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            trails: BoolParam::new("Trails", true),
            state: FloatParam::new("Note State", 0.0, FloatRange::Linear { min: 0.0, max: 4.0 }).read_only(),
            note: FloatParam::new("Note", 0.0, FloatRange::Linear { min: 0.0, max: 2000.0 })
                .with_unit(" Hz")
                .with_value_to_string(formatters::v2s_f32_rounded(1))
                .read_only(),
            harm: FloatParam::new("Sounding Harmonic", 0.0, FloatRange::Linear { min: 0.0, max: 6.0 }).read_only(),
            lock: FloatParam::new("Lock", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 }).read_only(),
        }
    }
}

impl SustainerParams {
    fn values(&self) -> [f32; NP] {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        [
            self.harmonic.value() as usize as f32,
            self.delay.value(),
            self.rise.value(),
            self.bloom.value(),
            self.dry.value(),
            self.mode.value() as usize as f32,
            self.trigger.value() as usize as f32,
            b(self.feed.value()),
            self.distance.value(),
            self.drift.value(),
            self.chord.value() as usize as f32,
            b(self.carry.value()),
            self.release.value(),
            self.mute_sens.value(),
            self.max_time.value(),
            self.duck.value(),
            self.color.value(),
            self.tone.value(),
            self.lowest.value(),
            self.thresh.value(),
            self.source.value() as usize as f32,
            b(self.bypass.value()),
            b(self.trails.value()),
        ]
    }
}

pub struct GfxSustainer {
    params: Arc<SustainerParams>,
    dsp: Sustainer,
    last: [f32; NP],
}

impl Default for GfxSustainer {
    fn default() -> Self {
        Self { params: Arc::new(SustainerParams::default()), dsp: Sustainer::new(), last: [f32::NAN; NP] }
    }
}

impl GfxSustainer {
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

impl Plugin for GfxSustainer {
    const NAME: &'static str = "GoleM FX Sustainer";
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
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
        ctx.set_latency_samples(0);
        true
    }

    fn reset(&mut self) {
        self.sync(true);
        self.dsp.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, aux: &mut AuxiliaryBuffers, _ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
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
        self.params.state.set_output_value(self.dsp.state() as u8 as f32);
        self.params.note.set_output_value(self.dsp.note_hz());
        self.params.harm.set_output_value(self.dsp.harm());
        self.params.lock.set_output_value(self.dsp.lock());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxSustainer {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxSustain01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::PitchShift];
}

nih_export_vst3!(GfxSustainer);
