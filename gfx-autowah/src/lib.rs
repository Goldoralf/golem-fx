// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-autowah — golem's envelope filter (design: README.md).
//!
//! Stands for golem's `autowah` block (id `wah` kept: here it is the **sensitivity**). The Mu-Tron III /
//! Q-Tron family done right:
//! - the follower reads the playing **relative to its own long-term level** (a slow follower), so the
//!   same phrase sweeps the same way whether it arrives at −30 or −10 dBFS;
//! - low-pass / band-pass / high-pass (2 or 4 poles), a wah voice (gfx-wah's GCB-95), band-pass + dry,
//!   and a vowel filter (two formants morphing a → e → i → o → u);
//! - saturation into the filter (the Q-Tron's "chew"), dry lows for bass, LFO / sample-and-hold sources
//!   synced to the host tempo.
//!
//! Zero latency.

#![allow(clippy::needless_range_loop)]

use gfx_dsp::adaa::{Adaa1, Shaper};
use gfx_dsp::lfo::Rng;
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::transport::{Sync, TransportInfo};
use gfx_dsp::util::one_pole_coef;
use gfx_dsp::{Effect, ParamInfo};
use gfx_dsp::wah::{Voice, GCB95, MODELS};

#[cfg(feature = "plugin")]
mod plugin;

const MAX_CH: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterType {
    LowPass,
    BandPass,
    HighPass,
    Wah,
    BandPassDry,
    Vowel,
}

impl FilterType {
    pub const ALL: [FilterType; 6] = [FilterType::LowPass, FilterType::BandPass, FilterType::HighPass, FilterType::Wah, FilterType::BandPassDry, FilterType::Vowel];
}

pub static PARAMS: [ParamInfo; 17] = [
    ParamInfo::float("wah", "Sensitivity", 0.0, 1.0, 0.3),
    ParamInfo::enumeration("type", "Type", 6, 3),
    ParamInfo::enumeration("poles", "Poles", 2, 0),
    ParamInfo::enumeration("direction", "Direction", 2, 0),
    ParamInfo::float("low", "Low", 50.0, 2000.0, 250.0),
    ParamInfo::float("high", "High", 200.0, 8000.0, 2500.0),
    ParamInfo::float("peak", "Peak", 0.5, 20.0, 4.0),
    ParamInfo::float("attack", "Attack", 0.5, 100.0, 5.0),
    ParamInfo::float("release", "Release", 10.0, 2000.0, 150.0),
    ParamInfo::enumeration("response", "Response", 3, 0),
    ParamInfo::float("drive", "Drive", 0.0, 24.0, 0.0),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 100.0),
    ParamInfo::float("level", "Level", -20.0, 12.0, 0.0),
    ParamInfo::float("bass_keep", "Bass Keep", 0.0, 300.0, 0.0),
    ParamInfo::enumeration("source", "Source", 4, 0),
    ParamInfo::float("rate", "Rate", 0.05, 10.0, 1.0),
    ParamInfo::enumeration("sync", "Sync", 17, 0),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub wah: f32,
    pub ftype: FilterType,
    pub four_poles: bool,
    pub down: bool,
    pub low: f32,
    pub high: f32,
    pub peak: f32,
    pub attack: f32,
    pub release: f32,
    /// 0 fast, 1 smooth, 2 opto
    pub response: u32,
    pub drive: f32,
    pub mix: f32,
    pub level: f32,
    pub bass_keep: f32,
    /// 0 envelope, 1 LFO, 2 S&H, 3 envelope + LFO
    pub source: u32,
    pub rate: f32,
    pub sync: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            wah: 0.3,
            ftype: FilterType::Wah,
            four_poles: false,
            down: false,
            low: 250.0,
            high: 2500.0,
            peak: 4.0,
            attack: 5.0,
            release: 150.0,
            response: 0,
            drive: 0.0,
            mix: 100.0,
            level: 0.0,
            bass_keep: 0.0,
            source: 0,
            rate: 1.0,
            sync: 0,
        }
    }
}

/// Vowel formants (F1, F2) in Hz: a, e, i, o, u.
pub const VOWELS: [(f32, f32); 5] = [(800.0, 1150.0), (400.0, 1600.0), (300.0, 2300.0), (450.0, 800.0), (325.0, 700.0)];

/// Formants at position `pos` (0 = a … 1 = u), interpolated in log frequency.
pub fn formants(pos: f32) -> (f32, f32) {
    let x = pos.clamp(0.0, 1.0) * 4.0;
    let i = (x.floor() as usize).min(3);
    let t = x - i as f32;
    let lerp = |a: f32, b: f32| (a.ln() + t * (b.ln() - a.ln())).exp();
    (lerp(VOWELS[i].0, VOWELS[i + 1].0), lerp(VOWELS[i].1, VOWELS[i + 1].1))
}

#[derive(Clone, Default)]
struct Channel {
    f1: Svf,
    f2: Svf,
    wah: Voice,
    split_a: Svf,
    split_b: Svf,
    adaa: Adaa1,
}

pub struct Autowah {
    s: Settings,
    sr: f32,
    ch: [Channel; MAX_CH],
    sc_hp: Svf,
    env: f32,
    slow_db: f32,
    lfo_phase: f64,
    sh_target: f32,
    sh: f32,
    rng: Rng,
    transport: TransportInfo,
    low: OnePole,
    high: OnePole,
    peak: OnePole,
    mix: OnePole,
    level: OnePole,
    drive: OnePole,
    /// Structural changes (type, poles, direction, source) go through a 5 ms dip of the wet to the
    /// dry; the new structure starts from rest at the bottom.
    gate: LinearRamp,
    pending: Option<Settings>,
    clock: u64,
}

impl Autowah {
    pub fn new() -> Self {
        let d = Settings::default();
        Self {
            s: d,
            sr: 48000.0,
            ch: Default::default(),
            sc_hp: Svf::new(),
            env: 0.0,
            slow_db: -40.0,
            lfo_phase: 0.0,
            sh_target: 0.5,
            sh: 0.5,
            rng: Rng::new(0xA070),
            transport: TransportInfo::default(),
            low: OnePole::new(20.0, d.low.ln(), 1e-6),
            high: OnePole::new(20.0, d.high.ln(), 1e-6),
            peak: OnePole::new(20.0, d.peak, 1e-5),
            mix: OnePole::new(20.0, d.mix, 1e-5),
            level: OnePole::new(20.0, d.level, 1e-5),
            drive: OnePole::new(20.0, d.drive, 1e-5),
            gate: LinearRamp::new(5.0, 1.0),
            pending: None,
            clock: 0,
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn set(&mut self, s: Settings) {
        self.low.set_target(s.low.clamp(50.0, 2000.0).ln());
        self.high.set_target(s.high.clamp(200.0, 8000.0).ln());
        self.peak.set_target(s.peak);
        self.mix.set_target(s.mix);
        self.level.set_target(s.level);
        self.drive.set_target(s.drive);
        let structural = s.ftype != self.s.ftype || s.four_poles != self.s.four_poles || s.down != self.s.down || s.source != self.s.source;
        if structural {
            let mut keep = s;
            keep.ftype = self.s.ftype;
            keep.four_poles = self.s.four_poles;
            keep.down = self.s.down;
            keep.source = self.s.source;
            self.s = keep;
            self.pending = Some(s);
            self.gate.set_target(0.0);
        } else if let Some(p) = self.pending.as_mut() {
            // continuous changes during the dip land on the pending settings too
            let (ft, fp, dn, so) = (p.ftype, p.four_poles, p.down, p.source);
            *p = s;
            p.ftype = ft;
            p.four_poles = fp;
            p.down = dn;
            p.source = so;
            self.s = Settings { ftype: self.s.ftype, four_poles: self.s.four_poles, down: self.s.down, source: self.s.source, ..s };
        } else {
            self.s = s;
        }
    }

    /// The filter position for a level relative to the playing's long-term level. Exposed for tests.
    pub fn position_of(level_db: f32, slow_db: f32, sensitivity: f32) -> f32 {
        // the window starts 18 dB under the long-term level (lower with more sensitivity), 24 dB wide
        let floor = slow_db - 6.0 - 24.0 * sensitivity;
        ((level_db - floor) / 24.0).clamp(0.0, 1.0)
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(MAX_CH);
        let n = buffers.first().map_or(0, |b| b.len());
        let sr = self.sr;
        let (mut att_ms, mut rel_ms) = (self.s.attack.max(0.5), self.s.release.max(10.0));
        if self.s.response == 1 {
            att_ms *= 2.0;
            rel_ms *= 2.0;
        }
        let att = one_pole_coef(att_ms as f64 * 0.001 * sr as f64) as f32;
        let slow = one_pole_coef(1.5 * sr as f64) as f32;
        let rate = match Sync::from_index(self.s.sync as usize).beats() {
            Some(b) => (self.transport.bpm() / 60.0 / b) as f32,
            None => self.s.rate,
        };
        let bk = self.s.bass_keep;
        for c in self.ch.iter_mut() {
            if bk >= 20.0 {
                c.split_a.set(SvfMode::LowPass, bk, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
                c.split_b.set(SvfMode::LowPass, bk, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            }
        }
        for i in 0..n {
            self.clock += 1;
            let g = self.gate.next();
            if self.gate.is_settled() && g == 0.0 {
                if let Some(p) = self.pending.take() {
                    self.s = p;
                    for c in self.ch.iter_mut() {
                        c.f1.reset();
                        c.f2.reset();
                        c.wah.reset();
                    }
                }
                self.gate.set_target(1.0);
            }
            // detection: linked, high-passed so the low strings do not rule it
            let mut x = 0.0f32;
            for c in 0..nch {
                if buffers[c][i].abs() > x.abs() {
                    x = buffers[c][i];
                }
            }
            let d = self.sc_hp.process(x).abs();
            let pos_now = Self::position_of(20.0 * self.env.max(1e-6).log10(), self.slow_db, self.s.wah);
            // opto: the release lags more as the light dims
            let rel_eff = if self.s.response == 2 { rel_ms * (1.0 + 3.0 * (1.0 - pos_now)) } else { rel_ms };
            let rel = one_pole_coef(rel_eff as f64 * 0.001 * sr as f64) as f32;
            let k = if d > self.env { att } else { rel };
            self.env = d + (self.env - d) * k;
            let env_db = 20.0 * self.env.max(1e-6).log10();
            if env_db > -70.0 {
                self.slow_db = env_db + (self.slow_db - env_db) * slow;
            }
            let pos_env = Self::position_of(env_db, self.slow_db, self.s.wah);

            // LFO and sample & hold (deterministic: phase 0, fixed seed at reset)
            self.lfo_phase += rate as f64 / sr as f64;
            if self.lfo_phase >= 1.0 {
                self.lfo_phase -= 1.0;
                self.sh_target = self.rng.next_f64() as f32;
            }
            let lfo = (0.5 + 0.5 * (std::f64::consts::TAU * self.lfo_phase).sin()) as f32;
            self.sh += (self.sh_target - self.sh) * 0.01;
            let mut pos = match self.s.source {
                1 => lfo,
                2 => self.sh,
                3 => (pos_env + 0.3 * (lfo - 0.5)).clamp(0.0, 1.0),
                _ => pos_env,
            };
            if self.s.down {
                pos = 1.0 - pos;
            }
            let (lo, hi) = (self.low.next(), self.high.next());
            let fc = (lo + (hi - lo) * pos).exp();
            let q = self.peak.next().max(0.5);
            let mix = self.mix.next() * 0.01;
            let level = {
                let l = self.level.next();
                if l == 0.0 {
                    1.0
                } else {
                    10f32.powf(l / 20.0)
                }
            };
            let drive_db = self.drive.next();
            let kd = 10f32.powf(drive_db / 20.0);
            let (f1, f2) = formants(pos);
            let ftype = self.s.ftype;
            let four = self.s.four_poles;
            let wah_model = &MODELS[GCB95];
            for c in 0..nch {
                let x = buffers[c][i];
                let ch = &mut self.ch[c];
                // dry lows: complementary split (low + high = input exactly)
                let low = if bk >= 20.0 { ch.split_b.process(ch.split_a.process(x)) } else { 0.0 };
                let hx = x - low;
                let driven = if drive_db > 0.0 { ch.adaa.process(Shaper::Tanh, hx * kd) / kd.sqrt() } else { hx };
                let wet = match ftype {
                    FilterType::Wah => {
                        ch.wah.set(wah_model, 1.0 - pos, q / 4.0, 1.0, sr);
                        ch.wah.process(driven, (-std::f32::consts::TAU * 20.0 / sr).exp(), 0.0)
                    }
                    FilterType::Vowel => {
                        ch.f1.set(SvfMode::BandPass, f1, 8.0, 0.0, sr);
                        ch.f2.set(SvfMode::BandPass, f2, 8.0, 0.0, sr);
                        2.0 * (ch.f1.process(driven) + 0.7 * ch.f2.process(driven))
                    }
                    _ => {
                        let mode = match ftype {
                            FilterType::LowPass => SvfMode::LowPass,
                            FilterType::HighPass => SvfMode::HighPass,
                            _ => SvfMode::BandPass,
                        };
                        let y = if four {
                            ch.f1.set(mode, fc, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
                            ch.f2.set(mode, fc, q, 0.0, sr);
                            ch.f2.process(ch.f1.process(driven))
                        } else {
                            ch.f1.set(mode, fc, q, 0.0, sr);
                            ch.f1.process(driven)
                        };
                        // band-pass: the resonant output of the analogue state-variable filter (gain = Q at
                        // the peak), as in the Mu-Tron and the Q-Tron, not a unity-peak band
                        match ftype {
                            FilterType::BandPassDry => driven + y * q.sqrt(),
                            FilterType::BandPass => y * q,
                            _ => y,
                        }
                    }
                };
                let high_out = hx + (wet - hx) * mix * g;
                buffers[c][i] = (low + high_out) * level;
            }
        }
    }
}

impl Default for Autowah {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Autowah {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        for s in [&mut self.low, &mut self.high, &mut self.peak, &mut self.mix, &mut self.level, &mut self.drive] {
            s.prepare(sample_rate);
        }
        self.gate.prepare(sample_rate);
        self.sc_hp.set(SvfMode::HighPass, 120.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, sample_rate);
    }

    fn reset(&mut self) {
        for s in [&mut self.low, &mut self.high, &mut self.peak, &mut self.mix, &mut self.level, &mut self.drive] {
            s.settle();
        }
        for c in self.ch.iter_mut() {
            c.f1.reset();
            c.f2.reset();
            c.wah.reset();
            c.split_a.reset();
            c.split_b.reset();
            c.adaa.reset(Shaper::Tanh);
        }
        self.sc_hp.reset();
        self.env = 0.0;
        self.slow_db = -40.0;
        self.lfo_phase = 0.0;
        self.rng.reset();
        self.sh_target = 0.5;
        self.sh = 0.5;
        if let Some(p) = self.pending.take() {
            self.s = p;
        }
        self.gate.set_immediate(1.0);
        self.clock = 0;
    }

    fn set_param(&mut self, id: &str, v: f32) {
        let mut s = self.s;
        let idx = |v: f32, n: usize| (v.round().max(0.0) as usize).min(n - 1);
        match id {
            "wah" => s.wah = v,
            "type" => s.ftype = FilterType::ALL[idx(v, 6)],
            "poles" => s.four_poles = v >= 0.5,
            "direction" => s.down = v >= 0.5,
            "low" => s.low = v,
            "high" => s.high = v,
            "peak" => s.peak = v,
            "attack" => s.attack = v,
            "release" => s.release = v,
            "response" => s.response = idx(v, 3) as u32,
            "drive" => s.drive = v,
            "mix" => s.mix = v,
            "level" => s.level = v,
            "bass_keep" => s.bass_keep = v,
            "source" => s.source = idx(v, 4) as u32,
            "rate" => s.rate = v,
            "sync" => s.sync = idx(v, 17) as u32,
            _ => return,
        }
        self.set(s);
    }

    fn set_transport(&mut self, transport: TransportInfo) {
        self.transport = transport;
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_block(buffers);
    }
}

#[cfg(test)]
mod tests;
