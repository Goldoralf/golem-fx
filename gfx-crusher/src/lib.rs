// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-crusher — golem's bitcrusher and sample-rate reducer (design: README.md).
//!
//! A virtual converter simulated exactly: an ADC → DAC pair at its own fractional, moving clock and
//! word format (gfx-dsp `converter`), the input read at each instant and the held staircase rendered
//! through one minimum-phase band-limited kernel (gfx-dsp `blep`). Everything the virtual converter
//! aliases, aliases; nothing else does. Around it: the models' anti-alias and reconstruction filters
//! (filters.rs), the tails window (quantisation faded out below 1 – 2 steps), the clock's jitter,
//! modulation and pitch tracking, and a dry path through the kernels' own response (so a blend
//! keeps only the hold's T / 2 between dry and wet).
//!
//! - Off (R ≥ min(fs, 48 kHz)): the virtual clock is the host's, no kernel, the dry undelayed.
//! - A change of model, format or Off starts the other path (a copy of the running one) and
//!   crossfades: 20 ms, 10 ms across Off.
//! - Reported latency 0: the wet carries the kernels' few samples of minimum-phase delay.
//!
//! Real-time safe, deterministic (seeded dither, jitter and ladder; LFO at phase 0).

#![allow(clippy::needless_range_loop)]

pub mod filters;

#[cfg(feature = "plugin")]
pub mod plugin;

#[cfg(test)]
mod tests;

use filters::{Cascade, Proto};
use gfx_dsp::blep::{Kernel, Reader, Renderer};
use gfx_dsp::converter::{tail_gain, Adc, BoxSampler, Clock, Dac, Format, Instants, Shape};
use gfx_dsp::detect::PitchDetector;
use gfx_dsp::envelope::{PeakFollower, SlidingMax};
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::smooth::OnePole;
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::transport::{Sync, TransportInfo};
use gfx_dsp::util::one_pole_coef;
use gfx_dsp::{Effect, ParamInfo};

pub const NP: usize = 25;

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::float("quant", "Quant", 1.0, 16.0, 8.0).with_exp(),
    ParamInfo::float("rate", "Rate", 50.0, 48000.0, 16000.0).with_exp(),
    ParamInfo::float("nonlin", "Non-Lin", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::enumeration("model", "Model", 6, 0),
    ParamInfo::enumeration("law", "Format", 3, 0),
    ParamInfo::enumeration("quantizer", "Quantizer", 3, 0),
    ParamInfo::float("dither", "Dither", 0.0, 100.0, 0.0),
    ParamInfo::enumeration("tails", "Tails", 3, 0),
    ParamInfo::float("input", "Input", -24.0, 24.0, 0.0),
    ParamInfo::float("level", "Level", -24.0, 12.0, 0.0).with_exp(),
    ParamInfo::float("low_cut", "Low Cut", 20.0, 1000.0, 20.0),
    ParamInfo::float("high_cut", "High Cut", 1000.0, 20000.0, 20000.0).with_exp(),
    ParamInfo::float("asym", "Asymmetry", 0.0, 100.0, 0.0),
    ParamInfo::float("ladder", "DAC Error", 0.0, 100.0, 0.0),
    ParamInfo::float("rate_shift", "Rate Shift", -72.0, 0.0, 0.0).with_exp(),
    ParamInfo::float("jitter", "Jitter", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::boolean("track", "Track", false),
    ParamInfo::enumeration("mod", "Rate Mod", 3, 0),
    ParamInfo::float("mod_depth", "Mod Depth", -100.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("mod_rate", "Mod Rate", 0.05, 10.0, 0.5).with_exp(),
    ParamInfo::enumeration("sync", "Sync", 17, 0),
    ParamInfo::float("env_sens", "Envelope Sensitivity", 0.0, 100.0, 50.0),
    ParamInfo::float("env_att", "Envelope Attack", 1.0, 100.0, 5.0),
    ParamInfo::float("env_rel", "Envelope Release", 20.0, 2000.0, 200.0),
];

pub mod p {
    pub const QUANT: usize = 0;
    pub const RATE: usize = 1;
    pub const NONLIN: usize = 2;
    pub const MIX: usize = 3;
    pub const MODEL: usize = 4;
    pub const LAW: usize = 5;
    pub const QUANTIZER: usize = 6;
    pub const DITHER: usize = 7;
    pub const TAILS: usize = 8;
    pub const INPUT: usize = 9;
    pub const LEVEL: usize = 10;
    pub const LOW_CUT: usize = 11;
    pub const HIGH_CUT: usize = 12;
    pub const ASYM: usize = 13;
    pub const LADDER: usize = 14;
    pub const RATE_SHIFT: usize = 15;
    pub const JITTER: usize = 16;
    pub const TRACK: usize = 17;
    pub const MOD: usize = 18;
    pub const MOD_DEPTH: usize = 19;
    pub const MOD_RATE: usize = 20;
    pub const SYNC: usize = 21;
    pub const ENV_SENS: usize = 22;
    pub const ENV_ATT: usize = 23;
    pub const ENV_REL: usize = 24;
}

/// Option labels (enum order, final: append only).
pub const MODELS: [&str; 6] = ["Raw", "Clean", "Average", "Sampler", "Drum Machine", "Console"];
pub const LAWS: [&str; 3] = ["Linear", "µ-law", "Float"];
pub const QUANTIZERS: [&str; 3] = ["Tread", "Riser", "Truncate"];
pub const TAILS: [&str; 3] = ["Clean", "Gate", "Raw"];
pub const MODS: [&str; 3] = ["Off", "LFO", "Envelope"];

pub mod model {
    pub const RAW: usize = 0;
    pub const CLEAN: usize = 1;
    pub const AVERAGE: usize = 2;
    pub const SAMPLER: usize = 3;
    pub const DRUM: usize = 4;
    pub const CONSOLE: usize = 5;
}

/// Above this the virtual clock is the host's (Off).
pub const OFF_HZ: f64 = 48000.0;
/// Lowest virtual rate (after shift and modulation).
pub const MIN_HZ: f64 = 20.0;
/// The Console model's Gaussian FIR on the virtual samples.
pub const CONSOLE_FIR: [f64; 3] = [0.18, 0.64, 0.18];
/// The Drum Machine ladder's feedback (light resonance) [I: voicing].
pub const LADDER_K: f64 = 1.0;
/// Track's hysteresis on N: ±(0.6 + 6 % of N).
pub const TRACK_HYST: f64 = 0.6;
pub const TRACK_HYST_REL: f64 = 0.06;
/// The tails window's smoothing: time constants toward crushed and toward tail (s).
pub const TAIL_DOWN_S: f64 = 0.000_25;
pub const TAIL_UP_S: f64 = 0.020;
const MAX_CH: usize = 2;
const CTL: u32 = 16;
/// The dry's history (the matching FIR has 2 · TAPS − 1 taps).
const DRY_N: usize = 2 * gfx_dsp::blep::TAPS;

/// The structure a path runs: what a crossfade is needed to change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Config {
    model: usize,
    law: usize,
    off: bool,
}

/// The models' filter edges, as fractions of R: (PRE, POST).
fn model_edges(m: usize) -> (Option<f64>, Option<f64>) {
    match m {
        model::CLEAN => (Some(0.45), Some(0.45)),
        model::SAMPLER => (Some(0.45), Some(0.42)),
        model::DRUM => (None, Some(0.5)),
        _ => (None, None),
    }
}

/// The prototypes, built once.
struct Protos {
    elliptic: Proto,
    butter6: Proto,
    ladder: Proto,
}

impl Protos {
    fn new() -> Self {
        Self { elliptic: Proto::elliptic(9, 0.1, 70.0).0, butter6: Proto::butterworth(6), ladder: Proto::ladder(LADDER_K) }
    }
    fn pre(&self, m: usize) -> Option<&Proto> {
        match m {
            model::CLEAN => Some(&self.elliptic),
            model::SAMPLER => Some(&self.butter6),
            _ => None,
        }
    }
    fn post(&self, m: usize) -> Option<&Proto> {
        match m {
            model::CLEAN => Some(&self.elliptic),
            model::SAMPLER => Some(&self.butter6),
            model::DRUM => Some(&self.ladder),
            _ => None,
        }
    }
}

/// What every path's conversion shares at one host sample.
#[derive(Clone, Copy)]
struct Conv {
    tails: usize,
    g: f64,
}

/// One channel's converter for one configuration.
#[derive(Clone)]
struct Path {
    cfg: Config,
    pre: Cascade,
    post: Cascade,
    reader: Reader,
    boxs: BoxSampler,
    adc: Adc,
    dac: Dac,
    /// the Console FIR's previous virtual levels
    fir: [f64; 2],
    render: Renderer,
}

impl Path {
    fn new() -> Self {
        Self {
            cfg: Config { model: 0, law: 0, off: false },
            pre: Cascade::default(),
            post: Cascade::default(),
            reader: Reader::default(),
            boxs: BoxSampler::default(),
            adc: Adc::default(),
            dac: Dac::default(),
            fir: [0.0; 2],
            render: Renderer::default(),
        }
    }

    fn reset(&mut self) {
        self.pre.reset();
        self.post.reset();
        self.reader.reset();
        self.boxs.reset();
        self.adc.reset();
        self.fir = [0.0; 2];
        self.render.reset();
    }

    /// One virtual sample, from the ADC's input to the held level.
    #[inline]
    fn convert(&mut self, s: f32, c: Conv) -> f32 {
        let x = (s as f64).clamp(-1.0, 1.0);
        let w = self.adc.convert(x);
        let l = self.dac.decode(w);
        let v = match c.tails {
            0 => l + c.g * (x - l),
            1 => l * (1.0 - c.g),
            _ => l,
        };
        let v = self.dac.curve(v);
        if self.cfg.model == model::CONSOLE {
            let y = CONSOLE_FIR[0] * v + CONSOLE_FIR[1] * self.fir[0] + CONSOLE_FIR[2] * self.fir[1];
            self.fir[1] = self.fir[0];
            self.fir[0] = v;
            y as f32
        } else {
            v as f32
        }
    }

    #[inline]
    fn process(&mut self, k: &Kernel, x: f32, ins: &Instants, c: Conv) -> f32 {
        let xp = if self.pre.is_empty() { x } else { self.pre.process(x) };
        let y = if self.cfg.off {
            self.reader.push(xp);
            self.convert(xp, c)
        } else {
            let avg = self.cfg.model == model::AVERAGE;
            for j in 0..ins.n {
                let d = ins.d[j];
                let s = if avg { self.boxs.sample(&self.reader, k, d) } else { self.reader.read(k, d) };
                let v = self.convert(s, c);
                self.render.set(k, v, d);
            }
            self.reader.push(xp);
            if avg {
                self.boxs.push(xp);
            }
            self.render.next()
        };
        if self.post.is_empty() {
            y
        } else {
            self.post.process(y)
        }
    }
}

/// One channel.
#[derive(Clone)]
struct Chan {
    low: Svf,
    det: SlidingMax,
    /// the tails window, smoothed
    g: f64,
    /// the dry's last DRY_N samples, newest first, stored twice
    dry: [f32; 2 * DRY_N],
    dpos: usize,
    paths: [Path; 2],
    high: [Svf; 2],
    /// the last output of the path running Off (where an on-path's hold starts)
    last: f32,
}

/// The rate's modulation sources.
#[derive(Clone, Debug, Default)]
struct Track {
    /// log2 of the smoothed fundamental (NaN: none yet)
    f0: f64,
    f0_target: f64,
    n: f64,
    unvoiced: u64,
}

pub struct Crusher {
    v: [f32; NP],
    sr: f64,
    nch: usize,
    ch: Vec<Chan>,
    protos: Protos,
    kernel: &'static Kernel,
    dry_fir: [f32; 2 * gfx_dsp::blep::TAPS - 1],
    /// R at the previous control tick (the filter design looks one tick ahead)
    r_ctl: f64,
    clock: Clock,
    now: u64,
    // EXP-capable controls
    quant: ExpSmoother,
    rate: ExpSmoother,
    nonlin: ExpSmoother,
    mix: ExpSmoother,
    level: ExpSmoother,
    high_cut: ExpSmoother,
    rate_shift: ExpSmoother,
    jitter: ExpSmoother,
    mod_depth: ExpSmoother,
    mod_rate: ExpSmoother,
    // the others
    dither: OnePole,
    input: OnePole,
    low_cut: OnePole,
    asym: OnePole,
    ladder: OnePole,
    /// log2 R, smoothed, and whether it sits on its target
    log_r: f64,
    r_settled: bool,
    in_g: f32,
    out_g: f32,
    r_now: f64,
    /// R and high / low cuts at the last filter design
    designed_r: f64,
    designed_hc: f32,
    designed_lc: f32,
    ctl: u32,
    // crossfade between the two paths
    cur: usize,
    xf_on: bool,
    xf_pos: u32,
    xf_len: u32,
    // modulation
    lfo_phase: f64,
    env: PeakFollower,
    env_out: f32,
    transport: TransportInfo,
    det: PitchDetector,
    track: Track,
    // smoother coefficients
    c_r: f64,
    c_r_slow: f64,
    c_f0: f64,
    c_g_down: f64,
    c_g_up: f64,
}

impl Default for Crusher {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn db(x: f32) -> f32 {
    10f32.powf(x / 20.0)
}

impl Crusher {
    pub fn new() -> Self {
        let mut v = [0.0; NP];
        for (k, pi) in PARAMS.iter().enumerate() {
            v[k] = pi.default;
        }
        let exp = |i: usize| ExpSmoother::new(PARAMS[i].min, PARAMS[i].max, PARAMS[i].default);
        Self {
            v,
            sr: 48000.0,
            nch: 1,
            ch: Vec::new(),
            protos: Protos::new(),
            kernel: Kernel::get(),
            dry_fir: [0.0; 2 * gfx_dsp::blep::TAPS - 1],
            r_ctl: 16000.0,
            clock: Clock::default(),
            now: 0,
            quant: exp(p::QUANT),
            rate: exp(p::RATE),
            nonlin: exp(p::NONLIN),
            mix: exp(p::MIX),
            level: exp(p::LEVEL),
            high_cut: exp(p::HIGH_CUT),
            rate_shift: exp(p::RATE_SHIFT),
            jitter: exp(p::JITTER),
            mod_depth: exp(p::MOD_DEPTH),
            mod_rate: exp(p::MOD_RATE),
            dither: OnePole::new(30.0, 0.0, 1e-4),
            input: OnePole::new(20.0, 0.0, 1e-4),
            low_cut: OnePole::new(30.0, 20.0, 1e-2),
            asym: OnePole::new(30.0, 0.0, 1e-4),
            ladder: OnePole::new(30.0, 0.0, 1e-4),
            log_r: 16000f64.log2(),
            r_settled: false,
            in_g: 1.0,
            out_g: 1.0,
            r_now: 16000.0,
            designed_r: f64::NAN,
            designed_hc: f32::NAN,
            designed_lc: f32::NAN,
            ctl: 0,
            cur: 0,
            xf_on: false,
            xf_pos: 0,
            xf_len: 1,
            lfo_phase: 0.0,
            env: PeakFollower::new(5.0, 200.0),
            env_out: 0.0,
            transport: TransportInfo::default(),
            det: PitchDetector::default(),
            track: Track { f0: f64::NAN, f0_target: f64::NAN, n: 0.0, unvoiced: 0 },
            c_r: 0.0,
            c_r_slow: 0.0,
            c_f0: 0.0,
            c_g_down: 0.0,
            c_g_up: 0.0,
        }
    }

    fn exps(&mut self) -> [(&mut ExpSmoother, usize); 10] {
        [
            (&mut self.quant, p::QUANT),
            (&mut self.rate, p::RATE),
            (&mut self.nonlin, p::NONLIN),
            (&mut self.mix, p::MIX),
            (&mut self.level, p::LEVEL),
            (&mut self.high_cut, p::HIGH_CUT),
            (&mut self.rate_shift, p::RATE_SHIFT),
            (&mut self.jitter, p::JITTER),
            (&mut self.mod_depth, p::MOD_DEPTH),
            (&mut self.mod_rate, p::MOD_RATE),
        ]
    }

    fn poles(&mut self) -> [(&mut OnePole, usize); 5] {
        [(&mut self.dither, p::DITHER), (&mut self.input, p::INPUT), (&mut self.low_cut, p::LOW_CUT), (&mut self.asym, p::ASYM), (&mut self.ladder, p::LADDER)]
    }

    /// The effective virtual rate R (after shift, modulation and track), for golem's display.
    pub fn rate_now(&self) -> f32 {
        self.r_now.min(OFF_HZ) as f32
    }

    /// The dry path's low-frequency delay (samples) outside Off: the two kernels' group delay.
    pub fn dry_align(&self) -> f64 {
        self.dry_fir.iter().enumerate().map(|(i, h)| i as f64 * *h as f64).sum::<f64>() / self.dry_fir.iter().map(|h| *h as f64).sum::<f64>()
    }

    fn off_hz(&self) -> f64 {
        OFF_HZ.min(self.sr)
    }

    fn target_cfg(&self) -> Config {
        Config {
            model: self.v[p::MODEL].round().clamp(0.0, 5.0) as usize,
            law: self.v[p::LAW].round().clamp(0.0, 2.0) as usize,
            off: self.r_now >= self.off_hz() * (1.0 - 1e-6),
        }
    }

    /// R before tracking: `rate` · 2^(shift / 12) · the modulation, clamped.
    fn r_target(&self, m: f64) -> f64 {
        let r = self.rate.value() as f64 * 2f64.powf(self.rate_shift.value() as f64 / 12.0);
        let r = r * 2f64.powf(3.0 * self.mod_depth.value() as f64 / 100.0 * m);
        r.clamp(MIN_HZ, self.sr)
    }

    /// (Re)designs a path's model filters at R.
    fn design_path(protos: &Protos, path: &mut Path, r: f64, sr: f64, ramp: u32) {
        let r = r.min(sr);
        let (pre_e, post_e) = model_edges(path.cfg.model);
        match (protos.pre(path.cfg.model), pre_e) {
            (Some(pr), Some(e)) => path.pre.design(pr, e * r, sr, ramp),
            _ => path.pre.clear(),
        }
        match (protos.post(path.cfg.model), post_e) {
            (Some(pr), Some(e)) => path.post.design(pr, e * r, sr, ramp),
            _ => path.post.clear(),
        }
    }

    fn set_converters(&mut self) {
        let shape = Shape::from_index(self.v[p::QUANTIZER].round() as usize);
        let bits = self.quant.value() as f64;
        let dither = self.dither.value() as f64 / 100.0;
        let (ladder, nonlin, asym) = (self.ladder.value() as f64 / 100.0, self.nonlin.value() as f64 / 100.0, self.asym.value() as f64 / 100.0);
        for c in self.ch.iter_mut() {
            for path in c.paths.iter_mut() {
                let fmt = Format::from_index(path.cfg.law);
                path.adc.set(fmt, shape, bits, dither);
                path.dac.set(ladder, nonlin, asym);
            }
        }
    }

    fn set_tone(&mut self) {
        let sr = self.sr as f32;
        let hc = self.high_cut.value();
        let lc = self.low_cut.value();
        for c in self.ch.iter_mut() {
            if lc != self.designed_lc {
                c.low.set(SvfMode::HighPass, lc, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            }
            if hc != self.designed_hc {
                c.high[0].set(SvfMode::LowPass, hc, 0.541_196_1, 0.0, sr);
                c.high[1].set(SvfMode::LowPass, hc, 1.306_563, 0.0, sr);
            }
        }
        self.designed_hc = hc;
        self.designed_lc = lc;
    }

    fn reset_audio(&mut self) {
        let off = self.target_cfg();
        for c in self.ch.iter_mut() {
            c.low.reset();
            c.det.reset();
            c.g = 1.0;
            c.dry = [0.0; 2 * DRY_N];
            c.dpos = 0;
            for h in c.high.iter_mut() {
                h.reset();
            }
            c.last = 0.0;
            for path in c.paths.iter_mut() {
                path.reset();
                path.cfg = off;
            }
        }
        self.cur = 0;
        self.xf_on = false;
        self.xf_pos = 0;
        self.clock.reset();
        let (r, sr) = (self.r_now, self.sr);
        for c in self.ch.iter_mut() {
            for path in c.paths.iter_mut() {
                Self::design_path(&self.protos, path, r, sr, 0);
            }
        }
        self.designed_r = r;
        self.r_ctl = r;
        self.designed_hc = f32::NAN;
        self.designed_lc = f32::NAN;
        self.set_tone();
        self.set_converters();
    }

    /// The rate modulation's value: LFO −1 … 1, envelope 0 … 1.
    #[inline]
    fn modulation(&mut self, i: usize, level: f32) -> f64 {
        match self.v[p::MOD].round() as usize {
            1 => {
                let sync = Sync::from_index(self.v[p::SYNC].round() as usize);
                let beats = sync.beats();
                match beats.and_then(|b| self.transport.phase_at(i, b, 0.0, self.sr as f32)) {
                    Some(ph) => self.lfo_phase = ph,
                    None => {
                        let hz = match beats {
                            Some(b) => self.transport.bpm() / 60.0 / b,
                            None => self.mod_rate.value() as f64,
                        };
                        self.lfo_phase = (self.lfo_phase + hz / self.sr).fract();
                    }
                }
                (std::f64::consts::TAU * self.lfo_phase).sin()
            }
            2 => {
                let e = self.env.next(level);
                let floor = -20.0 - 40.0 * self.v[p::ENV_SENS] * 0.01;
                self.env_out = ((20.0 * e.max(1e-6).log10() - floor) / 30.0).clamp(0.0, 1.0);
                self.env_out as f64
            }
            _ => 0.0,
        }
    }

    /// Pitch tracking: R = N · f0 with hysteresis on N; unvoiced frames hold; after 250 ms unvoiced
    /// R glides back to its target. Returns (R wanted, slow glide).
    #[inline]
    fn tracked(&mut self, r_target: f64, mono: f32) -> (f64, bool) {
        if self.det.push(mono) {
            if let Some(hz) = self.det.hz() {
                self.track.f0_target = (hz as f64).log2();
                if self.track.f0.is_nan() {
                    self.track.f0 = self.track.f0_target;
                }
                self.track.unvoiced = 0;
            }
        }
        self.track.unvoiced += 1;
        if self.track.f0.is_nan() {
            return (r_target, false);
        }
        self.track.f0 = self.track.f0_target + (self.track.f0 - self.track.f0_target) * self.c_f0;
        if self.track.unvoiced as f64 > 0.25 * self.sr {
            return (r_target, true);
        }
        let f0 = self.track.f0.exp2();
        let ratio = r_target / f0;
        let n = self.track.n;
        // hysteresis: ±0.6 of a multiple plus 6 % (a semitone) of it, so a ±50-cent vibrato across a
        // rounding boundary keeps N at any N (±0.6 alone holds it only for N ≤ 3)
        let band = TRACK_HYST + TRACK_HYST_REL * n;
        if n < 1.0 || ratio < n - band || ratio > n + band {
            self.track.n = ratio.round().max(1.0);
        }
        ((self.track.n * f0).clamp(MIN_HZ, self.sr), false)
    }

    fn start_crossfade(&mut self, cfg: Config) {
        let o = 1 - self.cur;
        let cur = self.cur;
        let (r, sr) = (self.r_now, self.sr);
        let was_off = self.ch.first().is_some_and(|c| c.paths[cur].cfg.off);
        for c in self.ch.iter_mut() {
            let mut fresh = c.paths[cur].clone();
            fresh.cfg = cfg;
            // its first period falls back to a point sample
            fresh.boxs.reset();
            if was_off && !cfg.off {
                // the hold starts where the Off path's output is
                fresh.render.reset();
                fresh.render.set(self.kernel, c.last, 1.0);
                for _ in 0..gfx_dsp::blep::TAPS {
                    fresh.render.next();
                }
            }
            if cfg.model != c.paths[cur].cfg.model {
                fresh.pre.clear();
                fresh.post.clear();
                fresh.fir = [0.0; 2];
            }
            Self::design_path(&self.protos, &mut fresh, r, sr, 0);
            c.paths[o] = fresh;
        }
        let ms = if was_off != cfg.off { 10.0 } else { 20.0 };
        self.xf_len = ((ms * 0.001 * self.sr) as u32).max(1);
        self.xf_pos = 0;
        self.xf_on = true;
        self.set_converters();
    }
}

impl Effect for Crusher {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate as f64;
        self.nch = channels.clamp(1, MAX_CH);
        self.kernel = Kernel::get();
        self.dry_fir = self.kernel.pair_fir();
        let win = (0.025 * self.sr).round() as usize;
        self.ch = (0..self.nch)
            .map(|_| {
                let mut det = SlidingMax::new();
                det.prepare(win);
                det.set_window(win);
                Chan {
                    low: Svf::new(),
                    det,
                    g: 1.0,
                    dry: [0.0; 2 * DRY_N],
                    dpos: 0,
                    paths: [Path::new(), Path::new()],
                    high: [Svf::new(), Svf::new()],
                    last: 0.0,
                }
            })
            .collect();
        for (s, _) in self.exps() {
            s.prepare(sample_rate);
        }
        for (s, _) in self.poles() {
            s.prepare(sample_rate);
        }
        self.env.prepare(sample_rate);
        self.det.prepare_range(sample_rate, 40.0, 1500.0, true);
        let sr = self.sr;
        self.c_r = one_pole_coef(0.020 * sr);
        self.c_r_slow = one_pole_coef(0.200 / 3.0 * sr);
        self.c_f0 = one_pole_coef(0.030 * sr);
        // toward crushed: settled (99 %) in about 1 ms, so a new note is crushed within 2 ms
        self.c_g_down = one_pole_coef(TAIL_DOWN_S * sr);
        self.c_g_up = one_pole_coef(TAIL_UP_S * sr);
        self.reset();
    }

    fn reset(&mut self) {
        let v = self.v;
        for (s, i) in self.exps() {
            s.settle(v[i]);
            s.reset();
        }
        for (s, i) in self.poles() {
            s.set_target(v[i]);
            s.settle();
        }
        self.now = 0;
        self.lfo_phase = 0.0;
        self.env.set_times(v[p::ENV_ATT], v[p::ENV_REL], self.sr as f32);
        self.env.reset();
        self.env_out = 0.0;
        self.det.reset();
        self.track = Track { f0: f64::NAN, f0_target: f64::NAN, n: 0.0, unvoiced: 0 };
        // the LFO starts at phase 0 and the envelope at rest: no modulation yet
        self.r_now = self.r_target(0.0);
        self.log_r = self.r_now.log2();
        self.r_settled = false;
        self.in_g = db(self.input.value());
        self.out_g = db(self.level.value()) / self.in_g;
        self.ctl = 0;
        self.reset_audio();
    }

    fn set_param(&mut self, id: &str, value: f32) {
        let Some(i) = PARAMS.iter().position(|q| q.id == id) else { return };
        let q = &PARAMS[i];
        let v = value.clamp(q.min, q.max);
        self.v[i] = v;
        let now = self.now;
        for (s, k) in self.exps() {
            if k == i {
                s.set_target(v, now);
                return;
            }
        }
        for (s, k) in self.poles() {
            if k == i {
                s.set_target(v);
                return;
            }
        }
        if i == p::ENV_ATT || i == p::ENV_REL {
            self.env.set_times(self.v[p::ENV_ATT], self.v[p::ENV_REL], self.sr as f32);
        }
        if i == p::QUANTIZER {
            self.set_converters();
        }
    }

    fn set_transport(&mut self, transport: TransportInfo) {
        self.transport = transport;
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(self.ch.len());
        if nch == 0 {
            return;
        }
        let n = buffers[0].len();
        let sr = self.sr;
        let kernel = self.kernel;
        let tails = self.v[p::TAILS].round().clamp(0.0, 2.0) as usize;
        let track_on = self.v[p::TRACK] > 0.5;
        let mod_on = self.v[p::MOD].round() as usize != 0;
        for i in 0..n {
            // ---- controls
            let mut moving = false;
            for (s, _) in self.exps() {
                if !s.is_settled() {
                    s.next();
                    moving = true;
                }
            }
            for (s, _) in self.poles() {
                if !s.is_settled() {
                    s.next();
                    moving = true;
                }
            }
            let mut lvl = 0.0f32;
            let mut mono = 0.0f32;
            for c in 0..nch {
                lvl = lvl.max(buffers[c][i].abs());
                mono += buffers[c][i];
            }
            mono /= nch as f32;

            // ---- the virtual rate (recomputed only while something moves it)
            if moving || mod_on || track_on || !self.r_settled {
                let m = self.modulation(i, lvl);
                let r_t = self.r_target(m);
                let (want, slow) = if track_on { self.tracked(r_t, mono) } else { (r_t, false) };
                let lw = want.log2();
                let c = if slow { self.c_r_slow } else { self.c_r };
                self.log_r = lw + (self.log_r - lw) * c;
                self.r_settled = (self.log_r - lw).abs() < 1e-9;
                if self.r_settled {
                    self.log_r = lw;
                }
                self.r_now = self.log_r.exp2().min(sr);
            }

            // ---- structure: start a crossfade when the model, format or Off changes
            let cfg = self.target_cfg();
            if !self.xf_on && cfg != self.ch[0].paths[self.cur].cfg {
                self.start_crossfade(cfg);
            }

            // ---- control rate: the converters every sample while moving, filters every CTL samples
            if moving {
                self.set_converters();
                self.in_g = db(self.input.value());
                self.out_g = db(self.level.value()) / self.in_g;
            }
            if self.ctl == 0 {
                // designed for where R will be when the 16-sample ramp ends
                let r = (self.r_now * self.r_now / self.r_ctl).clamp(MIN_HZ, sr);
                self.r_ctl = self.r_now;
                if (r - self.designed_r).abs() > 1e-6 * r {
                    let (protos, ch) = (&self.protos, &mut self.ch);
                    for c in ch.iter_mut() {
                        for path in c.paths.iter_mut() {
                            Self::design_path(protos, path, r, sr, CTL);
                        }
                    }
                    self.designed_r = r;
                }
                if self.high_cut.value() != self.designed_hc || self.low_cut.value() != self.designed_lc {
                    self.set_tone();
                }
            }
            self.ctl = (self.ctl + 1) % CTL;

            // ---- the clock (shared by every channel and path)
            let ins = self.clock.tick(self.r_now / sr, self.jitter.value() as f64 / 100.0);

            let (in_g, out_g) = (self.in_g, self.out_g);
            let mix = self.mix.value() / 100.0;
            let lc_on = self.low_cut.value() > 20.5;
            let hc_on = self.high_cut.value() < 19999.0;
            let a = if self.xf_on { (self.xf_pos as f32 + 0.5) / self.xf_len as f32 } else { 0.0 };
            let cur = self.cur;
            let xf_on = self.xf_on;
            let step = self.ch[0].paths[cur].adc.zero_step();
            let any_on = !self.ch[0].paths[cur].cfg.off || (xf_on && !self.ch[0].paths[1 - cur].cfg.off);
            let dry_needed = any_on && mix < 1.0;
            for k in 0..nch {
                let ch = &mut self.ch[k];
                let dry = buffers[k][i];
                ch.dpos = if ch.dpos == 0 { DRY_N - 1 } else { ch.dpos - 1 };
                ch.dry[ch.dpos] = dry;
                ch.dry[ch.dpos + DRY_N] = dry;
                // outside Off the dry runs through the read + render pair's response (only when heard)
                let dry_late = if dry_needed {
                    let h = &ch.dry[ch.dpos..ch.dpos + self.dry_fir.len()];
                    let mut acc = 0.0f32;
                    for (a, b) in h.iter().zip(self.dry_fir.iter()) {
                        acc += a * b;
                    }
                    acc
                } else {
                    0.0
                };
                let mut x = dry * in_g;
                // the tone filters run always (an engaged filter starts from a live state)
                let xl = ch.low.process(x);
                if lc_on {
                    x = xl;
                }
                // the tails window, from the converter's input level
                let peak = ch.det.next(x) as f64;
                let gt = tail_gain(peak, step);
                if gt != ch.g {
                    let cg = if gt < ch.g { self.c_g_down } else { self.c_g_up };
                    ch.g = gt + (ch.g - gt) * cg;
                    if (ch.g - gt).abs() < 1e-9 {
                        ch.g = gt;
                    }
                }
                let conv = Conv { tails, g: ch.g };
                let w0 = ch.paths[cur].process(kernel, x, &ins, conv);
                let d0 = if ch.paths[cur].cfg.off { dry } else { dry_late };
                let (mut wet, d) = if xf_on {
                    let o = 1 - cur;
                    let w1 = ch.paths[o].process(kernel, x, &ins, conv);
                    let d1 = if ch.paths[o].cfg.off { dry } else { dry_late };
                    if ch.paths[o].cfg.off {
                        ch.last = w1;
                    }
                    (w0 + (w1 - w0) * a, d0 + (d1 - d0) * a)
                } else {
                    (w0, d0)
                };
                if ch.paths[cur].cfg.off {
                    ch.last = w0;
                }
                let h = ch.high[0].process(wet);
                let h = ch.high[1].process(h);
                if hc_on {
                    wet = h;
                }
                wet *= out_g;
                buffers[k][i] = d + (wet - d) * mix;
            }
            if self.xf_on {
                self.xf_pos += 1;
                if self.xf_pos >= self.xf_len {
                    self.xf_on = false;
                    self.cur = 1 - self.cur;
                }
            }
            self.now += 1;
        }
    }
}
