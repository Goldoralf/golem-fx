// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2. Linear ranges only (golem contract).

use crate::{Character, Comp, Settings};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum CharacterParam {
    #[id = "clean"]
    Clean,
    #[id = "bus"]
    Bus,
    #[id = "fet"]
    #[name = "FET"]
    Fet,
    #[id = "opto"]
    Opto,
    #[id = "pedal"]
    Pedal,
}

fn lin(name: &str, default: f32, min: f32, max: f32, unit: &'static str, decimals: usize) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min, max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// The first five ids are golem's catalogue's (`comp` block), in its units.
#[derive(Params)]
struct CompParams {
    #[id = "thresh"]
    thresh: FloatParam,
    #[id = "ratio"]
    ratio: FloatParam,
    #[id = "makeup"]
    makeup: FloatParam,
    #[id = "attack"]
    attack: FloatParam,
    #[id = "release"]
    release: FloatParam,
    #[id = "character"]
    character: EnumParam<CharacterParam>,
    #[id = "knee"]
    knee: FloatParam,
    #[id = "auto_release"]
    auto_release: BoolParam,
    #[id = "sc_hpf"]
    sc_hpf: FloatParam,
    #[id = "range"]
    range: FloatParam,
    #[id = "drive"]
    drive: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "link"]
    link: FloatParam,
    #[id = "sc_listen"]
    sc_listen: BoolParam,
    /// Read-only meter: gain reduction (dB, ≥ 0).
    #[id = "gr"]
    gr: FloatParam,
}

impl Default for CompParams {
    fn default() -> Self {
        Self {
            thresh: lin("Threshold", -18.0, -60.0, 0.0, " dB", 1),
            ratio: lin("Ratio", 4.0, 1.0, 30.0, ":1", 1),
            makeup: lin("Makeup", 3.0, -12.0, 24.0, " dB", 1),
            attack: lin("Attack", 15.0, 0.02, 200.0, " ms", 2),
            release: lin("Release", 150.0, 5.0, 3000.0, " ms", 0),
            character: EnumParam::new("Character", CharacterParam::Clean),
            knee: lin("Knee", 3.0, 0.0, 24.0, " dB", 1),
            auto_release: BoolParam::new("Auto Release", true),
            sc_hpf: lin("Sidechain Low Cut", 0.0, 0.0, 500.0, " Hz", 0),
            range: lin("Range", 60.0, 0.0, 60.0, " dB", 1),
            drive: lin("Drive", 0.0, 0.0, 100.0, " %", 0),
            mix: lin("Mix", 100.0, 0.0, 100.0, " %", 0),
            link: lin("Stereo Link", 100.0, 0.0, 100.0, " %", 0),
            sc_listen: BoolParam::new("Sidechain Listen", false),
            gr: lin("Gain Reduction", 0.0, 0.0, 60.0, " dB", 1).read_only(),
        }
    }
}

impl CompParams {
    fn settings(&self) -> Settings {
        Settings {
            thresh: self.thresh.value(),
            ratio: self.ratio.value(),
            makeup: self.makeup.value(),
            attack: self.attack.value(),
            release: self.release.value(),
            character: match self.character.value() {
                CharacterParam::Clean => Character::Clean,
                CharacterParam::Bus => Character::Bus,
                CharacterParam::Fet => Character::Fet,
                CharacterParam::Opto => Character::Opto,
                CharacterParam::Pedal => Character::Pedal,
            },
            knee: self.knee.value(),
            auto_release: self.auto_release.value(),
            sc_hpf: self.sc_hpf.value(),
            range: self.range.value(),
            drive: self.drive.value(),
            mix: self.mix.value(),
            link: self.link.value(),
            sc_listen: self.sc_listen.value(),
        }
    }
}

fn same(a: &Settings, b: &Settings) -> bool {
    a.thresh == b.thresh
        && a.ratio == b.ratio
        && a.makeup == b.makeup
        && a.attack == b.attack
        && a.release == b.release
        && a.character == b.character
        && a.knee == b.knee
        && a.auto_release == b.auto_release
        && a.sc_hpf == b.sc_hpf
        && a.range == b.range
        && a.drive == b.drive
        && a.mix == b.mix
        && a.link == b.link
        && a.sc_listen == b.sc_listen
}

pub struct GfxComp {
    params: Arc<CompParams>,
    dsp: Comp,
}

impl Default for GfxComp {
    fn default() -> Self {
        Self { params: Arc::new(CompParams::default()), dsp: Comp::new() }
    }
}

impl Plugin for GfxComp {
    const NAME: &'static str = "GoleM FX Comp";
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
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-comp"))
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
        if !same(&s, &self.dsp.settings()) {
            self.dsp.set(s);
        }
        self.dsp.process(buffer.as_slice());
        self.params.gr.set_output_value(self.dsp.gain_reduction_db());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxComp {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxComp00001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_vst3!(GfxComp);
