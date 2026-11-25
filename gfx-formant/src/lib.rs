// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-formant — golem's formant shifter (design: README.md).
//!
//! - **Analysis** (every hop of 256 samples at 44.1 / 48 kHz, doubled per rate family; its work spread
//!   over the next 64 samples on sample counts, so the output does not depend on the block size):
//!   gfx-dsp `psola::Tracker` (YIN in the `register` range, voicing gate at `sens` 50 %, `thresh`,
//!   epochs, backward seeding) names the frame class; gfx-dsp `specenv` gives the power spectrum and its
//!   true envelope E: **voiced** (Voice, pitch tracked) a Hann of k whole periods, three spectra T / 3
//!   apart, order ⌊T / 2⌋; **tonal** (Instrument, or Voice untracked and peaky) N samples, order 60, the
//!   envelope smoothed 150 ms; **noise-like** (Voice untracked, fine-structure flatness over 300 Hz –
//!   8 kHz above 0.52, below 0.47 to leave) N / 4 samples, asymmetric, order 60, two iterations. Under
//!   `thresh` the frame is held (filters unchanged).
//! - **Ratio** L(f) = shift (α = 2^(`formant` / 12), α^(`unvoiced`) on noise-like frames, crossfaded
//!   10 / 30 ms over hops) + vowel + tilt, clamps, level; its minimum-phase FIR (512 taps) crossfaded
//!   over the hop that follows the frame's 64 samples.
//! - **Live** (`engine` 0): the input delayed by `lookahead` through the FIR (one kernel, a history per
//!   channel; the mid analysed), × the growl gain switched 0.2 T before each predicted epoch (1 ms
//!   raised-cosine edges), + breath. Latency = `lookahead`.
//! - **Grain** (`engine` 1): gfx-dsp `psola::Reader` at D = ⌈1.25 · T_max + gd + 2⌉ on the mid, per grain
//!   r = 2^(`shift` / 12) (slew 1 st / ms), α and gain (growl), the level matched to the fallback's
//!   (50 ms RMS; `level_norm` until both are measured; exact at r = α = 1), then the residual FIR
//!   (vowel and tilt only, when set); crossfaded (10 ms open, 30 ms close) with the fallback, the mid
//!   delayed by D through the Live FIR, by the voicing gate (open with three marks); + breath. Latency D.
//! - **Breath**: seeded white noise, high-passed at 1 kHz, through a second minimum-phase FIR of the
//!   target envelope E + L normalised to unit power through the high-pass, × the delayed input's RMS (10 ms) × the
//!   voicing weight; 100 % = 12 dB under the voice.
//!
//! Setup changes (`engine`, `register`, `lookahead` in Live): 10 ms fade-out, engines reset, new
//! latency, fade-in. `mix` is linear with the dry delayed by the latency; `bypass` crossfades (10 ms) to
//! the same dry. Identity (`formant` 0, `vowel_amt` 0, `tilt` 0, throat 0): Live is the input bit for
//! bit. Real-time safe, deterministic, block-size independent.

#![allow(clippy::needless_range_loop)]

#[cfg(feature = "plugin")]
pub mod plugin;

#[cfg(test)]
mod tests;

use gfx_dsp::delay::DelayLine;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::fir::Fir;
use gfx_dsp::lfo::Rng;
use gfx_dsp::noise::{White, WHITE_RMS};
use gfx_dsp::psola::{level_norm, Reader, Tracker, MARK_LP_HZ, REGISTERS};
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::specenv::{rate_scale, Analyzer, KernelFade, MinPhase, Ratio, Shape, Window, HOP};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::{Effect, ParamInfo};

pub const NP: usize = 16;

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::float("formant", "Formant", -12.0, 12.0, -3.0).with_exp(),
    ParamInfo::float("growl", "Growl", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("vowel", "Vowel", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("vowel_amt", "Vowel Amount", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::enumeration("engine", "Engine", 2, 0),
    ParamInfo::float("shift", "Pitch", -12.0, 12.0, 0.0).with_exp(),
    ParamInfo::enumeration("source", "Source", 2, 0),
    ParamInfo::enumeration("register", "Register", 3, 1),
    ParamInfo::float("unvoiced", "Unvoiced Shift", 0.0, 100.0, 100.0),
    ParamInfo::float("tilt", "Tilt", -6.0, 6.0, 0.0),
    ParamInfo::float("breath", "Breath", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("thresh", "Threshold", -90.0, 0.0, -60.0),
    ParamInfo::float("lookahead", "Lookahead", 0.0, 20.0, 0.0),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::float("output", "Output", -12.0, 12.0, 0.0),
    ParamInfo::boolean("bypass", "Bypass", false),
];

pub mod p {
    pub const FORMANT: usize = 0;
    pub const GROWL: usize = 1;
    pub const VOWEL: usize = 2;
    pub const VOWEL_AMT: usize = 3;
    pub const ENGINE: usize = 4;
    pub const SHIFT: usize = 5;
    pub const SOURCE: usize = 6;
    pub const REGISTER: usize = 7;
    pub const UNVOICED: usize = 8;
    pub const TILT: usize = 9;
    pub const BREATH: usize = 10;
    pub const THRESH: usize = 11;
    pub const LOOKAHEAD: usize = 12;
    pub const MIX: usize = 13;
    pub const OUTPUT: usize = 14;
    pub const BYPASS: usize = 15;
}

/// The EXP parameters, in `Formant::exp` order.
const EXP_IDS: [usize; 7] = [p::FORMANT, p::GROWL, p::VOWEL, p::VOWEL_AMT, p::SHIFT, p::BREATH, p::MIX];
const X_FORMANT: usize = 0;
const X_GROWL: usize = 1;
const X_VOWEL: usize = 2;
const X_VOWEL_AMT: usize = 3;
const X_SHIFT: usize = 4;
const X_BREATH: usize = 5;
const X_MIX: usize = 6;

/// Frame work: one stage every 4 samples (× the rate scale), the new kernels installed at stage 16
/// (64 samples after the frame).
const STAGE: usize = 4;
const INSTALL: usize = 16;
/// Frame class flatness thresholds (enter, leave), band (Hz).
const FLAT_ENTER: f32 = 0.52;
const FLAT_LEAVE: f32 = 0.47;
const FLAT_BAND: (f32, f32) = (300.0, 8000.0);
/// Tonal envelope smoothing (s), voicing ramps (s).
const TONAL_S: f64 = 0.150;
const OPEN_S: f64 = 0.010;
const CLOSE_S: f64 = 0.030;
/// Growl: depth per `growl`, roughness per `growl`², switch before the predicted epoch (periods),
/// edge (s).
const GROWL_DEPTH: f64 = 0.8;
const GROWL_ROUGH: f64 = 0.3;
const GROWL_LEAD: f64 = 0.2;
const GROWL_EDGE_S: f64 = 0.001;
const SWITCHES: usize = 64;
/// Breath: high-pass (Hz), follower (s), level at 100 % re the voice RMS (dB).
const BREATH_HP: f32 = 1000.0;
const BREATH_FOLLOW_S: f64 = 0.010;
const BREATH_DB: f64 = -12.0;
/// Grain level match (s).
const GRAIN_LEVEL_S: f64 = 0.050;
/// Pitch slew (semitones per ms).
const SHIFT_SLEW: f64 = 1.0;
/// Lookahead's maximum (ms), setup and bypass fades (s).
const LOOKAHEAD_MAX_MS: f64 = 20.0;
const SETUP_FADE_S: f32 = 0.010;
const MAX_CH: usize = 2;

#[inline]
fn idx(v: f32, n: usize) -> usize {
    (v.round().max(0.0) as usize).min(n - 1)
}

/// Grain's fixed delay D (samples): ⌈1.25 · T_max + gd + 2⌉, gd the epoch marker's low-pass delay
/// (README §2.3: 1,013 / 680 / 413 samples at 48 kHz).
pub fn grain_delay(sample_rate: f32, register: usize) -> usize {
    let sr = sample_rate as f64;
    let t_max = sr / REGISTERS[register.min(2)].0 as f64;
    let gd = sr * std::f64::consts::SQRT_2 / (std::f64::consts::TAU * MARK_LP_HZ as f64);
    (1.25 * t_max + gd + 2.0).ceil() as usize
}

/// Live's lookahead (samples) for `ms`, rounded to whole samples.
pub fn lookahead_samples(sample_rate: f32, ms: f32) -> usize {
    (ms.clamp(0.0, LOOKAHEAD_MAX_MS as f32) as f64 * 0.001 * sample_rate as f64).round() as usize
}

/// The frame class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Under `thresh`: envelope and filters held.
    Silent,
    Voiced,
    Tonal,
    Noise,
}

#[derive(Clone, Copy, Debug)]
struct Setup {
    grain: bool,
    register: usize,
    la: usize,
}

impl PartialEq for Setup {
    fn eq(&self, o: &Self) -> bool {
        self.grain == o.grain && self.register == o.register && (self.grain || self.la == o.la)
    }
}

pub struct Formant {
    v: [f32; NP],
    sr: f32,
    scale: usize,
    nch: usize,
    clock: u64,
    tr: Tracker,
    an: Analyzer,
    ratio: Ratio,
    ratio_res: Ratio,
    mp: MinPhase,
    fade: KernelFade,
    fade_res: KernelFade,
    fade_br: KernelFade,
    firs: [Fir; MAX_CH],
    fir_res: Fir,
    fir_br: Fir,
    dly: [DelayLine; MAX_CH],
    reader: Reader,
    // setup
    act: Setup,
    latency: usize,
    setup_pending: bool,
    setup_g: f32,
    bypass: LinearRamp,
    // the frame
    class: Class,
    noise_flag: bool,
    noise_w: f64,
    env_s: Vec<f32>,
    env_s_on: bool,
    res_on: bool,
    br_on: bool,
    br_l: Vec<f32>,
    // smoothers
    exp: [ExpSmoother; 7],
    unvoiced: OnePole,
    output: OnePole,
    c: [f64; 7],
    // growl (Live): scheduled switches (input time), the raised-cosine edge
    sw: [f64; SWITCHES],
    sw_head: usize,
    sw_len: usize,
    g_from: f64,
    g_to: f64,
    g_pos: u32,
    g_len: u32,
    g_now: f64,
    parity: bool,
    rng: Rng,
    // grain
    gv: f64,
    vg: f64,
    ms_grain: f64,
    ms_fb: f64,
    a_level: f64,
    marks_since: usize,
    was_active: bool,
    shift_now: f64,
    // breath
    noise: White,
    br_hp: Svf,
    br_ms: f64,
    a_follow: f64,
    /// a shaped breath kernel is installed; the breath's fade-in from it (10 ms)
    br_ready: bool,
    br_fade: f64,
    // meter
    state: u8,
}

impl Default for Formant {
    fn default() -> Self {
        Self::new()
    }
}

impl Formant {
    pub fn new() -> Self {
        let v: [f32; NP] = core::array::from_fn(|i| PARAMS[i].default);
        let mut tr = Tracker::new();
        tr.set_backward_seed(true);
        let mut reader = Reader::new();
        reader.set_left_guard(true);
        let mut f = Self {
            v,
            sr: 0.0,
            scale: 1,
            nch: 1,
            clock: 0,
            tr,
            an: Analyzer::new(),
            ratio: Ratio::new(),
            ratio_res: Ratio::new(),
            mp: MinPhase::new(),
            fade: KernelFade::new(),
            fade_res: KernelFade::new(),
            fade_br: KernelFade::new(),
            firs: [Fir::new(), Fir::new()],
            fir_res: Fir::new(),
            fir_br: Fir::new(),
            dly: [DelayLine::new(), DelayLine::new()],
            reader,
            act: Setup { grain: false, register: 1, la: 0 },
            latency: 0,
            setup_pending: false,
            setup_g: 1.0,
            bypass: LinearRamp::new(10.0, 0.0),
            class: Class::Silent,
            noise_flag: false,
            noise_w: 0.0,
            env_s: Vec::new(),
            env_s_on: false,
            res_on: false,
            br_on: false,
            br_l: Vec::new(),
            exp: core::array::from_fn(|k| {
                let pi = PARAMS[EXP_IDS[k]];
                ExpSmoother::new(pi.min, pi.max, pi.default)
            }),
            unvoiced: OnePole::new(30.0, 100.0, 1e-6),
            output: OnePole::new(30.0, 0.0, 1e-6),
            c: core::array::from_fn(|k| PARAMS[EXP_IDS[k]].default as f64),
            sw: [0.0; SWITCHES],
            sw_head: 0,
            sw_len: 0,
            g_from: 1.0,
            g_to: 1.0,
            g_pos: 0,
            g_len: 48,
            g_now: 1.0,
            parity: false,
            rng: Rng::new(0x6f72_6d61_6e74),
            gv: 0.0,
            vg: 0.0,
            ms_grain: 0.0,
            ms_fb: 0.0,
            a_level: 0.0,
            marks_since: 0,
            was_active: false,
            shift_now: 0.0,
            noise: White::new(0x6272_6561_7468),
            br_hp: Svf::new(),
            br_ms: 0.0,
            a_follow: 0.0,
            br_ready: false,
            br_fade: 0.0,
            state: 0,
        };
        f.prepare(48000.0, 512, 1);
        f
    }

    fn want(&self) -> Setup {
        let grain = self.v[p::ENGINE] >= 0.5;
        Setup { grain, register: idx(self.v[p::REGISTER], 3), la: if grain { 0 } else { lookahead_samples(self.sr, self.v[p::LOOKAHEAD]) } }
    }

    /// The voice state meter: 0 under `thresh` (held), 1 not tracked, 2 voiced.
    pub fn state(&self) -> u8 {
        self.state
    }

    /// The last frame's class (tests, diagnostics).
    pub fn class(&self) -> Class {
        self.class
    }

    /// The current main log ratio L (dB, N / 2 + 1 bins) and the analysis envelope (diagnostics).
    pub fn ratio_db(&self) -> &[f32] {
        self.ratio.l()
    }

    pub fn envelope_db(&self) -> &[f32] {
        self.an.envelope()
    }

    /// The Live growl gain now (diagnostics).
    pub fn growl_gain(&self) -> f64 {
        self.g_now
    }

    /// The main ratio's level correction (dB, diagnostics).
    pub fn level_db(&self) -> f64 {
        self.ratio.level_db()
    }

    /// Grain's voicing weight (0 = fallback filter, 1 = grains).
    pub fn grain_weight(&self) -> f64 {
        self.gv
    }

    /// The tracker (diagnostics).
    pub fn tracker(&self) -> &Tracker {
        &self.tr
    }

    fn hop(&self) -> usize {
        HOP * self.scale
    }

    /// Configures the engines for the wanted setup (no allocation) and resets the analysis.
    fn apply_setup(&mut self) {
        self.act = self.want();
        let reg = self.act.register;
        self.tr.set_register(reg);
        let t_max = self.sr as f64 / REGISTERS[reg].0 as f64;
        self.latency = if self.act.grain { grain_delay(self.sr, reg) } else { self.act.la };
        self.reader.configure(grain_delay(self.sr, reg), t_max);
        self.reset_analysis();
    }

    fn reset_analysis(&mut self) {
        self.tr.reset();
        self.reader.reset();
        self.ratio.reset();
        self.ratio_res.reset();
        for f in [&mut self.fade, &mut self.fade_res, &mut self.fade_br] {
            f.reset();
        }
        for f in self.firs.iter_mut() {
            f.reset();
        }
        self.fir_res.reset();
        self.fir_br.reset();
        self.class = Class::Silent;
        self.noise_flag = false;
        self.noise_w = 0.0;
        self.env_s_on = false;
        self.res_on = false;
        self.br_on = false;
        self.sw_head = 0;
        self.sw_len = 0;
        self.g_from = 1.0;
        self.g_to = 1.0;
        self.g_pos = 0;
        self.g_now = 1.0;
        self.parity = false;
        self.rng.reset();
        self.gv = 0.0;
        self.vg = 0.0;
        self.ms_grain = 0.0;
        self.ms_fb = 0.0;
        self.marks_since = 0;
        self.was_active = false;
        self.noise.reset();
        self.br_hp.reset();
        self.br_ms = 0.0;
        self.br_ready = false;
        self.br_fade = 0.0;
        self.state = 0;
    }

    fn set_targets(&mut self) {
        let now = self.clock;
        for (k, &i) in EXP_IDS.iter().enumerate() {
            self.exp[k].set_target(self.v[i], now);
        }
        self.unvoiced.set_target(self.v[p::UNVOICED]);
        self.output.set_target(self.v[p::OUTPUT]);
        self.bypass.set_target(if self.v[p::BYPASS] >= 0.5 { 1.0 } else { 0.0 });
        self.tr.set_gate(50.0, self.v[p::THRESH]);
    }

    // ---- analysis -----------------------------------------------------------------------------

    /// The frame's α for the main (fallback) filter: α^(`unvoiced`) on noise-like frames.
    fn frame_alpha(&self) -> f64 {
        let a = (self.c[X_FORMANT] / 12.0).exp2();
        let u = (self.unvoiced.value() as f64 * 0.01).clamp(0.0, 1.0);
        a.powf(1.0 - self.noise_w * (1.0 - u))
    }

    fn shape(&self, alpha: f64) -> Shape {
        Shape {
            alpha,
            vowel: (self.c[X_VOWEL] * 0.01) as f32,
            vowel_amt: (self.c[X_VOWEL_AMT] * 0.01).clamp(0.0, 1.0) as f32,
            tilt: self.v[p::TILT].clamp(-6.0, 6.0),
            weight: 1.0,
        }
    }

    /// Stage 0: the frame class, the spectrum, TE's start.
    fn frame_begin(&mut self) {
        let m = self.scale;
        let end = self.tr.time();
        let (hist, mask) = (self.tr.history(), self.tr.mask());
        let sr = self.sr as f64;
        let voice = self.v[p::SOURCE] < 0.5;
        let tracked = self.tr.gate_open() && self.tr.f_det() > 0.0;
        let (class, level) = if voice && tracked {
            let t = if self.tr.active() { self.tr.period() } else { sr / self.tr.f_det() as f64 };
            (Class::Voiced, self.an.analyse(hist, mask, end, Window::Periods(t)))
        } else {
            let lv = self.an.analyse(hist, mask, end, Window::Hann);
            if voice {
                let fl = self.an.flatness(FLAT_BAND.0, FLAT_BAND.1);
                if lv > 0.0 {
                    self.noise_flag = if self.noise_flag { fl > FLAT_LEAVE } else { fl > FLAT_ENTER };
                }
            } else {
                self.noise_flag = false;
            }
            if self.noise_flag {
                (Class::Noise, self.an.analyse(hist, mask, end, Window::Short))
            } else {
                (Class::Tonal, lv)
            }
        };
        let db = 10.0 * level.max(1e-30).log10();
        if db < self.v[p::THRESH] as f64 {
            self.class = Class::Silent;
            self.state = 0;
            return;
        }
        self.class = class;
        self.state = if self.tr.gate_open() && self.tr.active() { 2 } else { 1 };
        // the unvoiced weight: falls as the voicing gate opens (10 ms), rises as it closes (30 ms)
        let hop_s = self.hop() as f64 / sr;
        if class == Class::Noise {
            self.noise_w = (self.noise_w + hop_s / CLOSE_S).min(1.0);
        } else {
            self.noise_w = (self.noise_w - hop_s / OPEN_S).max(0.0);
        }
        let (order, iters) = match class {
            Class::Voiced => {
                let t = if self.tr.active() { self.tr.period() } else { sr / self.tr.f_det() as f64 };
                (((0.5 * t).floor() as usize).clamp(24 * m, 400 * m), 8)
            }
            Class::Tonal => (60 * m, 16),
            _ => (60 * m, 2),
        };
        self.an.te_start(order, iters);
    }

    /// Stage 9: TE's end, the tonal smoothing, the ratios.
    fn frame_compose(&mut self) {
        while !self.an.te_step() {}
        self.an.te_finish();
        if self.class == Class::Tonal {
            let a = (-(self.hop() as f64) / (TONAL_S * self.sr as f64)).exp() as f32;
            let e = self.an.envelope_mut();
            if self.env_s_on {
                for (s, v) in self.env_s.iter_mut().zip(e.iter_mut()) {
                    *s = a * *s + (1.0 - a) * *v;
                    *v = *s;
                }
            } else {
                self.env_s.copy_from_slice(e);
                self.env_s_on = true;
            }
        } else {
            self.env_s_on = false;
        }
        let shape = self.shape(self.frame_alpha());
        let main = self.ratio.compose(self.an.envelope(), self.an.power(), &shape, false);
        if main {
            let k = self.fade.next_mut();
            self.mp.design(self.ratio.l(), 0.0, k);
        } else {
            self.fade.set_next_identity();
        }
    }

    /// Stage 10: Grain's residual filter (vowel and tilt on the grains).
    fn frame_residual(&mut self) {
        if !self.act.grain {
            return;
        }
        let shape = self.shape((self.c[X_FORMANT] / 12.0).exp2());
        self.res_on = self.ratio_res.compose(self.an.envelope(), self.an.power(), &shape, true);
        if self.res_on {
            let k = self.fade_res.next_mut();
            self.mp.design(self.ratio_res.l(), 0.0, k);
        } else {
            self.fade_res.set_next_identity();
        }
    }

    /// Stage 11: the breath filter from the target envelope, normalised to unit noise power through the
    /// 1 kHz high-pass.
    fn frame_breath(&mut self) {
        self.br_on = self.c[X_BREATH] > 0.0 || self.exp[X_BREATH].target() > 0.0;
        if !self.br_on {
            return;
        }
        let tgt = self.ratio.target();
        let nb = tgt.len();
        // unit noise power through the 1 kHz high-pass (12 dB / oct: its response weights every bin)
        let df = self.sr as f64 / (2 * (nb - 1)) as f64;
        let peak = tgt.iter().fold(f32::MIN, |m, &v| m.max(v));
        let sum: f64 = tgt
            .iter()
            .enumerate()
            .map(|(k, &v)| {
                let r4 = (k as f64 * df / BREATH_HP as f64).powi(4);
                r4 / (1.0 + r4) * 10f64.powf((v - peak) as f64 * 0.1)
            })
            .sum();
        let norm = peak as f64 + 10.0 * (sum / nb as f64).max(1e-30).log10();
        for (b, &t) in self.br_l.iter_mut().zip(tgt.iter()) {
            *b = (t as f64 - norm).max(-120.0) as f32;
        }
        let k = self.fade_br.next_mut();
        self.mp.design(&self.br_l, 0.0, k);
    }

    fn frame_install(&mut self) {
        self.fade.commit();
        if self.act.grain {
            self.fade_res.commit();
        }
        if self.br_on {
            self.fade_br.commit();
            if !self.br_ready {
                // the first shaped kernel: no crossfade from the identity (unshaped noise)
                self.fade_br.settle();
            }
            self.br_ready = true;
        }
    }

    /// The frame schedule at the tracker's time (after its push).
    fn schedule(&mut self) {
        let t = (self.tr.time() - 1) as usize;
        let hop = self.hop();
        let unit = STAGE * self.scale;
        let o = t % hop;
        if !o.is_multiple_of(unit) {
            return;
        }
        let stage = o / unit;
        if stage == 0 {
            self.frame_begin();
            return;
        }
        if self.class == Class::Silent {
            return;
        }
        match stage {
            1..=8 => {
                let n = if self.class == Class::Tonal { 2 } else { 1 };
                for _ in 0..n {
                    self.an.te_step();
                }
            }
            9 => self.frame_compose(),
            10 => self.frame_residual(),
            11 => self.frame_breath(),
            INSTALL => self.frame_install(),
            _ => {}
        }
    }

    // ---- growl --------------------------------------------------------------------------------

    /// A new period's gain: alternate periods by d, every one by a seeded roughness.
    fn next_growl_gain(parity: &mut bool, rng: &mut Rng, growl: f64) -> f64 {
        let g = (growl * 0.01).clamp(0.0, 1.0);
        *parity = !*parity;
        let u = rng.bipolar() as f64;
        let base = if *parity { 1.0 - GROWL_DEPTH * g } else { 1.0 };
        base * (1.0 + GROWL_ROUGH * g * g * u)
    }

    fn growl_to(&mut self, g: f64) {
        self.g_from = self.g_now;
        self.g_to = g;
        self.g_pos = 0;
    }

    /// Live: switches scheduled on the predicted epochs, applied at audio time `tau`.
    fn growl_live(&mut self, mark: bool, tau: f64) -> f64 {
        let voiced = self.tr.active() && self.tr.gate_open();
        if mark && voiced && self.sw_len < SWITCHES {
            if let Some(pn) = self.tr.predicted_next() {
                let s = pn - GROWL_LEAD * self.tr.period();
                self.sw[(self.sw_head + self.sw_len) % SWITCHES] = s;
                self.sw_len += 1;
            }
        }
        while self.sw_len > 0 && self.sw[self.sw_head] <= tau {
            self.sw_head = (self.sw_head + 1) % SWITCHES;
            self.sw_len -= 1;
            let g = Self::next_growl_gain(&mut self.parity, &mut self.rng, self.c[X_GROWL]);
            self.growl_to(g);
        }
        if !voiced && self.sw_len == 0 && self.g_to != 1.0 {
            self.growl_to(1.0);
        }
        if self.g_pos < self.g_len {
            self.g_pos += 1;
            let w = 0.5 - 0.5 * (std::f64::consts::PI * self.g_pos as f64 / self.g_len as f64).cos();
            self.g_now = if self.g_pos == self.g_len { self.g_to } else { self.g_from + (self.g_to - self.g_from) * w };
        }
        self.g_now
    }

    fn process_inner(&mut self, buffers: &mut [&mut [f32]]) {
        let n = buffers.first().map_or(0, |b| b.len());
        let nch = buffers.len().min(MAX_CH);
        let sr = self.sr as f64;
        let setup_step = 1.0 / (SETUP_FADE_S * self.sr);
        let (open, close) = (1.0 / (OPEN_S * sr), 1.0 / (CLOSE_S * sr));
        let slew = SHIFT_SLEW / (0.001 * sr);
        for i in 0..n {
            // setup changes: fade out, reconfigure, fade in
            if !self.setup_pending && self.want() != self.act {
                self.setup_pending = true;
            }
            if self.setup_pending {
                self.setup_g -= setup_step;
                if self.setup_g <= 0.0 {
                    self.setup_g = 0.0;
                    self.setup_pending = false;
                    self.apply_setup();
                }
            } else if self.setup_g < 1.0 {
                self.setup_g = (self.setup_g + setup_step).min(1.0);
            }
            // smoothed controls
            for k in 0..EXP_IDS.len() {
                self.c[k] = self.exp[k].next() as f64;
            }
            self.unvoiced.next();
            let out_db = self.output.next();
            let bp = self.bypass.next();
            let mut x = [0.0f32; MAX_CH];
            let mut xd = [0.0f32; MAX_CH];
            for c in 0..nch {
                x[c] = buffers[c][i];
                self.dly[c].push(x[c]);
                xd[c] = self.dly[c].tap(self.latency);
            }
            let mid = if nch > 1 { 0.5 * (x[0] + x[1]) } else { x[0] };
            let mid_d = if nch > 1 { 0.5 * (xd[0] + xd[1]) } else { xd[0] };
            let ev = self.tr.push(mid);
            let t = self.tr.time() - 1;
            self.schedule();
            let voiced = self.tr.active() && self.tr.gate_open();
            self.vg = if voiced { (self.vg + open).min(1.0) } else { (self.vg - close).max(0.0) };
            self.fade.tick();
            let mut y = [0.0f32; MAX_CH];
            let gate_w = if !self.act.grain {
                let g = self.growl_live(ev.mark, t as f64 - self.latency as f64);
                for c in 0..nch {
                    self.firs[c].push(xd[c]);
                    let v = self.fade.apply(&self.firs[c], xd[c]);
                    y[c] = if g == 1.0 { v } else { (v as f64 * g) as f32 };
                }
                self.vg
            } else {
                // the marks
                let active = self.tr.active();
                let activated = active && !self.was_active;
                self.was_active = active;
                if !active {
                    self.marks_since = 0;
                }
                if activated || ev.mark {
                    let marks = self.tr.marks();
                    let from = if activated { 0 } else { marks.len().saturating_sub(1) };
                    let per = self.tr.period();
                    for k in from..marks.len() {
                        let m = self.tr.marks()[k];
                        self.reader.push_mark(m, per);
                        self.marks_since += 1;
                    }
                }
                let target = (self.c[X_SHIFT]).clamp(-12.0, 12.0);
                self.shift_now += (target - self.shift_now).clamp(-slew, slew);
                let r = (self.shift_now / 12.0).exp2();
                let alpha = (self.c[X_FORMANT] / 12.0).exp2();
                let growl = self.c[X_GROWL];
                let (parity, rng) = (&mut self.parity, &mut self.rng);
                let mut ctl = |_src: f64, _per: f64| {
                    let g = if growl > 0.0 { Self::next_growl_gain(parity, rng, growl) } else { 1.0 };
                    (r, alpha, g as f32)
                };
                let mut grain = self.reader.process_with(t, self.tr.history(), self.tr.mask(), &mut ctl);
                // the level: the grains' RMS matched to the fallback's (both 50 ms), from
                // `level_norm` (r, α) as the start; exact at r = α = 1
                self.firs[0].push(mid_d);
                let fb = self.fade.apply(&self.firs[0], mid_d);
                self.ms_grain += self.a_level * ((grain as f64) * (grain as f64) - self.ms_grain);
                self.ms_fb += self.a_level * ((fb as f64) * (fb as f64) - self.ms_fb);
                if r != 1.0 || alpha != 1.0 {
                    let n = if self.ms_grain > 1e-12 && self.ms_fb > 1e-12 { (self.ms_fb / self.ms_grain).sqrt() } else { level_norm(r, alpha) };
                    grain = (grain as f64 * n.clamp(0.25, 4.0)) as f32;
                }
                self.fade_res.tick();
                if !self.fade_res.is_identity() {
                    self.fir_res.push(grain);
                    grain = self.fade_res.apply(&self.fir_res, grain);
                }
                let on = voiced && self.marks_since >= 3;
                self.gv = if on { (self.gv + open).min(1.0) } else { (self.gv - close).max(0.0) };
                let w = if self.gv >= 1.0 {
                    grain
                } else if self.gv <= 0.0 {
                    fb
                } else {
                    (self.gv * grain as f64 + (1.0 - self.gv) * fb as f64) as f32
                };
                y[..nch].iter_mut().for_each(|v| *v = w);
                self.gv
            };
            // breath
            self.br_ms += self.a_follow * ((mid_d as f64) * (mid_d as f64) - self.br_ms);
            let br = self.c[X_BREATH];
            self.fade_br.tick();
            // the breath waits for its first shaped kernel (unshaped high-passed noise would be a
            // burst of hiss), then fades in over 10 ms
            self.br_fade = if self.br_ready && br > 0.0 { (self.br_fade + open).min(1.0) } else if br > 0.0 { 0.0 } else { self.br_fade };
            if br > 0.0 && self.br_fade > 0.0 {
                let nz = self.br_hp.process(self.noise.next());
                self.fir_br.push(nz);
                let b = self.fade_br.apply(&self.fir_br, nz) as f64;
                let amt = b * self.br_ms.sqrt() / WHITE_RMS * 10f64.powf(BREATH_DB / 20.0) * (br * 0.01) * gate_w * self.br_fade;
                for c in 0..nch {
                    y[c] = (y[c] as f64 + amt) as f32;
                }
            }
            // mix, output, bypass, setup fade
            let m = (self.c[X_MIX] * 0.01).clamp(0.0, 1.0) as f32;
            let og = gfx_dsp::util::db_to_gain(out_db);
            let gs = self.setup_g;
            for c in 0..nch {
                let wet = if m >= 1.0 { y[c] } else { (1.0 - m) * xd[c] + m * y[c] };
                let o = if og == 1.0 { wet } else { wet * og };
                let o = if bp <= 0.0 { o } else { bp * xd[c] + (1.0 - bp) * o };
                buffers[c][i] = if gs >= 1.0 { o } else { gs * o };
            }
            self.clock += 1;
        }
    }
}

impl Effect for Formant {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate;
        self.scale = rate_scale(sample_rate);
        self.nch = channels.clamp(1, MAX_CH);
        let sr = sample_rate as f64;
        self.tr.prepare(sample_rate, 0.4);
        self.an.prepare(sample_rate);
        self.ratio.prepare(sample_rate);
        self.ratio_res.prepare(sample_rate);
        self.mp.prepare(sample_rate);
        let taps = self.mp.taps();
        let hop = self.hop();
        for f in [&mut self.fade, &mut self.fade_res, &mut self.fade_br] {
            f.prepare(taps, hop);
        }
        for f in self.firs.iter_mut() {
            f.prepare(taps);
        }
        self.fir_res.prepare(taps);
        self.fir_br.prepare(taps);
        let nb = self.an.bins();
        self.env_s = vec![0.0; nb];
        self.br_l = vec![0.0; nb];
        let max_lat = grain_delay(sample_rate, 0).max(lookahead_samples(sample_rate, LOOKAHEAD_MAX_MS as f32));
        for d in self.dly.iter_mut() {
            d.prepare(max_lat + 4);
        }
        for e in self.exp.iter_mut() {
            e.prepare(sample_rate);
        }
        self.unvoiced.prepare(sample_rate);
        self.output.prepare(sample_rate);
        self.bypass.prepare(sample_rate);
        self.br_hp.set(SvfMode::HighPass, BREATH_HP, std::f32::consts::FRAC_1_SQRT_2, 0.0, sample_rate);
        self.a_follow = 1.0 - (-1.0 / (BREATH_FOLLOW_S * sr)).exp();
        self.a_level = 1.0 - (-1.0 / (GRAIN_LEVEL_S * sr)).exp();
        self.g_len = ((GROWL_EDGE_S * sr).round() as u32).max(1);
        self.reset();
    }

    fn reset(&mut self) {
        self.clock = 0;
        self.set_targets();
        for (k, e) in self.exp.iter_mut().enumerate() {
            e.reset();
            self.c[k] = e.value() as f64;
        }
        self.unvoiced.settle();
        self.output.settle();
        self.bypass.settle();
        self.setup_pending = false;
        self.setup_g = 1.0;
        self.shift_now = self.c[X_SHIFT];
        self.apply_setup();
        for d in self.dly.iter_mut() {
            d.reset();
        }
    }

    fn set_param(&mut self, id: &str, value: f32) {
        let Some(i) = PARAMS.iter().position(|pi| pi.id == id) else { return };
        let pi = &PARAMS[i];
        self.v[i] = value.clamp(pi.min, pi.max);
        self.set_targets();
    }

    fn meter(&self, id: &str) -> Option<f32> {
        match id {
            "state" => Some(self.state as f32),
            _ => None,
        }
    }

    fn latency(&self) -> usize {
        self.latency
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_inner(buffers);
    }
}
