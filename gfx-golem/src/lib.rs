// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-golem — golem's live vocal doubler (design: README.md).
//!
//! TD-PSOLA on a monophonic voice. The lead's glottal epochs are tracked (predicted, polarity held,
//! refined to sub-sample); each double is built from two-period grains cut on those epochs:
//! - an octave below is **phase-locked**: its grains land on the lead's own predicted epochs (one every
//!   second epoch, one every fourth for −24), so its harmonics stay coherent with the lead's;
//! - any other interval is scheduled freely at the smoothed period / r.
//! Formants move by resampling the grains; the double's level is normalised over interval and formant.
//! The dry voice is never delayed.

use gfx_dsp::detect::{harmony_interval, NoteTracker, PitchDetector, SCALES};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
mod plugin;

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

/// `register`: (lowest, highest) sung fundamental, Hz.
pub const REGISTERS: [(f32, f32); 3] = [(60.0, 500.0), (90.0, 800.0), (150.0, 1200.0)];
/// The epoch marker's low-pass (Hz).
pub const MARK_LP_HZ: f32 = 1000.0;
/// Octave-error rejection: a ×2 / ÷2 jump must last this long to be believed (ms).
pub const OCTAVE_CONFIRM_MS: f32 = 40.0;
/// Voicing: hold (ms), gain ramps (ms).
pub const HOLD_MS: f32 = 30.0;
pub const OPEN_MS: f32 = 10.0;
pub const CLOSE_MS: f32 = 30.0;
/// Interval slew (semitones per ms).
pub const SLEW_ST_PER_MS: f64 = 1.0;
const VOICES: usize = 2;
const MARKS: usize = 16;

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

/// Voicing thresholds (clarity) for `sens` 0 – 100 %: (open, close).
pub fn voicing_thresholds(sens: f32) -> (f32, f32) {
    let s = (sens / 100.0).clamp(0.0, 1.0);
    // 0 % → 0.9 / 0.8, 50 % → 0.8 / 0.65, 100 % → 0.7 / 0.5
    let lerp = |a: f32, b: f32, c: f32| if s < 0.5 { a + (b - a) * s * 2.0 } else { b + (c - b) * (s - 0.5) * 2.0 };
    (lerp(0.9, 0.8, 0.7), lerp(0.8, 0.65, 0.5))
}

/// The double's level normalisation for an interval ratio `r` and a formant ratio `alpha`: the grains'
/// overlap scales the output power by about r / alpha.
pub fn level_norm(r: f64, alpha: f64) -> f64 {
    (alpha / r).sqrt().clamp(0.25, 4.0)
}

#[inline]
fn hermite(buf: &[f32], mask: usize, pos: f64) -> f32 {
    let i = pos.floor();
    let f = (pos - i) as f32;
    let i = i as i64;
    let at = |k: i64| buf[(k as usize) & mask];
    let (y0, y1, y2, y3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
    let c0 = y1;
    let c1 = 0.5 * (y2 - y0);
    let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
    ((c3 * f + c2) * f + c1) * f + c0
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
    // history (input), its marker low-pass and its anti-aliased copy
    hist: Vec<f32>,
    lpbuf: Vec<f32>,
    aabuf: Vec<f32>,
    mask: usize,
    mark_lp: Svf,
    aa: Svf,
    aa_alpha: f64,
    gd: f64,
    det: PitchDetector,
    notes: NoteTracker,
    // voice tracking
    f_det: f32,
    octave_pending: u32,
    level: f64,
    gate_open: bool,
    agree: u32,
    hold: u32,
    gain: f64,
    pitched_quiet: bool,
    // epochs
    active: bool,
    pol: f32,
    marks: [f64; MARKS],
    nmarks: usize,
    mark_index: u64,
    period: f64,
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
    /// the detector's analysis hop (samples)
    det_hop: f64,
}

impl Golem {
    pub fn new() -> Self {
        Self {
            s: Settings::default(),
            sr: 48000.0,
            t: 0,
            hist: Vec::new(),
            lpbuf: Vec::new(),
            aabuf: Vec::new(),
            mask: 0,
            mark_lp: Svf::new(),
            aa: Svf::new(),
            aa_alpha: 1.0,
            gd: 0.0,
            det: PitchDetector::default(),
            notes: NoteTracker::default(),
            f_det: 0.0,
            octave_pending: 0,
            level: 0.0,
            gate_open: false,
            agree: 0,
            hold: 0,
            gain: 0.0,
            pitched_quiet: false,
            active: false,
            pol: 1.0,
            marks: [0.0; MARKS],
            nmarks: 0,
            mark_index: 0,
            period: 0.0,
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
            det_hop: 128.0,
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn set(&mut self, s: Settings) {
        let reg = self.s.register;
        self.s = s;
        if s.register != reg && !self.hist.is_empty() {
            let (lo, hi) = REGISTERS[s.register.min(2)];
            self.det.set_range(lo, hi);
        }
    }

    /// 0 idle, 1 tracking, 2 pitched but gated by level.
    pub fn state(&self) -> u8 {
        if self.gate_open {
            1
        } else if self.pitched_quiet {
            2
        } else {
            0
        }
    }

    /// The tracked period (samples), when tracking.
    pub fn period(&self) -> Option<f64> {
        self.active.then_some(self.period)
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
        let c = self.marks[src];
        let left = c - self.marks[src - 1];
        let right = self.marks[src + 1] - c;
        let alpha = self.alpha(v);
        // output spans: lowering formants stretches the grain to the right only (no extra delay)
        let lo = if alpha > 1.0 { left / alpha } else { left };
        let ro = right / alpha;
        let buf = if alpha > 1.0 { &self.aabuf } else { &self.hist };
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
        self.mark_index += 1;
        let n = self.nmarks;
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
                    let last = self.marks[n - 1];
                    let left = self.marks[src] - self.marks[src - 1];
                    let mut at = last + 2.0 * self.period;
                    while at - left <= self.t as f64 + 1.0 {
                        at += self.period;
                    }
                    self.write_grain(v, src, at);
                }
                self.voices[v].next = 0.0; // free mode re-seeds when it takes over
            }
        }
    }

    /// Free-mode scheduling, every sample.
    fn schedule(&mut self) {
        let n = self.nmarks;
        if !self.active || n < 3 {
            return;
        }
        for v in 0..VOICES {
            if self.locked(v).is_some() {
                continue;
            }
            let r = 2f64.powf(self.voices[v].st / 12.0);
            let src = n - 2;
            let left = self.marks[src] - self.marks[src - 1];
            let alpha = self.alpha(v);
            let lo = if alpha > 1.0 { left / alpha } else { left };
            let now = self.t as f64;
            if self.voices[v].next < now + lo + 1.0 {
                if self.voices[v].next < now - 1.0 {
                    self.voices[v].next = now + lo + 1.0;
                }
                let at = self.voices[v].next + self.s.humanize as f64 * 0.01 * 0.010 * self.sr * self.drift_time.next();
                self.write_grain(v, src, at.max(now + lo + 1.0));
                self.voices[v].next += self.period / r;
            }
        }
    }

    /// The epoch tracker, every sample.
    fn track(&mut self) {
        let t = self.t as f64;
        let tdet = self.sr / self.f_det.max(1.0) as f64;
        if !self.gate_open && self.hold == 0 {
            self.active = false;
            self.nmarks = 0;
            return;
        }
        if self.f_det <= 0.0 {
            return;
        }
        if !self.active {
            // start: polarity from the last two periods, first mark on the strongest peak of the last one
            let span = (2.0 * tdet) as i64;
            let (mut mx, mut mn) = (f32::MIN, f32::MAX);
            for k in 0..span {
                let v = self.lpbuf[((self.t as i64 - k) as usize) & self.mask];
                mx = mx.max(v);
                mn = mn.min(v);
            }
            self.pol = if mx >= -mn { 1.0 } else { -1.0 };
            let mut best = 0i64;
            let mut bv = f32::MIN;
            for k in 0..(tdet as i64) {
                let v = self.pol * self.lpbuf[((self.t as i64 - k) as usize) & self.mask];
                if v > bv {
                    bv = v;
                    best = k;
                }
            }
            self.marks[0] = t - best as f64 - self.gd;
            self.nmarks = 1;
            self.period = tdet;
            self.active = true;
            return;
        }
        let last = self.marks[self.nmarks - 1];
        let pred = last + self.period;
        let half = 0.25 * self.period;
        // the search window must be fully in the past of the low-passed signal
        if t < pred + half + self.gd + 2.0 {
            return;
        }
        let lo = (pred - half + self.gd).ceil() as i64;
        let hi = (pred + half + self.gd).floor() as i64;
        let mut best = lo;
        let mut bs = f32::MIN;
        for k in lo..=hi {
            let d = (k as f64 - self.gd - pred) / half;
            let pen = 1.0 - 0.3 * (d * d) as f32;
            let v = self.pol * self.lpbuf[(k as usize) & self.mask] * pen;
            if v > bs {
                bs = v;
                best = k;
            }
        }
        let y = |k: i64| self.pol * self.lpbuf[(k as usize) & self.mask];
        let (y0, y1, y2) = (y(best - 1), y(best), y(best + 1));
        let den = y0 - 2.0 * y1 + y2;
        let off = if den.abs() > 1e-12 { (0.5 * (y0 - y2) / den).clamp(-0.5, 0.5) } else { 0.0 } as f64;
        let m = best as f64 + off - self.gd;
        let measured = m - last;
        if (measured - tdet).abs() < 0.3 * tdet {
            self.period += 0.3 * (measured - self.period);
        }
        self.period += 0.05 * (tdet - self.period);
        if self.nmarks == MARKS {
            self.marks.copy_within(1.., 0);
            self.nmarks -= 1;
        }
        self.marks[self.nmarks] = m;
        self.nmarks += 1;
        self.on_mark();
    }

    /// The detector's verdict → octave-error rejection and the voicing gate.
    fn voicing(&mut self) {
        let f = self.det.period_hz();
        let c = self.det.clarity();
        let (lo, hi) = REGISTERS[self.s.register.min(2)];
        let in_range = f >= lo && f <= hi;
        // octave errors: a ×2 / ÷2 jump must persist before it is believed
        let frames_confirm = ((OCTAVE_CONFIRM_MS * 0.001) as f64 * self.sr / self.det_hop).ceil() as u32;
        let mut f_new = f;
        if self.f_det > 0.0 && in_range {
            let r = f / self.f_det;
            if ((r - 2.0).abs() < 0.12 || (r - 0.5).abs() < 0.03) && self.octave_pending < frames_confirm {
                self.octave_pending += 1;
                f_new = self.f_det;
            } else {
                self.octave_pending = 0;
            }
        }
        let (open_th, close_th) = voicing_thresholds(self.s.sens);
        let level_ok = self.level > 10f64.powf(self.s.thresh as f64 / 10.0);
        let pitched = in_range && c >= close_th;
        self.pitched_quiet = pitched && !level_ok;
        let hold_frames = ((HOLD_MS * 0.001) as f64 * self.sr / self.det_hop).ceil() as u32;
        if !self.gate_open {
            self.agree = if in_range && c >= open_th && level_ok { self.agree + 1 } else { 0 };
            if self.agree >= 3 {
                self.gate_open = true;
                self.hold = hold_frames;
            }
        } else if pitched && level_ok {
            self.hold = hold_frames;
        } else if self.hold > 0 {
            self.hold -= 1;
        } else {
            self.gate_open = false;
            self.agree = 0;
        }
        if in_range && (self.gate_open || c >= open_th) {
            self.f_det = f_new;
        }
        self.notes.update(if self.gate_open { Some(self.f_det) } else { None });
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
        let a_level = 1.0 - (-1.0 / (0.020 * self.sr)).exp();
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
            self.hist[t & self.mask] = x;
            self.lpbuf[t & self.mask] = self.mark_lp.process(x);
            self.aabuf[t & self.mask] = if self.aa_alpha > 1.0 { self.aa.process(x) } else { x };
            self.level += a_level * ((x as f64) * (x as f64) - self.level);
            if self.det.push(x) {
                self.voicing();
            }
            self.track();
            self.schedule();
            // the doubles
            let target = if self.gate_open { 1.0 } else { 0.0 };
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
        let need = (0.4 * self.sr) as usize;
        let len = need.next_power_of_two();
        self.hist = vec![0.0; len];
        self.lpbuf = vec![0.0; len];
        self.aabuf = vec![0.0; len];
        self.mask = len - 1;
        let olen = ((0.3 * self.sr) as usize).next_power_of_two();
        for v in &mut self.voices {
            v.ola = vec![0.0; olen];
        }
        self.olamask = olen - 1;
        self.mark_lp.set(SvfMode::LowPass, MARK_LP_HZ, std::f32::consts::FRAC_1_SQRT_2, 0.0, sample_rate);
        // second-order Butterworth low-pass group delay at low frequency: √2 / ωc
        self.gd = self.sr * std::f64::consts::SQRT_2 / (2.0 * std::f64::consts::PI * MARK_LP_HZ as f64);
        self.det.prepare_range(sample_rate, REGISTERS[0].0, REGISTERS[2].1, false);
        self.det_hop = 32.0 * (self.sr / 12000.0).round().max(1.0);
        let (lo, hi) = REGISTERS[self.s.register.min(2)];
        self.det.set_range(lo, hi);
        self.drift_pitch.prepare(self.sr);
        self.drift_time.prepare(self.sr);
        self.aa_alpha = f64::NAN;
        self.reset();
    }

    fn reset(&mut self) {
        self.t = 0;
        for b in [&mut self.hist, &mut self.lpbuf, &mut self.aabuf] {
            b.iter_mut().for_each(|x| *x = 0.0);
        }
        self.mark_lp.reset();
        self.aa.reset();
        self.det.reset();
        self.notes.reset();
        self.f_det = 0.0;
        self.octave_pending = 0;
        self.level = 0.0;
        self.gate_open = false;
        self.agree = 0;
        self.hold = 0;
        self.gain = 0.0;
        self.pitched_quiet = false;
        self.active = false;
        self.nmarks = 0;
        self.mark_index = 0;
        self.period = 0.0;
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
