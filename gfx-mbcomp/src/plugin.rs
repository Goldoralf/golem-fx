// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 second. Linear ranges only (golem
//! contract). `bypass` is the VST3 bypass parameter (a 10 ms crossfade to the input). The latency is
//! reported per `phase`: 0 in Minimum, L in Linear (2,048 / 4,096 / 8,192 samples per rate family; a
//! set-up switch). Read-only meters `gr` and `<b>_dyn` through the fork's output parameters.
//! Generated from PARAMS (same order and ids).

use crate::{MbComp, NP, PARAMS};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum CharacterP {
    #[id = "clean"]
    #[name = "Clean"]
    Clean,
    #[id = "bus"]
    #[name = "Bus"]
    Bus,
    #[id = "fet"]
    #[name = "FET"]
    Fet,
    #[id = "opto"]
    #[name = "Opto"]
    Opto,
    #[id = "pedal"]
    #[name = "Pedal"]
    Pedal,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SidechainP {
    #[id = "band"]
    #[name = "Band"]
    Band,
    #[id = "wide"]
    #[name = "Wide"]
    Wide,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum BandsP {
    #[id = "3"]
    #[name = "3"]
    B3,
    #[id = "4"]
    #[name = "4"]
    B4,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SlopeP {
    #[id = "24"]
    #[name = "24 dB/oct"]
    S24,
    #[id = "48"]
    #[name = "48 dB/oct"]
    S48,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum PhaseP {
    #[id = "minimum"]
    #[name = "Minimum"]
    Minimum,
    #[id = "linear"]
    #[name = "Linear"]
    Linear,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum SoloP {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "low"]
    #[name = "Low"]
    Low,
    #[id = "low-mid"]
    #[name = "Low Mid"]
    LowMid,
    #[id = "high-mid"]
    #[name = "High Mid"]
    HighMid,
    #[id = "high"]
    #[name = "High"]
    High,
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

#[derive(Params)]
struct MbCompParams {
    #[id = "amount"]
    amount: FloatParam,
    #[id = "tilt"]
    tilt: FloatParam,
    #[id = "time"]
    time: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "level"]
    level: FloatParam,
    #[id = "character"]
    character: EnumParam<CharacterP>,
    #[id = "lo_thresh"]
    lo_thresh: FloatParam,
    #[id = "lo_ratio"]
    lo_ratio: FloatParam,
    #[id = "lo_lift_thresh"]
    lo_lift_thresh: FloatParam,
    #[id = "lo_lift"]
    lo_lift: FloatParam,
    #[id = "lo_attack"]
    lo_attack: FloatParam,
    #[id = "lo_release"]
    lo_release: FloatParam,
    #[id = "lo_gain"]
    lo_gain: FloatParam,
    #[id = "lo_sc"]
    lo_sc: EnumParam<SidechainP>,
    #[id = "lm_thresh"]
    lm_thresh: FloatParam,
    #[id = "lm_ratio"]
    lm_ratio: FloatParam,
    #[id = "lm_lift_thresh"]
    lm_lift_thresh: FloatParam,
    #[id = "lm_lift"]
    lm_lift: FloatParam,
    #[id = "lm_attack"]
    lm_attack: FloatParam,
    #[id = "lm_release"]
    lm_release: FloatParam,
    #[id = "lm_gain"]
    lm_gain: FloatParam,
    #[id = "lm_sc"]
    lm_sc: EnumParam<SidechainP>,
    #[id = "hm_thresh"]
    hm_thresh: FloatParam,
    #[id = "hm_ratio"]
    hm_ratio: FloatParam,
    #[id = "hm_lift_thresh"]
    hm_lift_thresh: FloatParam,
    #[id = "hm_lift"]
    hm_lift: FloatParam,
    #[id = "hm_attack"]
    hm_attack: FloatParam,
    #[id = "hm_release"]
    hm_release: FloatParam,
    #[id = "hm_gain"]
    hm_gain: FloatParam,
    #[id = "hm_sc"]
    hm_sc: EnumParam<SidechainP>,
    #[id = "hi_thresh"]
    hi_thresh: FloatParam,
    #[id = "hi_ratio"]
    hi_ratio: FloatParam,
    #[id = "hi_lift_thresh"]
    hi_lift_thresh: FloatParam,
    #[id = "hi_lift"]
    hi_lift: FloatParam,
    #[id = "hi_attack"]
    hi_attack: FloatParam,
    #[id = "hi_release"]
    hi_release: FloatParam,
    #[id = "hi_gain"]
    hi_gain: FloatParam,
    #[id = "hi_sc"]
    hi_sc: EnumParam<SidechainP>,
    #[id = "bands"]
    bands: EnumParam<BandsP>,
    #[id = "x_lo"]
    x_lo: FloatParam,
    #[id = "x_mid"]
    x_mid: FloatParam,
    #[id = "x_hi"]
    x_hi: FloatParam,
    #[id = "slope"]
    slope: EnumParam<SlopeP>,
    #[id = "phase"]
    phase: EnumParam<PhaseP>,
    #[id = "auto_gain"]
    auto_gain: BoolParam,
    #[id = "auto_release"]
    auto_release: BoolParam,
    #[id = "knee"]
    knee: FloatParam,
    #[id = "range"]
    range: FloatParam,
    #[id = "lift_range"]
    lift_range: FloatParam,
    #[id = "guard"]
    guard: FloatParam,
    #[id = "link"]
    link: FloatParam,
    #[id = "solo"]
    solo: EnumParam<SoloP>,
    #[id = "delta"]
    delta: BoolParam,
    #[id = "bypass"]
    bypass: BoolParam,
    /// Read-only meters (golem contract): the largest band reduction (dB, ≥ 0), and each band's
    /// dynamic gain (dB, signed: negative = reduction, positive = Lift's boost).
    #[id = "gr"]
    gr: FloatParam,
    #[id = "lo_dyn"]
    lo_dyn: FloatParam,
    #[id = "lm_dyn"]
    lm_dyn: FloatParam,
    #[id = "hm_dyn"]
    hm_dyn: FloatParam,
    #[id = "hi_dyn"]
    hi_dyn: FloatParam,
}

impl Default for MbCompParams {
    fn default() -> Self {
        Self {
            amount: f(0, " %", 0),
            tilt: f(1, " %", 0),
            time: f(2, " %", 0),
            mix: f(3, " %", 0),
            level: f(4, " dB", 1),
            character: EnumParam::new(PARAMS[5].name, CharacterP::Clean),
            lo_thresh: f(6, " dB", 1),
            lo_ratio: f(7, ":1", 1),
            lo_lift_thresh: f(8, " dB", 1),
            lo_lift: f(9, " %", 0),
            lo_attack: f(10, " ms", 1),
            lo_release: f(11, " ms", 0),
            lo_gain: f(12, " dB", 1),
            lo_sc: EnumParam::new(PARAMS[13].name, SidechainP::Band),
            lm_thresh: f(14, " dB", 1),
            lm_ratio: f(15, ":1", 1),
            lm_lift_thresh: f(16, " dB", 1),
            lm_lift: f(17, " %", 0),
            lm_attack: f(18, " ms", 1),
            lm_release: f(19, " ms", 0),
            lm_gain: f(20, " dB", 1),
            lm_sc: EnumParam::new(PARAMS[21].name, SidechainP::Band),
            hm_thresh: f(22, " dB", 1),
            hm_ratio: f(23, ":1", 1),
            hm_lift_thresh: f(24, " dB", 1),
            hm_lift: f(25, " %", 0),
            hm_attack: f(26, " ms", 1),
            hm_release: f(27, " ms", 0),
            hm_gain: f(28, " dB", 1),
            hm_sc: EnumParam::new(PARAMS[29].name, SidechainP::Band),
            hi_thresh: f(30, " dB", 1),
            hi_ratio: f(31, ":1", 1),
            hi_lift_thresh: f(32, " dB", 1),
            hi_lift: f(33, " %", 0),
            hi_attack: f(34, " ms", 1),
            hi_release: f(35, " ms", 0),
            hi_gain: f(36, " dB", 1),
            hi_sc: EnumParam::new(PARAMS[37].name, SidechainP::Band),
            bands: EnumParam::new(PARAMS[38].name, BandsP::B3),
            x_lo: f(39, " Hz", 0),
            x_mid: f(40, " Hz", 0),
            x_hi: f(41, " Hz", 0),
            slope: EnumParam::new(PARAMS[42].name, SlopeP::S24),
            phase: EnumParam::new(PARAMS[43].name, PhaseP::Minimum),
            auto_gain: BoolParam::new(PARAMS[44].name, true),
            auto_release: BoolParam::new(PARAMS[45].name, true),
            knee: f(46, " dB", 1),
            range: f(47, " dB", 1),
            lift_range: f(48, " dB", 1),
            guard: f(49, " dBFS", 0),
            link: f(50, " %", 0),
            solo: EnumParam::new(PARAMS[51].name, SoloP::Off),
            delta: BoolParam::new(PARAMS[52].name, false),
            bypass: BoolParam::new(PARAMS[53].name, false).make_bypass(),
            gr: meter("Gain Reduction", 0.0, 60.0),
            lo_dyn: meter("Low Dynamics", -60.0, 24.0),
            lm_dyn: meter("Low Mid Dynamics", -60.0, 24.0),
            hm_dyn: meter("High Mid Dynamics", -60.0, 24.0),
            hi_dyn: meter("High Dynamics", -60.0, 24.0),
        }
    }
}

impl MbCompParams {
    fn values(&self) -> [f32; NP] {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        [
            self.amount.value(),
            self.tilt.value(),
            self.time.value(),
            self.mix.value(),
            self.level.value(),
            self.character.value() as usize as f32,
            self.lo_thresh.value(),
            self.lo_ratio.value(),
            self.lo_lift_thresh.value(),
            self.lo_lift.value(),
            self.lo_attack.value(),
            self.lo_release.value(),
            self.lo_gain.value(),
            self.lo_sc.value() as usize as f32,
            self.lm_thresh.value(),
            self.lm_ratio.value(),
            self.lm_lift_thresh.value(),
            self.lm_lift.value(),
            self.lm_attack.value(),
            self.lm_release.value(),
            self.lm_gain.value(),
            self.lm_sc.value() as usize as f32,
            self.hm_thresh.value(),
            self.hm_ratio.value(),
            self.hm_lift_thresh.value(),
            self.hm_lift.value(),
            self.hm_attack.value(),
            self.hm_release.value(),
            self.hm_gain.value(),
            self.hm_sc.value() as usize as f32,
            self.hi_thresh.value(),
            self.hi_ratio.value(),
            self.hi_lift_thresh.value(),
            self.hi_lift.value(),
            self.hi_attack.value(),
            self.hi_release.value(),
            self.hi_gain.value(),
            self.hi_sc.value() as usize as f32,
            self.bands.value() as usize as f32,
            self.x_lo.value(),
            self.x_mid.value(),
            self.x_hi.value(),
            self.slope.value() as usize as f32,
            self.phase.value() as usize as f32,
            b(self.auto_gain.value()),
            b(self.auto_release.value()),
            self.knee.value(),
            self.range.value(),
            self.lift_range.value(),
            self.guard.value(),
            self.link.value(),
            self.solo.value() as usize as f32,
            b(self.delta.value()),
            b(self.bypass.value()),
        ]
    }
}

pub struct GfxMbComp {
    params: Arc<MbCompParams>,
    dsp: MbComp,
    last: [f32; NP],
    reported: u32,
}

impl Default for GfxMbComp {
    fn default() -> Self {
        Self { params: Arc::new(MbCompParams::default()), dsp: MbComp::new(), last: [f32::NAN; NP], reported: 0 }
    }
}

impl GfxMbComp {
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

impl Plugin for GfxMbComp {
    const NAME: &'static str = "GoleM FX MB Comp";
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
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-mbcomp"))
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

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.sync(false);
        self.dsp.process(buffer.as_slice());
        let l = self.dsp.latency() as u32;
        if l != self.reported {
            self.reported = l;
            ctx.set_latency_samples(l);
        }
        let m = self.dsp.meters();
        self.params.gr.set_output_value(m.gr.clamp(0.0, 60.0));
        for (p, v) in [&self.params.lo_dyn, &self.params.lm_dyn, &self.params.hm_dyn, &self.params.hi_dyn].into_iter().zip(m.dyn_db) {
            p.set_output_value(v.clamp(-60.0, 24.0));
        }
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxMbComp {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxMbComp001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_vst3!(GfxMbComp);
