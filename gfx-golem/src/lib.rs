// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-golem — golem's live vocal doubler (design: README.md).
//!
//! TD-PSOLA on a monophonic voice. The lead's glottal epochs are tracked (predicted, polarity held,
//! refined to sub-sample); each double is built from two-period grains cut on those epochs:
//! - an octave below is **phase-locked**: its grains land on the lead's own predicted epochs (one every
//!   second epoch, one every fourth for −24), so its harmonics stay coherent with the lead's;
//! - any other interval is scheduled freely at the smoothed period / r.
//!
//! Formants move by resampling the grains; the double's level is normalised over interval and formant.
//! The dry voice is never delayed. The epoch tracker, the voicing gate and the helpers live in gfx-dsp
//! `psola` (moved there bit-exact); the overlap-add writer and the scheduler stay here.

use gfx_dsp::detect::{harmony_interval, NoteTracker, SCALES};
pub use gfx_dsp::psola::{level_norm, voicing_thresholds, CLOSE_MS, HOLD_MS, MARK_LP_HZ, OCTAVE_CONFIRM_MS, OPEN_MS, REGISTERS};
use gfx_dsp::psola::{hermite, Tracker};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
pub mod plugin;

pub static PARAMS: [ParamInfo; 18] = [
    ParamInfo::float("shift", "Interval", -24.0, 24.0, -12.0).with_exp(),
    ParamInfo::float("formant", "Formant", -12.0, 12.0, 0.0).with_exp(),
    ParamInfo::float("voice1", "Double Level", -60.0, 6.0, -6.0).with_exp(),
    ParamInfo::float("lpf", "Tone", 2000.0, 16000.0, 6000.0),
    ParamInfo::float("shift2", "Interval 2", -24.0, 24.0, 12.0),
    ParamInfo::float("voice2", "Voice 2 Level", -60.0, 0.0, -60.0),
    ParamInfo::float("pan", "Voice 2 Pan", -100.0, 100.0, 50.0),
    ParamInfo::float("fine", "Fine", -50.0, 50.0, 0.0),
    ParamInfo::float("humanize", "Humanize", 0.0, 100.0, 0.0),
    ParamInfo::boolean("formant_link", "Formant Follows Interval", false),
    ParamInfo::float("glide", "Glide", 0.0, 1000.0, 0.0),
    ParamInfo::float("sens", "Sensitivity", 0.0, 100.0, 50.0),
    ParamInfo::float("thresh", "Threshold", -90.0, 0.0, -50.0),
    ParamInfo::enumeration("register", "Register", 3, 1),
    ParamInfo::enumeration("harmony", "Harmony", 2, 0),
    ParamInfo::enumeration("key", "Key", 12, 0),
    ParamInfo::enumeration("scale", "Scale", SCALES.len() as u32, 0),
    ParamInfo::float("hpf", "Low Cut", 20.0, 200.0, 40.0),
];

/// Interval slew (semitones per ms).
pub const SLEW_ST_PER_MS: f64 = 1.0;
const VOICES: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub shift: f32,
    pub formant: f32,
    pub voice1: f32,
    pub lpf: f32,
    pub shift2: f32,
    pub voice2: f32,
    pub pan: f32,
    pub fine: f32,
    pub humanize: f32,
    pub formant_link: bool,
    pub glide: f32,
    pub sens: f32,
    pub thresh: f32,
    pub register: usize,
    pub harmony: bool,
    pub key: u8,
    pub scale: usize,
    pub hpf: f32,
}

impl Default for Settings {
    fn default() -> Self {
        let v = |i: usize| PARAMS[i].default;
        Self {
            shift: v(0),
            formant: v(1),
            voice1: v(2),
            lpf: v(3),
            shift2: v(4),
            voice2: v(5),
            pan: v(6),
            fine: v(7),
            humanize: v(8),
            formant_link: false,
            glide: v(10),
            sens: v(11),
            thresh: v(12),
            register: 1,
            harmony: false,
            key: 0,
            scale: 0,
            hpf: v(17),
        }
    }
}

/// A deterministic slow random walk in −1 … 1 (humanize).
#[derive(Clone, Copy, Debug)]
struct Drift {
    seed: u32,
    a: f64,
    b: f64,
    coef: f64,
}

impl Drift {
    fn new(seed: u32) -> Self {
        Self { seed, a: 0.0, b: 0.0, coef: 0.0 }
    }
    fn prepare(&mut self, sr: f64) {
        self.coef = 1.0 - (-2.0 * std::f64::consts::PI * 0.4 / sr).exp();
    }
    #[inline]
    fn next(&mut self) -> f64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        let n = (self.seed as f64 / u32::MAX as f64) * 2.0 - 1.0;
        self.a += self.coef * (n * 40.0 - self.a);
        self.b += self.coef * (self.a - self.b);
        self.b.clamp(-1.0, 1.0)
    }
}

/// One double: its interval, its overlap-add buffer, its scheduler.
#[derive(Clone, Debug, Default)]
struct Voice {
    ola: Vec<f32>,
    /// current interval ratio (slewed in semitones)
    st: f64,
    /// next free-mode synthesis centre (absolute time)
    next: f64,
    /// marks since the last locked grain
    count: u32,
    lp: Svf,
    hp: Svf,
}

pub struct Golem {
    s: Settings,
    sr: f64,
    t: u64,
    /// history, voicing gate and epochs (gfx-dsp `psola`)
    tr: Tracker,
    // the anti-aliased copy of the input
    aabuf: Vec<f32>,
    mask: usize,
    aa: Svf,
    aa_alpha: f64,
    notes: NoteTracker,
    gain: f64,
    voices: [Voice; VOICES],
    olamask: usize,
    drift_pitch: Drift,
    drift_time: Drift,
    // smoothed controls
    voice1: f64,
    voice2: f64,
    voice1_t: f64,
    voice2_t: f64,
    formant: f64,
    pan: f64,
    lpf: f64,
    hpf: f64,
    ctl: u32,
}

impl Golem {
    pub fn new() -> Self {
        Self {
            s: Settings::default(),
            sr: 48000.0,
            t: 0,
            tr: Tracker::new(),
            aabuf: Vec::new(),
            mask: 0,
            aa: Svf::new(),
            aa_alpha: 1.0,
            notes: NoteTracker::default(),
            gain: 0.0,
            voices: [Voice::default(), Voice::default()],
            olamask: 0,
            drift_pitch: Drift::new(0x9E37_79B9),
            drift_time: Drift::new(0x85EB_CA6B),
            voice1: 0.0,
            voice2: 0.0,
            voice1_t: 0.0,
            voice2_t: 0.0,
            formant: 0.0,
            pan: 0.0,
            lpf: 6000.0,
            hpf: 40.0,
            ctl: 0,
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn set(&mut self, s: Settings) {
        self.s = s;
        self.tr.set_register(s.register);
        self.tr.set_gate(s.sens, s.thresh);
    }

    /// 0 idle, 1 tracking, 2 pitched but gated by level.
    pub fn state(&self) -> u8 {
        self.tr.state()
    }

    /// The tracked period (samples), when tracking.
    pub fn period(&self) -> Option<f64> {
        self.tr.active().then_some(self.tr.period())
    }

    fn target_st(&self, v: usize) -> f64 {
        let base = if v == 0 { self.s.shift } else { self.s.shift2 };
        let iv = if self.s.harmony {
            match self.notes.pitch_class() {
                Some(pc) => harmony_interval(pc, self.s.key, self.s.scale.min(SCALES.len() - 1), base) as f32,
                None => base,
            }
        } else {
            base
        };
        iv as f64 + self.s.fine as f64 * 0.01
    }

    /// A voice runs phase-locked when its interval is a whole octave down and nothing detunes it.
    fn locked(&self, v: usize) -> Option<u32> {
        let st = self.voices[v].st;
        if self.s.fine != 0.0 || self.s.humanize > 0.0 || self.s.harmony {
            return None;
        }
        if (st + 12.0).abs() < 1e-6 {
            Some(2)
        } else if (st + 24.0).abs() < 1e-6 {
            Some(4)
        } else {
            None
        }
    }

    fn alpha(&self, v: usize) -> f64 {
        let f = if self.s.formant_link { self.voices[v].st + self.formant } else { self.formant };
        2f64.powf(f / 12.0)
    }

    /// Writes one grain: the source grain around mark `src` (needs `src − 1` and `src + 1`), centred at
    /// absolute output time `at`, into voice `v`'s overlap-add buffer.
    fn write_grain(&mut self, v: usize, src: usize, at: f64) {
        let marks = self.tr.marks();
        let c = marks[src];
        let left = c - marks[src - 1];
        let right = marks[src + 1] - c;
        let alpha = self.alpha(v);
        // output spans: lowering formants stretches the grain to the right only (no extra delay)
        let lo = if alpha > 1.0 { left / alpha } else { left };
        let ro = right / alpha;
        let buf = if alpha > 1.0 { &self.aabuf } else { self.tr.history() };
        let g = level_norm(2f64.powf(self.voices[v].st / 12.0), alpha) as f32;
        let start = (at - lo).ceil() as i64;
        let end = (at + ro).floor() as i64;
        let now = self.t as i64;
        let om = self.olamask;
        for k in start.max(now + 1)..=end {
            let u = k as f64 - at;
            let w = if u < 0.0 {
                0.5 * (1.0 + (std::f64::consts::PI * u / lo).cos())
            } else {
                0.5 * (1.0 + (std::f64::consts::PI * u / ro).cos())
            };
            let x = hermite(buf, self.mask, c + u * alpha);
            self.voices[v].ola[(k as usize) & om] += g * (w as f32) * x;
        }
    }

    fn on_mark(&mut self) {
        let n = self.tr.marks().len();
        if n < 3 {
            return;
        }
        for v in 0..VOICES {
            if let Some(step) = self.locked(v) {
                self.voices[v].count += 1;
                if self.voices[v].count >= step {
                    self.voices[v].count = 0;
                    // the grain one period back, landed on a predicted epoch of the lead that is still
                    // ahead of the output
                    let src = n - 2;
                    let marks = self.tr.marks();
                    let last = marks[n - 1];
                    let left = marks[src] - marks[src - 1];
                    let period = self.tr.period();
                    let mut at = last + 2.0 * period;
                    while at - left <= self.t as f64 + 1.0 {
                        at += period;
                    }
                    self.write_grain(v, src, at);
                }
                self.voices[v].next = 0.0; // free mode re-seeds when it takes over
            }
        }
    }

    /// Free-mode scheduling, every sample.
    fn schedule(&mut self) {
        let n = self.tr.marks().len();
        if !self.tr.active() || n < 3 {
            return;
        }
        for v in 0..VOICES {
            if self.locked(v).is_some() {
                continue;
            }
            let r = 2f64.powf(self.voices[v].st / 12.0);
            let src = n - 2;
            let left = self.tr.marks()[src] - self.tr.marks()[src - 1];
            let alpha = self.alpha(v);
            let lo = if alpha > 1.0 { left / alpha } else { left };
            let now = self.t as f64;
            if self.voices[v].next < now + lo + 1.0 {
                if self.voices[v].next < now - 1.0 {
                    self.voices[v].next = now + lo + 1.0;
                }
                let at = self.voices[v].next + self.s.humanize as f64 * 0.01 * 0.010 * self.sr * self.drift_time.next();
                self.write_grain(v, src, at.max(now + lo + 1.0));
                self.voices[v].next += self.tr.period() / r;
            }
        }
    }

    fn controls(&mut self) {
        let a = 1.0 - (-32.0 / (0.030 * self.sr)).exp();
        let db = |x: f32| if x <= -60.0 { 0.0 } else { 10f64.powf(x as f64 / 20.0) };
        self.voice1_t = db(self.s.voice1);
        self.voice2_t = db(self.s.voice2);
        self.formant += (1.0 - (-32.0 / (0.050 * self.sr)).exp()) * (self.s.formant as f64 - self.formant);
        self.pan += a * (self.s.pan as f64 / 100.0 - self.pan);
        // the doubles' filters glide in log frequency (no burst when Tone or Low Cut jumps)
        self.lpf *= ((self.s.lpf as f64).max(1.0) / self.lpf).powf(a);
        self.hpf *= ((self.s.hpf as f64).max(1.0) / self.hpf).powf(a);
        // the anti-alias copy tracks the formant ratio when grains are read faster than real time
        let alpha = 2f64.powf(self.formant / 12.0).max(2f64.powf((self.voices[0].st.max(self.voices[1].st) + self.formant) / 12.0));
        if (alpha - self.aa_alpha).abs() > 1e-3 {
            self.aa_alpha = alpha;
            let fc = (0.45 * self.sr / alpha.max(1.0)).min(0.45 * self.sr) as f32;
            self.aa.set(SvfMode::LowPass, fc, std::f32::consts::FRAC_1_SQRT_2, 0.0, self.sr as f32);
        }
        // intervals: slewed in semitones (1 st / ms, slower with glide)
        let per = 32.0 / (0.001 * self.sr) * SLEW_ST_PER_MS;
        let per = if self.s.glide > 0.0 { per.min(24.0 * 32.0 / (self.s.glide as f64 * 0.001 * self.sr)) } else { per };
        let hum = self.s.humanize as f64 * 0.01 * 0.08;
        for v in 0..VOICES {
            let target = self.target_st(v) + hum * self.drift_pitch.next();
            let d = target - self.voices[v].st;
            self.voices[v].st += d.clamp(-per, per);
            let sr = self.sr as f32;
            self.voices[v].lp.set(SvfMode::LowPass, (self.lpf as f32).min(0.45 * sr), std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            self.voices[v].hp.set(SvfMode::HighPass, self.hpf as f32, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
        }
    }

    fn process_inner(&mut self, buffers: &mut [&mut [f32]]) {
        let n = buffers.first().map_or(0, |b| b.len());
        let nch = buffers.len();
        let a_open = 1.0 - (-1.0 / (OPEN_MS as f64 * 0.001 * self.sr)).exp();
        let a_close = 1.0 - (-1.0 / (CLOSE_MS as f64 * 0.001 * self.sr)).exp();
        let a_lvl = 1.0 - (-1.0 / (0.030 * self.sr)).exp();
        for i in 0..n {
            if self.ctl == 0 {
                self.controls();
            }
            self.ctl = (self.ctl + 1) % 32;
            let x = if nch > 1 { 0.5 * (buffers[0][i] + buffers[1][i]) } else { buffers[0][i] };
            let t = self.t as usize;
            self.aabuf[t & self.mask] = if self.aa_alpha > 1.0 { self.aa.process(x) } else { x };
            let ev = self.tr.push(x);
            if ev.frame {
                self.notes.update(if self.tr.gate_open() { Some(self.tr.f_det()) } else { None });
            }
            if ev.mark {
                self.on_mark();
            }
            self.schedule();
            // the doubles
            let target = if self.tr.gate_open() { 1.0 } else { 0.0 };
            self.gain += if target > self.gain { a_open } else { a_close } * (target - self.gain);
            let om = self.olamask;
            let mut d = [0.0f32; VOICES];
            for v in 0..VOICES {
                let s = std::mem::take(&mut self.voices[v].ola[t & om]);
                let s = self.voices[v].hp.process(s);
                d[v] = self.voices[v].lp.process(s);
            }
            // the double levels glide per sample (no zipper)
            self.voice1 += a_lvl * (self.voice1_t - self.voice1);
            self.voice2 += a_lvl * (self.voice2_t - self.voice2);
            let g = self.gain as f32;
            let d1 = d[0] * self.voice1 as f32 * g;
            let d2 = d[1] * self.voice2 as f32 * g;
            if nch > 1 {
                let p = (self.pan.clamp(-1.0, 1.0) + 1.0) * std::f64::consts::FRAC_PI_4;
                let (gl, gr) = (p.cos() as f32, p.sin() as f32);
                buffers[0][i] += d1 + d2 * gl;
                buffers[1][i] += d1 + d2 * gr;
            } else {
                buffers[0][i] += d1 + d2;
            }
            self.t += 1;
        }
    }
}

impl Default for Golem {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Golem {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate as f64;
        self.tr.set_register(self.s.register);
        self.tr.set_gate(self.s.sens, self.s.thresh);
        self.tr.prepare(sample_rate, 0.4);
        let len = self.tr.history().len();
        self.aabuf = vec![0.0; len];
        self.mask = len - 1;
        let olen = ((0.3 * self.sr) as usize).next_power_of_two();
        for v in &mut self.voices {
            v.ola = vec![0.0; olen];
        }
        self.olamask = olen - 1;
        self.drift_pitch.prepare(self.sr);
        self.drift_time.prepare(self.sr);
        self.aa_alpha = f64::NAN;
        self.reset();
    }

    fn reset(&mut self) {
        self.t = 0;
        self.tr.reset();
        self.aabuf.iter_mut().for_each(|x| *x = 0.0);
        self.aa.reset();
        self.notes.reset();
        self.gain = 0.0;
        let db = |x: f32| if x <= -60.0 { 0.0 } else { 10f64.powf(x as f64 / 20.0) };
        self.voice1 = db(self.s.voice1);
        self.voice2 = db(self.s.voice2);
        self.voice1_t = self.voice1;
        self.voice2_t = self.voice2;
        self.formant = self.s.formant as f64;
        self.pan = self.s.pan as f64 / 100.0;
        self.lpf = self.s.lpf as f64;
        self.hpf = self.s.hpf as f64;
        self.drift_pitch = Drift { seed: 0x9E37_79B9, ..self.drift_pitch };
        self.drift_time = Drift { seed: 0x85EB_CA6B, ..self.drift_time };
        self.drift_pitch.a = 0.0;
        self.drift_pitch.b = 0.0;
        self.drift_time.a = 0.0;
        self.drift_time.b = 0.0;
        for v in 0..VOICES {
            let st = self.target_st(v);
            let voice = &mut self.voices[v];
            voice.ola.iter_mut().for_each(|x| *x = 0.0);
            voice.st = st;
            voice.next = 0.0;
            voice.count = 0;
            voice.lp.reset();
            voice.hp.reset();
        }
        self.aa_alpha = f64::NAN;
        self.ctl = 0;
        self.controls();
    }

    fn set_param(&mut self, id: &str, value: f32) {
        let mut s = self.s;
        let idx = |v: f32, n: usize| (v.round().max(0.0) as usize).min(n - 1);
        match id {
            "shift" => s.shift = value,
            "formant" => s.formant = value,
            "voice1" => s.voice1 = value,
            "lpf" => s.lpf = value,
            "shift2" => s.shift2 = value,
            "voice2" => s.voice2 = value,
            "pan" => s.pan = value,
            "fine" => s.fine = value,
            "humanize" => s.humanize = value,
            "formant_link" => s.formant_link = value >= 0.5,
            "glide" => s.glide = value,
            "sens" => s.sens = value,
            "thresh" => s.thresh = value,
            "register" => s.register = idx(value, 3),
            "harmony" => s.harmony = value >= 0.5,
            "key" => s.key = idx(value, 12) as u8,
            "scale" => s.scale = idx(value, SCALES.len()),
            "hpf" => s.hpf = value,
            _ => return,
        }
        self.set(s);
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_inner(buffers);
    }
}

#[cfg(test)]
mod tests;
