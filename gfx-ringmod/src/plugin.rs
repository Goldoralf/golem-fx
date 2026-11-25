// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3): mono 1 / 1 first, stereo 2 / 2 (not stereo by nature: the right
//! carrier is offset by `stereo_phase` only). Linear ranges only (golem contract): the ten catalogue
//! ids stay on their 0 – 1 scale and **display their real value** (README §3). Read-only meters
//! `carrier_hz` and `state`. Zero latency. Generated from PARAMS (same order and ids).

use crate::{cut_hz, free_hz, pitch_st, speed_tau, thr_db, RingMod, NP, PARAMS};
use gfx_dsp::transport::TransportInfo;
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

gfx_dsp::sync_param_enum!();

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum CarrierP {
    #[id = "free"]
    #[name = "Free"]
    Free,
    #[id = "track"]
    #[name = "Track"]
    Track,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModeP {
    #[id = "ring"]
    #[name = "Ring"]
    Ring,
    #[id = "upper"]
    #[name = "Upper"]
    Upper,
    #[id = "lower"]
    #[name = "Lower"]
    Lower,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ModelP {
    #[id = "clean"]
    #[name = "Clean"]
    Clean,
    #[id = "diode"]
    #[name = "Diode"]
    Diode,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum WaveP {
    #[id = "sine"]
    #[name = "Sine"]
    Sine,
    #[id = "triangle"]
    #[name = "Triangle"]
    Triangle,
    #[id = "square"]
    #[name = "Square"]
    Square,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum TuningP {
    #[id = "equal"]
    #[name = "Equal"]
    Equal,
    #[id = "just"]
    #[name = "Just"]
    Just,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
enum ShapeP {
    #[id = "sine"]
    #[name = "Sine"]
    Sine,
    #[id = "triangle"]
    #[name = "Triangle"]
    Triangle,
    #[id = "square"]
    #[name = "Square"]
    Square,
    #[id = "random"]
    #[name = "Random"]
    Random,
}

fn f(i: usize, unit: &'static str, decimals: usize) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max })
        .with_unit(unit)
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

/// A catalogue id on its 0 – 1 scale (stored with `decimals`), displayed through its law.
fn law(i: usize, _decimals: usize, show: fn(f64) -> String) -> FloatParam {
    let pi = PARAMS[i];
    FloatParam::new(pi.name, pi.default, FloatRange::Linear { min: pi.min, max: pi.max }).with_value_to_string(Arc::new(move |v| show(v as f64)))
}

fn hz(v: f64) -> String {
    if v >= 1000.0 {
        let r = v.round() as i64;
        format!("{},{:03} Hz", r / 1000, r % 1000)
    } else if v >= 100.0 {
        format!("{v:.0} Hz")
    } else {
        format!("{v:.1} Hz")
    }
}

fn hz_khz(v: f64) -> String {
    if v >= 1000.0 {
        format!("{:.2} kHz", v / 1000.0)
    } else {
        format!("{v:.0} Hz")
    }
}

/// `freq`, `fine`, `feedback` are the `ringmod` block's ids; `thr`, `speed`, `lowcut`, `highcut`,
/// `pitch`, `ring`, `mix` the `surge-treemonster` block's.
#[derive(Params)]
struct RingModParams {
    #[id = "carrier"]
    carrier: EnumParam<CarrierP>,
    #[id = "freq"]
    freq: FloatParam,
    #[id = "fine"]
    fine: FloatParam,
    #[id = "pitch"]
    pitch: FloatParam,
    #[id = "ring"]
    ring: FloatParam,
    #[id = "feedback"]
    feedback: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "thr"]
    thr: FloatParam,
    #[id = "speed"]
    speed: FloatParam,
    #[id = "lowcut"]
    lowcut: FloatParam,
    #[id = "highcut"]
    highcut: FloatParam,
    #[id = "mode"]
    mode: EnumParam<ModeP>,
    #[id = "model"]
    model: EnumParam<ModelP>,
    #[id = "wave"]
    wave: EnumParam<WaveP>,
    #[id = "tuning"]
    tuning: EnumParam<TuningP>,
    #[id = "lock"]
    lock: BoolParam,
    #[id = "snap"]
    snap: BoolParam,
    #[id = "phase"]
    phase: FloatParam,
    #[id = "drive"]
    drive: FloatParam,
    #[id = "carrier_level"]
    carrier_level: FloatParam,
    #[id = "balance"]
    balance: FloatParam,
    #[id = "rate"]
    rate: FloatParam,
    #[id = "lfo_depth"]
    lfo_depth: FloatParam,
    #[id = "shape"]
    shape: EnumParam<ShapeP>,
    #[id = "sync"]
    sync: EnumParam<SyncParam>,
    #[id = "stereo_phase"]
    stereo_phase: FloatParam,
    #[id = "env_amount"]
    env_amount: FloatParam,
    #[id = "env_sens"]
    env_sens: FloatParam,
    #[id = "env_att"]
    env_att: FloatParam,
    #[id = "env_rel"]
    env_rel: FloatParam,
    #[id = "hpf"]
    hpf: FloatParam,
    #[id = "lpf"]
    lpf: FloatParam,
    #[id = "bass_keep"]
    bass_keep: FloatParam,
    #[id = "level"]
    level: FloatParam,
    /// Read-only meters (golem contract): the carrier's frequency now; 0 free or holding, 1 tracking,
    /// 2 locked.
    #[id = "carrier_hz"]
    carrier_hz: FloatParam,
    #[id = "state"]
    state: FloatParam,
}

impl Default for RingModParams {
    fn default() -> Self {
        Self {
            carrier: EnumParam::new("Carrier", CarrierP::Free),
            freq: law(1, 3, |v| hz(free_hz(v, 0.0))),
            fine: law(2, 3, |v| format!("+{:.1} Hz", 100.0 * v)),
            pitch: law(3, 3, |v| format!("{:+.2} st", pitch_st(v))),
            ring: law(4, 3, |v| format!("{v:.2}")),
            feedback: law(5, 3, |v| format!("{:.3} loop", 0.95 * v)),
            mix: law(6, 3, |v| format!("{v:.2}")),
            thr: law(7, 3, |v| format!("{:.1} dB", thr_db(v))),
            speed: law(8, 3, |v| format!("{:.0} ms", 1000.0 * speed_tau(v))),
            lowcut: law(9, 3, |v| hz_khz(cut_hz(v))),
            highcut: law(10, 3, |v| hz_khz(cut_hz(v))),
            mode: EnumParam::new("Mode", ModeP::Ring),
            model: EnumParam::new("Model", ModelP::Clean),
            wave: EnumParam::new("Carrier Wave", WaveP::Sine),
            tuning: EnumParam::new("Tuning", TuningP::Just),
            lock: BoolParam::new("Phase Lock", true),
            snap: BoolParam::new("Snap", false),
            phase: f(17, "°", 0),
            drive: f(18, " %", 0),
            carrier_level: f(19, " dB", 1),
            balance: f(20, " %", 0),
            rate: f(21, " Hz", 2),
            lfo_depth: f(22, " st", 1),
            shape: EnumParam::new("LFO Shape", ShapeP::Sine),
            sync: EnumParam::new("LFO Sync", SyncParam::Off),
            stereo_phase: f(25, "°", 0),
            env_amount: f(26, " st", 1),
            env_sens: f(27, " %", 0),
            env_att: f(28, " ms", 0),
            env_rel: f(29, " ms", 0),
            hpf: f(30, " Hz", 0),
            lpf: f(31, " Hz", 0),
            bass_keep: f(32, " Hz", 0),
            level: f(33, " dB", 1),
            carrier_hz: FloatParam::new("Carrier Hz", 0.0, FloatRange::Linear { min: 0.0, max: 20000.0 })
                .with_unit(" Hz")
                .with_value_to_string(formatters::v2s_f32_rounded(1))
                .read_only(),
            state: FloatParam::new("Track State", 0.0, FloatRange::Linear { min: 0.0, max: 2.0 }).read_only(),
        }
    }
}

impl RingModParams {
    fn values(&self) -> [f32; NP] {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        [
            self.carrier.value() as usize as f32,
            self.freq.value(),
            self.fine.value(),
            self.pitch.value(),
            self.ring.value(),
            self.feedback.value(),
            self.mix.value(),
            self.thr.value(),
            self.speed.value(),
            self.lowcut.value(),
            self.highcut.value(),
            self.mode.value() as usize as f32,
            self.model.value() as usize as f32,
            self.wave.value() as usize as f32,
            self.tuning.value() as usize as f32,
            b(self.lock.value()),
            b(self.snap.value()),
            self.phase.value(),
            self.drive.value(),
            self.carrier_level.value(),
            self.balance.value(),
            self.rate.value(),
            self.lfo_depth.value(),
            self.shape.value() as usize as f32,
            self.sync.value() as usize as f32,
            self.stereo_phase.value(),
            self.env_amount.value(),
            self.env_sens.value(),
            self.env_att.value(),
            self.env_rel.value(),
            self.hpf.value(),
            self.lpf.value(),
            self.bass_keep.value(),
            self.level.value(),
        ]
    }
}

pub struct GfxRingMod {
    params: Arc<RingModParams>,
    dsp: RingMod,
    last: [f32; NP],
}

impl Default for GfxRingMod {
    fn default() -> Self {
        Self { params: Arc::new(RingModParams::default()), dsp: RingMod::new(), last: [f32::NAN; NP] }
    }
}

impl GfxRingMod {
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

impl Plugin for GfxRingMod {
    const NAME: &'static str = "GoleM FX Ringmod";
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
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-ringmod"))
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

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.sync(false);
        let t = ctx.transport();
        self.dsp.set_transport(TransportInfo { tempo: t.tempo, pos_beats: t.pos_beats(), playing: t.playing });
        self.dsp.process(buffer.as_slice());
        self.params.carrier_hz.set_output_value(self.dsp.carrier_hz());
        self.params.state.set_output_value(self.dsp.state() as f32);
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxRingMod {
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxRingmod01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Modulation];
}

nih_export_vst3!(GfxRingMod);
