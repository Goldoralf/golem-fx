// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second (two identical chains with linked
//! keying and losses, the room's two outputs). Linear ranges only (golem contract). `bypass` is the VST3
//! bypass parameter, with trails (the room and an open squelch tail ring out). `state` is a read-only
//! meter (1 = keyed / squelch open). Reported latency 0. Generated from PARAMS (same order and ids).

use crate::{Radio, NP, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModelP {
    #[id = "telephone"]
    #[name = "Telephone"]
    Telephone,
    #[id = "cell-phone"]
    #[name = "Cell Phone"]
    CellPhone,
    #[id = "megaphone"]
    #[name = "Megaphone"]
    Megaphone,
    #[id = "walkie-talkie"]
    #[name = "Walkie-Talkie"]
    WalkieTalkie,
    #[id = "am-radio"]
    #[name = "AM Radio"]
    AmRadio,
    #[id = "pa-horn"]
    #[name = "PA Horn"]
    PaHorn,
    #[id = "intercom"]
    #[name = "Intercom"]
    Intercom,
    #[id = "pocket-radio"]
    #[name = "Pocket Radio"]
    PocketRadio,
    #[id = "gramophone"]
    #[name = "Gramophone"]
    Gramophone,
    #[id = "laptop"]
    #[name = "Laptop"]
    Laptop,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum MicP {
    #[id = "model"]
    #[name = "Model"]
    Model,
    #[id = "clean"]
    #[name = "Clean"]
    Clean,
    #[id = "carbon"]
    #[name = "Carbon"]
    Carbon,
    #[id = "dynamic"]
    #[name = "Dynamic"]
    Dynamic,
    #[id = "crystal"]
    #[name = "Crystal"]
    Crystal,
    #[id = "electret"]
    #[name = "Electret"]
    Electret,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ChannelP {
    #[id = "model"]
    #[name = "Model"]
    Model,
    #[id = "none"]
    #[name = "None"]
    None,
    #[id = "phone-line"]
    #[name = "Phone Line"]
    PhoneLine,
    #[id = "digital-line"]
    #[name = "Digital Line"]
    DigitalLine,
    #[id = "cell-codec"]
    #[name = "Cell Codec"]
    CellCodec,
    #[id = "am"]
    #[name = "AM"]
    Am,
    #[id = "nbfm"]
    #[name = "NBFM"]
    Nbfm,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SpeakerP {
    #[id = "model"]
    #[name = "Model"]
    Model,
    #[id = "none"]
    #[name = "None"]
    None,
    #[id = "handset"]
    #[name = "Handset"]
    Handset,
    #[id = "megaphone-horn"]
    #[name = "Megaphone Horn"]
    MegaphoneHorn,
    #[id = "walkie-speaker"]
    #[name = "Walkie Speaker"]
    WalkieSpeaker,
    #[id = "table-radio"]
    #[name = "Table Radio"]
    TableRadio,
    #[id = "pa-horn"]
    #[name = "PA Horn"]
    PaHorn,
    #[id = "intercom"]
    #[name = "Intercom"]
    Intercom,
    #[id = "pocket-radio"]
    #[name = "Pocket Radio"]
    PocketRadio,
    #[id = "gramophone-horn"]
    #[name = "Gramophone Horn"]
    GramophoneHorn,
    #[id = "laptop"]
    #[name = "Laptop"]
    Laptop,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum RoomP {
    #[id = "model"]
    #[name = "Model"]
    Model,
    #[id = "none"]
    #[name = "None"]
    None,
    #[id = "small-room"]
    #[name = "Small Room"]
    SmallRoom,
    #[id = "hall"]
    #[name = "Hall"]
    Hall,
    #[id = "street"]
    #[name = "Street"]
    Street,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum MainsP {
    #[id = "50-hz"]
    #[name = "50 Hz"]
    Hz50Hz,
    #[id = "60-hz"]
    #[name = "60 Hz"]
    Hz60Hz,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

#[derive(Params)]
struct RadioParams {
    #[id = "model"]
    model: EnumParam<ModelP>,
    #[id = "drive"]
    drive: FloatParam,
    #[id = "noise"]
    noise: FloatParam,
    #[id = "distance"]
    distance: FloatParam,
    #[id = "tune"]
    tune: FloatParam,
    #[id = "squelch"]
    squelch: FloatParam,
    #[id = "mic"]
    mic: EnumParam<MicP>,
    #[id = "channel"]
    channel: EnumParam<ChannelP>,
    #[id = "speaker"]
    speaker: EnumParam<SpeakerP>,
    #[id = "room"]
    room: EnumParam<RoomP>,
    #[id = "band"]
    band: FloatParam,
    #[id = "resonance"]
    resonance: FloatParam,
    #[id = "codec"]
    codec: FloatParam,
    #[id = "dropouts"]
    dropouts: FloatParam,
    #[id = "whistle"]
    whistle: FloatParam,
    #[id = "squelch_tail"]
    squelch_tail: FloatParam,
    #[id = "roger"]
    roger: BoolParam,
    #[id = "battery"]
    battery: FloatParam,
    #[id = "rattle"]
    rattle: FloatParam,
    #[id = "mains"]
    mains: EnumParam<MainsP>,
    #[id = "level"]
    level: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "bypass"]
    bypass: BoolParam,
    /// Read-only meter (golem contract): channel keyed / squelch open, 0 / 1.
    #[id = "state"]
    state: FloatParam,
}

impl Default for RadioParams {
    fn default() -> Self {
        Self {
            model: EnumParam::new("Model", ModelP::Telephone),
            drive: f(1, " %", 0),
            noise: f(2, " %", 0),
            distance: f(3, " %", 0),
            tune: f(4, " %", 0),
            squelch: f(5, " %", 0),
            mic: EnumParam::new("Mic", MicP::Model),
            channel: EnumParam::new("Channel", ChannelP::Model),
            speaker: EnumParam::new("Speaker", SpeakerP::Model),
            room: EnumParam::new("Room", RoomP::Model),
            band: f(10, " %", 0),
            resonance: f(11, " %", 0),
            codec: f(12, " %", 0),
            dropouts: f(13, " %", 0),
            whistle: f(14, " dB", 1),
            squelch_tail: f(15, " %", 0),
            roger: BoolParam::new("Roger", false),
            battery: f(17, " %", 0),
            rattle: f(18, " %", 0),
            mains: EnumParam::new("Mains", MainsP::Hz50Hz),
            level: f(20, " dB", 1),
            mix: f(21, " %", 0),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            state: FloatParam::new("Keyed", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 }).read_only(),
        }
    }
}

impl RadioParams {
    fn values(&self) -> [f32; NP] {
        [
            self.model.value() as usize as f32,
            self.drive.value(),
            self.noise.value(),
            self.distance.value(),
            self.tune.value(),
            self.squelch.value(),
            self.mic.value() as usize as f32,
            self.channel.value() as usize as f32,
            self.speaker.value() as usize as f32,
            self.room.value() as usize as f32,
            self.band.value(),
            self.resonance.value(),
            self.codec.value(),
            self.dropouts.value(),
            self.whistle.value(),
            self.squelch_tail.value(),
            if self.roger.value() { 1.0 } else { 0.0 },
            self.battery.value(),
            self.rattle.value(),
            self.mains.value() as usize as f32,
            self.level.value(),
            self.mix.value(),
            if self.bypass.value() { 1.0 } else { 0.0 },
        ]
    }
}

pub struct GfxRadio {
    params: Arc<RadioParams>,
    dsp: Radio,
    last: [f32; NP],
}

impl Default for GfxRadio {
    fn default() -> Self {
        Self { params: Arc::new(RadioParams::default()), dsp: Radio::new(), last: [f32::NAN; NP] }
    }
}

impl GfxRadio {
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

impl Plugin for GfxRadio {
    const NAME: &'static str = "GoleM FX Radio";
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

    fn initialize(&mut self, layout: &AudioIOLayout, config: &BufferConfig, _ctx: &mut impl InitContext<Self>) -> bool {
        let channels = layout.main_output_channels.map_or(1, |c| c.get() as usize);
        self.sync(true);
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
        true
    }

    fn reset(&mut self) {
        self.sync(true);
        self.dsp.reset();
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.sync(false);
        self.dsp.process(buffer.as_slice());
        self.params.state.set_output_value(self.dsp.meter("state").unwrap_or(0.0));
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxRadio {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxRadio0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_vst3!(GfxRadio);
