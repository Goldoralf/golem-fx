// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first (the organ is a source, first in a mono lane; the
//! Leslie's mics summed), stereo 2 / 2 second (the mono sum is played, the Leslie stereo). Linear ranges
//! only (golem contract). `bypass` is the VST3 bypass parameter: with `trails` the organ releases over
//! `release` and the rotors coast. Zero latency. Read-only meters `state` and `roots`. Generated from
//! PARAMS (same order and ids; the drawbars, `perc*`, `vibrato` and `leslie` are golem `organ`'s ids and
//! option ids).

use crate::{Organ, NP, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum RegisterP {
    #[id = "tutti"]
    #[name = "Tutti"]
    VTutti,
    #[id = "trio"]
    #[name = "Trio"]
    VTrio,
    #[id = "choir"]
    #[name = "Choir"]
    VChoir,
    #[id = "bark"]
    #[name = "Bark"]
    VBark,
    #[id = "nave"]
    #[name = "Nave"]
    VNave,
    #[id = "chime"]
    #[name = "Chime"]
    VChime,
    #[id = "velvet"]
    #[name = "Velvet"]
    VVelvet,
    #[id = "combo"]
    #[name = "Combo"]
    VCombo,
    #[id = "coupler"]
    #[name = "Coupler"]
    VCoupler,
    #[id = "custom"]
    #[name = "Custom"]
    VCustom,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum RotorP {
    #[id = "stop"]
    #[name = "Stop"]
    VStop,
    #[id = "slow"]
    #[name = "Slow"]
    VSlow,
    #[id = "fast"]
    #[name = "Fast"]
    VFast,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum PercHarmP {
    #[id = "2nd"]
    #[name = "2nd"]
    V2Nd,
    #[id = "3rd"]
    #[name = "3rd"]
    V3Rd,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum PercDecayP {
    #[id = "slow"]
    #[name = "Slow"]
    VSlow,
    #[id = "fast"]
    #[name = "Fast"]
    VFast,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum PercVolP {
    #[id = "normal"]
    #[name = "Normal"]
    VNormal,
    #[id = "soft"]
    #[name = "Soft"]
    VSoft,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum PercTrigP {
    #[id = "pick"]
    #[name = "Pick"]
    VPick,
    #[id = "single"]
    #[name = "Single"]
    VSingle,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum VibratoP {
    #[id = "off"]
    #[name = "Off"]
    VOff,
    #[id = "V1"]
    #[name = "V1"]
    VV1,
    #[id = "C1"]
    #[name = "C1"]
    VC1,
    #[id = "V2"]
    #[name = "V2"]
    VV2,
    #[id = "C2"]
    #[name = "C2"]
    VC2,
    #[id = "V3"]
    #[name = "V3"]
    VV3,
    #[id = "C3"]
    #[name = "C3"]
    VC3,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SpeakerP {
    #[id = "off"]
    #[name = "Off"]
    VOff,
    #[id = "122"]
    #[name = "122"]
    V122,
    #[id = "147"]
    #[name = "147"]
    V147,
    #[id = "145"]
    #[name = "145"]
    V145,
    #[id = "vibratone"]
    #[name = "Vibratone"]
    VVibratone,
    #[id = "122rear"]
    #[name = "122 Rear"]
    V122Rear,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum TuningP {
    #[id = "tonewheel"]
    #[name = "Tonewheel"]
    VTonewheel,
    #[id = "pure"]
    #[name = "Pure"]
    VPure,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum BankP {
    #[id = "dual"]
    #[name = "Dual"]
    VDual,
    #[id = "fast"]
    #[name = "Fast"]
    VFast,
    #[id = "clean"]
    #[name = "Clean"]
    VClean,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

fn i(i: usize) -> IntParam {
    let pi = PARAMS[i];
    IntParam::new(pi.name, pi.default as i32, IntRange::Linear { min: pi.min as i32, max: pi.max as i32 })
}

#[derive(Params)]
struct OrganParams {
    #[id = "register"]
    register: EnumParam<RegisterP>,
    #[id = "organ"]
    organ: FloatParam,
    #[id = "dry"]
    dry: FloatParam,
    #[id = "mod"]
    modulation: FloatParam,
    #[id = "click"]
    click: FloatParam,
    #[id = "leslie"]
    leslie: EnumParam<RotorP>,
    #[id = "db16"]
    db16: IntParam,
    #[id = "db513"]
    db513: IntParam,
    #[id = "db8"]
    db8: IntParam,
    #[id = "db4"]
    db4: IntParam,
    #[id = "db223"]
    db223: IntParam,
    #[id = "db2"]
    db2: IntParam,
    #[id = "db135"]
    db135: IntParam,
    #[id = "db113"]
    db113: IntParam,
    #[id = "db1"]
    db1: IntParam,
    #[id = "perc"]
    perc: BoolParam,
    #[id = "perc_harm"]
    perc_harm: EnumParam<PercHarmP>,
    #[id = "perc_decay"]
    perc_decay: EnumParam<PercDecayP>,
    #[id = "perc_vol"]
    perc_vol: EnumParam<PercVolP>,
    #[id = "perc_trig"]
    perc_trig: EnumParam<PercTrigP>,
    #[id = "vibrato"]
    vibrato: EnumParam<VibratoP>,
    #[id = "speaker"]
    speaker: EnumParam<SpeakerP>,
    #[id = "tuning"]
    tuning: EnumParam<TuningP>,
    #[id = "drive"]
    drive: FloatParam,
    #[id = "ramp"]
    ramp: FloatParam,
    #[id = "spacing"]
    spacing: FloatParam,
    #[id = "hold"]
    hold: FloatParam,
    #[id = "touch"]
    touch: FloatParam,
    #[id = "strings"]
    strings: FloatParam,
    #[id = "swell"]
    swell: FloatParam,
    #[id = "release"]
    release: FloatParam,
    #[id = "bank"]
    bank: EnumParam<BankP>,
    #[id = "lowest"]
    lowest: FloatParam,
    #[id = "thresh"]
    thresh: FloatParam,
    #[id = "bypass"]
    bypass: BoolParam,
    #[id = "trails"]
    trails: BoolParam,
    /// Read-only meters (golem contract): 0 silence · 1 attack (undecided partials only) · 2 organ (roots
    /// sounding); the confirmed roots (0 – 12).
    #[id = "state"]
    state: FloatParam,
    #[id = "roots"]
    roots: FloatParam,
}

impl Default for OrganParams {
    fn default() -> Self {
        Self {
            register: EnumParam::new(PARAMS[0].name, RegisterP::VTrio),
            organ: f(1, " dB", 1),
            dry: f(2, " dB", 1),
            modulation: f(3, " %", 0),
            click: f(4, " %", 0),
            leslie: EnumParam::new(PARAMS[5].name, RotorP::VSlow),
            db16: i(6),
            db513: i(7),
            db8: i(8),
            db4: i(9),
            db223: i(10),
            db2: i(11),
            db135: i(12),
            db113: i(13),
            db1: i(14),
            perc: BoolParam::new(PARAMS[15].name, PARAMS[15].default >= 0.5),
            perc_harm: EnumParam::new(PARAMS[16].name, PercHarmP::V3Rd),
            perc_decay: EnumParam::new(PARAMS[17].name, PercDecayP::VFast),
            perc_vol: EnumParam::new(PARAMS[18].name, PercVolP::VNormal),
            perc_trig: EnumParam::new(PARAMS[19].name, PercTrigP::VPick),
            vibrato: EnumParam::new(PARAMS[20].name, VibratoP::VOff),
            speaker: EnumParam::new(PARAMS[21].name, SpeakerP::V122),
            tuning: EnumParam::new(PARAMS[22].name, TuningP::VTonewheel),
            drive: f(23, " %", 0),
            ramp: f(24, " %", 0),
            spacing: f(25, " %", 0),
            hold: f(26, " %", 0),
            touch: f(27, " %", 0),
            strings: f(28, " %", 0),
            swell: f(29, " ms", 0),
            release: f(30, " ms", 0),
            bank: EnumParam::new(PARAMS[31].name, BankP::VDual),
            lowest: f(32, " Hz", 0),
            thresh: f(33, " dB", 1),
            bypass: BoolParam::new(PARAMS[34].name, PARAMS[34].default >= 0.5).make_bypass(),
            trails: BoolParam::new(PARAMS[35].name, PARAMS[35].default >= 0.5),
            state: FloatParam::new("Organ State", 0.0, FloatRange::Linear { min: 0.0, max: 2.0 }).read_only(),
            roots: FloatParam::new("Roots", 0.0, FloatRange::Linear { min: 0.0, max: 12.0 }).read_only(),
        }
    }
}

impl OrganParams {
    fn values(&self) -> [f32; NP] {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        [
            self.register.value() as usize as f32,
            self.organ.value(),
            self.dry.value(),
            self.modulation.value(),
            self.click.value(),
            self.leslie.value() as usize as f32,
            self.db16.value() as f32,
            self.db513.value() as f32,
            self.db8.value() as f32,
            self.db4.value() as f32,
            self.db223.value() as f32,
            self.db2.value() as f32,
            self.db135.value() as f32,
            self.db113.value() as f32,
            self.db1.value() as f32,
            b(self.perc.value()),
            self.perc_harm.value() as usize as f32,
            self.perc_decay.value() as usize as f32,
            self.perc_vol.value() as usize as f32,
            self.perc_trig.value() as usize as f32,
            self.vibrato.value() as usize as f32,
            self.speaker.value() as usize as f32,
            self.tuning.value() as usize as f32,
            self.drive.value(),
            self.ramp.value(),
            self.spacing.value(),
            self.hold.value(),
            self.touch.value(),
            self.strings.value(),
            self.swell.value(),
            self.release.value(),
            self.bank.value() as usize as f32,
            self.lowest.value(),
            self.thresh.value(),
            b(self.bypass.value()),
            b(self.trails.value()),
        ]
    }
}

pub struct GfxOrgan {
    params: Arc<OrganParams>,
    dsp: Organ,
    last: [f32; NP],
}

impl Default for GfxOrgan {
    fn default() -> Self {
        Self { params: Arc::new(OrganParams::default()), dsp: Organ::new(), last: [f32::NAN; NP] }
    }
}

impl GfxOrgan {
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

impl Plugin for GfxOrgan {
    const NAME: &'static str = "GoleM FX Organ";
    const VENDOR: &'static str = "GoleM";
    const URL: &'static str = "https://golem.fm";
    const EMAIL: &'static str = "contact@golem.fm";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout { main_input_channels: NonZeroU32::new(1), main_output_channels: NonZeroU32::new(1), ..AudioIOLayout::const_default() },
        AudioIOLayout { main_input_channels: NonZeroU32::new(2), main_output_channels: NonZeroU32::new(2), ..AudioIOLayout::const_default() },
    ];

    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    #[cfg(feature = "gui")]
    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-organ"))
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

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.sync(false);
        self.dsp.process(buffer.as_slice());
        self.params.state.set_output_value(self.dsp.state());
        self.params.roots.set_output_value(self.dsp.roots());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxOrgan {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxOrgan0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Synth];
}

nih_export_vst3!(GfxOrgan);
