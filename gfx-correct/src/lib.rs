// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-correct — golem's pitch correction (design: README.md).
//!
//! - **Analysis** (gfx-dsp `psola::Tracker`, backward seeding on): YIN names the note and gates the
//!   voicing; the glottal epochs give one pitch per cycle (two cycles averaged in Live, three centred in
//!   Studio), stamped at the cycles' centre. YIN's range narrows to 0.6 · f while a note is tracked.
//! - **Vibrato model**: a sinusoid c + a cos ωt + b sin ωt fitted per cycle over the last 1.25 vibrato
//!   periods (4 – 8 Hz), three cycles back; present when 12 ≤ A ≤ 110 ¢ and the residual is under
//!   0.3 · A; left when the voice strays max(30 ¢, A / 2) from the prediction twice. The quantiser reads
//!   the steady pitch (vibrato removed by w_v), the correction keeps `vibrato` of it.
//! - **Quantiser**: the nearest note of `m00 … m11` ∩ `key` / `scale`, the held note favoured by
//!   `hysteresis` past the midpoint.
//! - **Law**: flex, a one-pole of τ = `transition` during a note's onset phase and `filter` after it,
//!   s = g_v · `corr` · ĉ + (`vibrato` − 1)⁺ · w_v · v_m + 100 · `offset` (cents).
//! - **Control timeline**: every control value carries the input time of the audio it describes; the
//!   engine reads it at its own audio time (the input delayed by the latency), so the correction meets
//!   its audio (Live: late only where 1.3 T > C; Studio: always aligned, each voiced onset back-dated to
//!   its first epoch).
//! - **Live** (`fastmode` on): gfx-dsp `pitch::PeriodShifter`, C = T_max / 2 + 1 ms, re-centred when the
//!   voice stops. **Studio** (`fastmode` off): gfx-dsp `psola::Reader` at D = 2 · T_max + 4 ms, r = 1 on
//!   unvoiced sound, formants kept (α = 1) unless `formant_keep` is off (α = r).
//!
//! Setup changes (`fastmode`, `register`): 10 ms fade-out, engines reset, new latency, fade-in.
//! `bypass`: the input delayed by the reported latency, 10 ms crossfade. Real-time safe, deterministic,
//! block-size independent.

#![allow(clippy::needless_range_loop)]

#[cfg(feature = "plugin")]
mod plugin;

#[cfg(test)]
mod tests;

use gfx_dsp::delay::DelayLine;
use gfx_dsp::detect::SCALES;
use gfx_dsp::pitch::PeriodShifter;
use gfx_dsp::psola::{voicing_thresholds, Reader, Tracker, REGISTERS};
use gfx_dsp::{Effect, ParamInfo};

pub const NP: usize = 28;

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::float("corr", "Correction", 0.0, 1.0, 1.0).with_exp(),
    ParamInfo::float("filter", "Filter", 0.0, 1.0, 0.1),
    ParamInfo::float("transition", "Transition", 0.0, 500.0, 0.0),
    ParamInfo::float("vibrato", "Vibrato", 0.0, 200.0, 0.0).with_exp(),
    ParamInfo::enumeration("key", "Key", 12, 0),
    ParamInfo::enumeration("scale", "Scale", 11, 0),
    ParamInfo::boolean("fastmode", "Fast Correction", true),
    ParamInfo::float("offset", "Offset", -2.0, 2.0, 0.0).with_exp(),
    ParamInfo::float("flex", "Flex", 0.0, 100.0, 0.0),
    ParamInfo::float("hysteresis", "Note Hold", 0.0, 50.0, 23.0),
    ParamInfo::boolean("formant_keep", "Formant Keep", true),
    ParamInfo::enumeration("register", "Register", 3, 1),
    ParamInfo::float("reference", "Reference", 400.0, 480.0, 440.0),
    ParamInfo::float("sens", "Sensitivity", 0.0, 100.0, 50.0),
    ParamInfo::float("thresh", "Threshold", -90.0, 0.0, -50.0),
    ParamInfo::boolean("m00", "C", true),
    ParamInfo::boolean("m01", "C#", true),
    ParamInfo::boolean("m02", "D", true),
    ParamInfo::boolean("m03", "D#", true),
    ParamInfo::boolean("m04", "E", true),
    ParamInfo::boolean("m05", "F", true),
    ParamInfo::boolean("m06", "F#", true),
    ParamInfo::boolean("m07", "G", true),
    ParamInfo::boolean("m08", "G#", true),
    ParamInfo::boolean("m09", "A", true),
    ParamInfo::boolean("m10", "A#", true),
    ParamInfo::boolean("m11", "B", true),
    ParamInfo::boolean("bypass", "Bypass", false),
];

pub mod p {
    pub const CORR: usize = 0;
    pub const FILTER: usize = 1;
    pub const TRANSITION: usize = 2;
    pub const VIBRATO: usize = 3;
    pub const KEY: usize = 4;
    pub const SCALE: usize = 5;
    pub const FASTMODE: usize = 6;
    pub const OFFSET: usize = 7;
    pub const FLEX: usize = 8;
    pub const HYSTERESIS: usize = 9;
    pub const FORMANT_KEEP: usize = 10;
    pub const REGISTER: usize = 11;
    pub const REFERENCE: usize = 12;
    pub const SENS: usize = 13;
    pub const THRESH: usize = 14;
    pub const M00: usize = 15;
    pub const BYPASS: usize = 27;
}

/// `scale` options (enum order, final: append only): golem's catalogue (Chromatic · Major · Minor),
/// then gfx-pitch's scales. `scale` k ≥ 1 is `detect::SCALES[k − 1]`.
pub const SCALE_NAMES: [&str; 11] = [
    "Chromatic",
    "Major",
    "Minor",
    "Harmonic Minor",
    "Melodic Minor",
    "Dorian",
    "Phrygian",
    "Lydian",
    "Mixolydian",
    "Major Pentatonic",
    "Minor Pentatonic",
];

/// Vibrato model: candidate rates (Hz), window (vibrato periods), lag (cycles), points per fit at most.
const VIB_RATES: [f64; 9] = [4.0, 4.5, 5.0, 5.5, 6.0, 6.5, 7.0, 7.5, 8.0];
const VIB_WINDOW: f64 = 1.25;
const VIB_LAG: usize = 3;
const VIB_POINTS: usize = 32;
/// Half-extent bounds (cents), shape bound (residual / A), exit (cents, share of A), ramps (s), the
/// shortest segment before a fit (s).
const VIB_A_MIN: f64 = 12.0;
const VIB_A_MAX: f64 = 110.0;
const VIB_SHAPE: f64 = 0.3;
const VIB_EXIT: f64 = 30.0;
const VIB_EXIT_A: f64 = 0.5;
const VIB_ON_S: f64 = 0.060;
const VIB_OFF_S: f64 = 0.020;
const VIB_SEGMENT_S: f64 = 0.25;
/// Correction weight ramps (s).
const GV_ON_S: f64 = 0.010;
const GV_OFF_S: f64 = 0.030;
/// Onset phase: ends when the pitch moves less than this (cents / s) for ONSET_SETTLE_S, or after
/// ONSET_MAX_S.
const ONSET_SLOPE: f64 = 200.0;
const ONSET_SETTLE_S: f64 = 0.030;
const ONSET_MAX_S: f64 = 0.250;
/// Flex: smoothstep from (cents) to (cents).
const FLEX_LO: f64 = 15.0;
const FLEX_HI: f64 = 60.0;
/// YIN range narrowing: the floor follows 0.6 · f.
const NARROW: f32 = 0.6;
/// Pitch history (cycles) and control queue lengths.
const HIST: usize = 512;
const QUEUE: usize = 512;
const MAX_CH: usize = 2;
/// Setup and bypass fades (s).
const SETUP_FADE_S: f32 = 0.010;
const BYPASS_FADE_S: f32 = 0.010;

#[inline]
fn idx(v: f32, n: usize) -> usize {
    (v.round().max(0.0) as usize).min(n - 1)
}

/// Reported latency (samples) of a mode and register at a rate: Live ⌈T_max / 2 + 1 ms⌉, Studio
/// ⌈2 · T_max + 4 ms⌉.
pub fn latency_samples(sample_rate: f32, live: bool, register: usize) -> usize {
    let t_max = sample_rate as f64 / REGISTERS[register.min(2)].0 as f64;
    let ms = sample_rate as f64 * 0.001;
    if live {
        (0.5 * t_max + ms).ceil() as usize
    } else {
        (2.0 * t_max + 4.0 * ms).ceil() as usize
    }
}

/// The notes allowed by the mask and `key` / `scale`, as 12 pitch-class flags (0 = C).
pub fn allowed_notes(mask: [bool; 12], key: usize, scale: usize) -> [bool; 12] {
    let mut out = mask;
    if scale >= 1 {
        let sc = SCALES[(scale - 1).min(SCALES.len() - 1)];
        let mut in_scale = [false; 12];
        for &s in sc {
            in_scale[(key as i32 + s).rem_euclid(12) as usize] = true;
        }
        for i in 0..12 {
            out[i] &= in_scale[i];
        }
    }
    out
}

/// The control at time `tau` in a queue ring, interpolated; before the head, the head's value.
fn peek_in(queue: &[Ctl; QUEUE], head: usize, len: usize, tau: f64) -> Ctl {
    let mut k = 0;
    while k + 1 < len {
        let a = queue[(head + k) % QUEUE];
        let b = queue[(head + k + 1) % QUEUE];
        if b.stamp > tau {
            if a.stamp <= tau && b.stamp > a.stamp {
                return a.lerp(&b, (tau - a.stamp) / (b.stamp - a.stamp));
            }
            return a;
        }
        k += 1;
    }
    queue[(head + len.max(1) - 1) % QUEUE]
}

/// One control value, stamped with the input time (samples) of the audio it describes.
#[derive(Clone, Copy, Debug, Default)]
struct Ctl {
    stamp: f64,
    /// the smoothed correction ĉ (cents)
    chat: f64,
    /// correction weight (voicing)
    gv: f64,
    /// the vibrato model's sinusoid times w_v (cents)
    vm: f64,
}

impl Ctl {
    fn lerp(&self, o: &Ctl, x: f64) -> Ctl {
        Ctl { stamp: self.stamp + (o.stamp - self.stamp) * x, chat: self.chat + (o.chat - self.chat) * x, gv: self.gv + (o.gv - self.gv) * x, vm: self.vm + (o.vm - self.vm) * x }
    }
}

#[derive(Clone, Copy, Debug)]
struct Model {
    f: f64,
    c: f64,
    a: f64,
    b: f64,
    t_ref: f64,
    amp: f64,
}

impl Model {
    #[inline]
    fn vib(&self, t: f64) -> f64 {
        let th = std::f64::consts::TAU * self.f * (t - self.t_ref);
        self.a * th.cos() + self.b * th.sin()
    }
}

pub struct Correct {
    v: [f32; NP],
    sr: f32,
    channels: usize,
    t: u64,
    tr: Tracker,
    live: PeriodShifter,
    readers: [Reader; MAX_CH],
    chbuf: [Vec<f32>; MAX_CH],
    dry: [DelayLine; MAX_CH],
    // the active setup and its latency
    act_live: bool,
    act_register: usize,
    latency: usize,
    setup_pending: bool,
    setup_g: f32,
    bypass_g: f32,
    allowed: [bool; 12],
    // control timeline
    queue: [Ctl; QUEUE],
    q_head: usize,
    q_len: usize,
    last_stamp: f64,
    // the per-cycle analysis
    ht: [f64; HIST],
    hp: [f64; HIST],
    h_n: usize,
    seg_start: usize,
    model: Option<Model>,
    bad: u32,
    w_v: f64,
    g_v: f64,
    chat: f64,
    held: Option<i32>,
    voiced: bool,
    onset: bool,
    onset_t0: f64,
    settle: f64,
    last_pd: f64,
    was_active: bool,
    narrowed: f32,
    recentred: bool,
    // smoothed parameters (per sample)
    corr_s: f64,
    vib_s: f64,
    off_s: f64,
    a20: f64,
    a30: f64,
    // meters
    note_m: f32,
    error_m: f32,
}

impl Default for Correct {
    fn default() -> Self {
        Self::new()
    }
}

impl Correct {
    pub fn new() -> Self {
        let v: [f32; NP] = core::array::from_fn(|i| PARAMS[i].default);
        let mut tr = Tracker::new();
        tr.set_backward_seed(true);
        Self {
            v,
            sr: 0.0,
            channels: 1,
            t: 0,
            tr,
            live: PeriodShifter::new(),
            readers: [Reader::new(), Reader::new()],
            chbuf: [Vec::new(), Vec::new()],
            dry: [DelayLine::new(), DelayLine::new()],
            act_live: true,
            act_register: 1,
            latency: 0,
            setup_pending: false,
            setup_g: 1.0,
            bypass_g: 1.0,
            allowed: [true; 12],
            queue: [Ctl::default(); QUEUE],
            q_head: 0,
            q_len: 0,
            last_stamp: 0.0,
            ht: [0.0; HIST],
            hp: [0.0; HIST],
            h_n: 0,
            seg_start: 0,
            model: None,
            bad: 0,
            w_v: 0.0,
            g_v: 0.0,
            chat: 0.0,
            held: None,
            voiced: false,
            onset: false,
            onset_t0: 0.0,
            settle: 0.0,
            last_pd: 0.0,
            was_active: false,
            narrowed: 0.0,
            recentred: true,
            corr_s: 1.0,
            vib_s: 0.0,
            off_s: 0.0,
            a20: 1.0,
            a30: 1.0,
            note_m: 0.0,
            error_m: 0.0,
        }
    }

    fn want_live(&self) -> bool {
        self.v[p::FASTMODE] >= 0.5
    }

    fn want_register(&self) -> usize {
        idx(self.v[p::REGISTER], 3)
    }

    /// 0 unvoiced / idle, 1 correcting, 2 pitched but gated by level.
    pub fn state(&self) -> u8 {
        self.tr.state()
    }

    /// The target note (MIDI number), 0 when none.
    pub fn note(&self) -> f32 {
        self.note_m
    }

    /// Steady pitch minus target (cents, ±200).
    pub fn error(&self) -> f32 {
        self.error_m
    }

    /// The Live head's offset from its centre (samples): a diagnostic.
    pub fn head_offset(&self) -> f64 {
        self.live.offset()
    }

    /// The current correction weight and smoothed correction (cents): a diagnostic.
    pub fn control(&self) -> (f64, f64, f64) {
        (self.g_v, self.chat, self.w_v)
    }

    fn update_allowed(&mut self) {
        let mut mask = [true; 12];
        for i in 0..12 {
            mask[i] = self.v[p::M00 + i] >= 0.5;
        }
        self.allowed = allowed_notes(mask, idx(self.v[p::KEY], 12), idx(self.v[p::SCALE], 11));
    }

    /// Configures the engines for the wanted setup (no allocation) and resets the analysis.
    fn apply_setup(&mut self) {
        self.act_live = self.want_live();
        self.act_register = self.want_register();
        if self.sr <= 0.0 {
            return;
        }
        let sr = self.sr as f64;
        let (lo, hi) = REGISTERS[self.act_register];
        let (t_min, t_max) = (sr / hi as f64, sr / lo as f64);
        self.tr.set_register(self.act_register);
        self.live.configure(self.sr, t_min, t_max);
        self.latency = latency_samples(self.sr, self.act_live, self.act_register);
        for r in self.readers.iter_mut() {
            r.configure(self.latency, t_max);
        }
        self.reset_analysis();
    }

    fn reset_analysis(&mut self) {
        self.tr.reset();
        self.live.reset();
        for r in self.readers.iter_mut() {
            r.reset();
        }
        for b in self.chbuf.iter_mut() {
            b.iter_mut().for_each(|x| *x = 0.0);
        }
        self.t = 0;
        self.q_head = 0;
        self.q_len = 0;
        self.last_stamp = 0.0;
        self.h_n = 0;
        self.seg_start = 0;
        self.model = None;
        self.bad = 0;
        self.w_v = 0.0;
        self.g_v = 0.0;
        self.chat = 0.0;
        self.held = None;
        self.voiced = false;
        self.onset = false;
        self.settle = 0.0;
        self.last_pd = 0.0;
        self.was_active = false;
        self.narrowed = 0.0;
        self.recentred = true;
        self.note_m = 0.0;
        self.error_m = 0.0;
        self.push_ctl(Ctl::default());
    }

    // ---- control queue -------------------------------------------------------------------------

    /// Appends a control value; a stamp earlier than queued ones (a back-dated onset) replaces them,
    /// never the head.
    fn push_ctl(&mut self, mut c: Ctl) {
        while self.q_len > 1 && self.queue[(self.q_head + self.q_len - 1) % QUEUE].stamp > c.stamp {
            self.q_len -= 1;
        }
        if self.q_len > 0 {
            let last = self.queue[(self.q_head + self.q_len - 1) % QUEUE];
            c.stamp = c.stamp.max(last.stamp);
        }
        if self.q_len == QUEUE {
            self.q_head = (self.q_head + 1) % QUEUE;
            self.q_len -= 1;
        }
        self.queue[(self.q_head + self.q_len) % QUEUE] = c;
        self.q_len += 1;
    }

    /// The control at audio time `tau`, dropping what lies wholly behind it.
    fn ctl_at(&mut self, tau: f64) -> Ctl {
        while self.q_len >= 2 && self.queue[(self.q_head + 1) % QUEUE].stamp <= tau {
            self.q_head = (self.q_head + 1) % QUEUE;
            self.q_len -= 1;
        }
        self.peek(tau)
    }

    /// The control at time `tau` (≥ the head's), without dropping anything.
    fn peek(&self, tau: f64) -> Ctl {
        peek_in(&self.queue, self.q_head, self.q_len, tau)
    }

    // ---- analysis ------------------------------------------------------------------------------

    /// The nearest allowed note (semitones from A4) to `pq` (cents from A4), the held one favoured.
    fn quantise(&self, pq: f64) -> Option<i32> {
        let hyst = self.v[p::HYSTERESIS].clamp(0.0, 50.0) as f64;
        let n0 = (pq / 100.0).round() as i32;
        let mut best: Option<(i32, f64)> = None;
        for k in n0 - 7..=n0 + 7 {
            if !self.allowed[(69 + k).rem_euclid(12) as usize] {
                continue;
            }
            let cost = (pq - 100.0 * k as f64).abs() - if Some(k) == self.held { 2.0 * hyst } else { 0.0 };
            if best.is_none_or(|(_, c)| cost < c) {
                best = Some((k, cost));
            }
        }
        best.map(|(k, _)| k)
    }

    /// One least-squares fit c + a cos + b sin at rate `f` on history points `b0 ..= e`, relative to
    /// time `t_ref`: (c, a, b, rms residual).
    fn fit(&self, f: f64, b0: usize, e: usize, t_ref: f64) -> Option<(f64, f64, f64, f64)> {
        let n = e + 1 - b0;
        let stride = n.div_ceil(VIB_POINTS).max(1);
        let mut m = [[0.0f64; 3]; 3];
        let mut r = [0.0f64; 3];
        let mut pts = 0;
        let mut i = e as i64;
        while i >= b0 as i64 {
            let k = i as usize % HIST;
            let th = std::f64::consts::TAU * f * (self.ht[k] - t_ref);
            let row = [1.0, th.cos(), th.sin()];
            for u in 0..3 {
                for w in 0..3 {
                    m[u][w] += row[u] * row[w];
                }
                r[u] += row[u] * self.hp[k];
            }
            pts += 1;
            i -= stride as i64;
        }
        if pts < 4 {
            return None;
        }
        let det = |m: &[[f64; 3]; 3]| m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
        let d = det(&m);
        if d.abs() < 1e-9 {
            return None;
        }
        let mut sol = [0.0; 3];
        for c in 0..3 {
            let mut mc = m;
            for u in 0..3 {
                mc[u][c] = r[u];
            }
            sol[c] = det(&mc) / d;
        }
        // residual
        let mut ss = 0.0;
        let mut i = e as i64;
        while i >= b0 as i64 {
            let k = i as usize % HIST;
            let th = std::f64::consts::TAU * f * (self.ht[k] - t_ref);
            let y = sol[0] + sol[1] * th.cos() + sol[2] * th.sin();
            ss += (self.hp[k] - y) * (self.hp[k] - y);
            i -= stride as i64;
        }
        Some((sol[0], sol[1], sol[2], (ss / pts as f64).sqrt()))
    }

    /// The vibrato model's step for the newest cycle (time `ts` s, pitch `pc` cents).
    fn vibrato_step(&mut self, ts: f64, pc: f64, dt: f64) {
        // 1. the voice against the model's prediction
        if let Some(m) = self.model {
            let pred = m.c + m.vib(ts);
            if (pc - pred).abs() > VIB_EXIT.max(VIB_EXIT_A * m.amp) {
                self.bad += 1;
            } else {
                self.bad = 0;
            }
            if self.bad >= 2 {
                self.model = None;
                self.bad = 0;
                self.seg_start = self.h_n.saturating_sub(2).max(self.seg_start);
            }
        }
        // 2. refit, three cycles back
        let n = self.h_n;
        if n > VIB_LAG + 1 && n - 1 - VIB_LAG >= self.seg_start {
            let e = n - 1 - VIB_LAG;
            let oldest = self.seg_start.max(n.saturating_sub(HIST));
            let t_e = self.ht[e % HIST];
            let t_s = self.ht[oldest % HIST];
            if t_e - t_s >= VIB_SEGMENT_S {
                let mut best: Option<Model> = None;
                let mut best_ratio = f64::MAX;
                for &f in VIB_RATES.iter() {
                    let len = VIB_WINDOW / f;
                    if t_e - len < t_s {
                        continue;
                    }
                    let mut b0 = e;
                    while b0 > oldest && self.ht[(b0 - 1) % HIST] >= t_e - len {
                        b0 -= 1;
                    }
                    if let Some((c, a, b, eps)) = self.fit(f, b0, e, t_e) {
                        let amp = a.hypot(b);
                        let ratio = eps / amp.max(1e-9);
                        if ratio < best_ratio {
                            best_ratio = ratio;
                            best = Some(Model { f, c, a, b, t_ref: t_e, amp });
                        }
                    }
                }
                if let Some(m) = best {
                    let (amp, ratio) = (m.amp, best_ratio);
                    if (VIB_A_MIN..=VIB_A_MAX).contains(&amp) && ratio < VIB_SHAPE {
                        self.model = Some(m);
                    } else if self.model.is_some() && (amp < 0.7 * VIB_A_MIN || ratio > 1.5 * VIB_SHAPE) {
                        self.model = None;
                    }
                }
            }
        }
        // 3. the split's weight
        if self.model.is_some() {
            self.w_v = (self.w_v + dt / VIB_ON_S).min(1.0);
        } else {
            self.w_v = (self.w_v - dt / VIB_OFF_S).max(0.0);
        }
    }

    /// One control update: a cycle pitch (cents from `reference`) stamped at input time `stamp`, or
    /// None (unvoiced). `first_mark` is the oldest epoch of a voiced onset (back-dating).
    fn update(&mut self, stamp: f64, pitch: Option<f64>, first_mark: Option<f64>) {
        let sr = self.sr as f64;
        let stamp = stamp.max(self.last_stamp);
        let dt = ((stamp - self.last_stamp) / sr).clamp(0.0, 0.1);
        self.last_stamp = stamp;
        let ts = stamp / sr;
        let Some(pc) = pitch else {
            self.voiced = false;
            self.model = None;
            self.w_v = (self.w_v - dt / VIB_OFF_S).max(0.0);
            self.g_v = (self.g_v - dt / GV_OFF_S).max(0.0);
            if self.g_v == 0.0 {
                self.note_m = 0.0;
                self.error_m = 0.0;
            }
            self.push_ctl(Ctl { stamp, chat: self.chat, gv: self.g_v, vm: 0.0 });
            return;
        };
        let onset_v = !self.voiced;
        if onset_v {
            self.voiced = true;
            self.seg_start = self.h_n;
            self.model = None;
            self.bad = 0;
            self.w_v = 0.0;
            self.held = None;
            self.onset = true;
            self.onset_t0 = ts;
            self.settle = 0.0;
            self.chat = 0.0;
        }
        let k = self.h_n % HIST;
        self.ht[k] = ts;
        self.hp[k] = pc;
        self.h_n += 1;
        self.vibrato_step(ts, pc, dt);
        let (dev, vm) = match self.model {
            Some(m) => (pc - m.c, m.vib(ts)),
            None => (0.0, 0.0),
        };
        let vib = (self.v[p::VIBRATO] as f64 * 0.01).clamp(0.0, 2.0);
        let pq = pc - self.w_v * dev;
        let pst = pc - self.w_v * vib.min(1.0) * dev;
        let q = self.quantise(pq);
        if q.is_some() && self.held.is_some() && q != self.held {
            self.onset = true;
            self.onset_t0 = ts;
            self.settle = 0.0;
        }
        self.held = q;
        // the onset phase ends on the landing
        if !onset_v && dt > 0.0 {
            let slope = (pq - self.last_pd).abs() / dt;
            if slope < ONSET_SLOPE {
                self.settle += dt;
            } else {
                self.settle = 0.0;
            }
        }
        self.last_pd = pq;
        if self.onset && (self.settle >= ONSET_SETTLE_S || ts - self.onset_t0 >= ONSET_MAX_S) {
            self.onset = false;
        }
        // the law
        let e = q.map_or(0.0, |n| 100.0 * n as f64 - pst);
        let flex = (self.v[p::FLEX] as f64 * 0.01).clamp(0.0, 1.0);
        let u = ((e.abs() - FLEX_LO) / (FLEX_HI - FLEX_LO)).clamp(0.0, 1.0);
        let ef = e * (1.0 - flex * u * u * (3.0 - 2.0 * u));
        let tau = if self.onset { self.v[p::TRANSITION] as f64 * 0.001 } else { self.v[p::FILTER] as f64 };
        let a = if tau <= 0.0 { 1.0 } else { 1.0 - (-dt / tau).exp() };
        self.chat += a * (ef - self.chat);
        let vmw = self.w_v * vm;
        if onset_v {
            // the weight ramps in over 10 ms ending at the first epoch of the phrase
            let s0 = first_mark.unwrap_or(stamp).min(stamp);
            let ramp = GV_ON_S * sr;
            self.push_ctl(Ctl { stamp: s0 - ramp, chat: self.chat, gv: 0.0, vm: 0.0 });
            self.g_v = 1.0;
            self.push_ctl(Ctl { stamp: s0, chat: self.chat, gv: 1.0, vm: vmw });
        } else {
            self.g_v = (self.g_v + dt / GV_ON_S).min(1.0);
        }
        self.push_ctl(Ctl { stamp, chat: self.chat, gv: self.g_v, vm: vmw });
        match q {
            Some(n) => {
                self.note_m = (69 + n).clamp(0, 127) as f32;
                self.error_m = (pst - 100.0 * n as f64).clamp(-200.0, 200.0) as f32;
            }
            None => {
                self.note_m = 0.0;
                self.error_m = 0.0;
            }
        }
    }

    /// A new epoch (or a newly active tracker): the readers' marks and a cycle pitch.
    fn on_marks(&mut self, activated: bool) {
        let marks = self.tr.marks();
        let n = marks.len();
        if n == 0 {
            return;
        }
        let period = self.tr.period();
        if !self.act_live {
            let from = if activated { 0 } else { n - 1 };
            for k in from..n {
                let m = self.tr.marks()[k];
                for r in self.readers.iter_mut() {
                    r.push_mark(m, period);
                }
            }
        }
        if !self.tr.gate_open() {
            return;
        }
        let sr = self.sr as f64;
        let f_lo = REGISTERS[self.act_register].0 as f64;
        let cycles = if self.act_live || sr / period < 1.15 * f_lo { 2 } else { 3 };
        let marks = self.tr.marks();
        if n < cycles + 1 {
            return;
        }
        let span = marks[n - 1] - marks[n - 1 - cycles];
        let per = span / cycles as f64;
        if (per - period).abs() > 0.3 * period || per <= 0.0 {
            return;
        }
        let stamp = 0.5 * (marks[n - 1] + marks[n - 1 - cycles]);
        let first = marks[0];
        let reference = self.v[p::REFERENCE].clamp(400.0, 480.0) as f64;
        let cents = 1200.0 * (sr / per / reference).log2();
        self.update(stamp, Some(cents), Some(first));
    }

    /// A detector analysis: YIN's range, and the unvoiced updates.
    fn on_frame(&mut self) {
        let (lo, hi) = REGISTERS[self.act_register];
        let (_, close) = voicing_thresholds(self.v[p::SENS]);
        let f = self.tr.f_det();
        if self.tr.gate_open() && f > 0.0 && self.tr.clarity() >= close {
            let floor = (NARROW * f).max(lo);
            if (floor - self.narrowed).abs() > 0.02 * floor {
                self.narrowed = floor;
                self.tr.set_detector_range(floor, hi);
            }
        } else if self.narrowed != 0.0 {
            // a fall below the narrowed floor, or the end of the note: the full range at once
            self.narrowed = 0.0;
            self.tr.set_detector_range(lo, hi);
        }
        if !(self.tr.gate_open() && self.tr.active()) && (self.voiced || self.g_v > 0.0) {
            let t_max = self.sr as f64 / lo as f64;
            self.update(self.t as f64 - t_max, None, None);
        }
    }

    fn process_inner(&mut self, buffers: &mut [&mut [f32]]) {
        let n = buffers.first().map_or(0, |b| b.len());
        let nch = buffers.len().min(MAX_CH);
        let setup_step = 1.0 / (SETUP_FADE_S * self.sr);
        let byp_step = 1.0 / (BYPASS_FADE_S * self.sr);
        let byp_target = if self.v[p::BYPASS] >= 0.5 { 1.0 } else { 0.0 };
        let corr_t = self.v[p::CORR].clamp(0.0, 1.0) as f64;
        let vib_t = (self.v[p::VIBRATO] as f64 * 0.01).clamp(0.0, 2.0);
        let off_t = self.v[p::OFFSET].clamp(-2.0, 2.0) as f64;
        let keep = self.v[p::FORMANT_KEEP] >= 0.5;
        let mask = self.tr.mask();
        for i in 0..n {
            // setup changes: fade out, reconfigure, fade in
            if !self.setup_pending && (self.want_live() != self.act_live || self.want_register() != self.act_register) {
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
            let mut x = [0.0f32; MAX_CH];
            for c in 0..nch {
                x[c] = buffers[c][i];
            }
            let mono = if nch > 1 { 0.5 * (x[0] + x[1]) } else { x[0] };
            let t = self.t;
            if nch > 1 && !self.act_live {
                for c in 0..nch {
                    self.chbuf[c][t as usize & mask] = x[c];
                }
            }
            for c in 0..nch {
                self.dry[c].push(x[c]);
            }
            let ev = self.tr.push(mono);
            let active = self.tr.active();
            let activated = active && !self.was_active;
            self.was_active = active;
            if ev.mark || activated {
                self.on_marks(activated);
            }
            if ev.frame {
                self.on_frame();
            }
            // the control at this sample's audio time
            let tau = t as f64 - self.latency as f64;
            let ctl = self.ctl_at(tau);
            self.corr_s += self.a20 * (corr_t - self.corr_s);
            self.vib_s += self.a30 * (vib_t - self.vib_s);
            self.off_s += self.a20 * (off_t - self.off_s);
            let vib_add = (self.vib_s - 1.0).max(0.0);
            let (corr_s, off_s) = (self.corr_s, self.off_s);
            let s_of = |c: &Ctl, off_w: f64| c.gv * corr_s * c.chat + vib_add * c.vm + 100.0 * off_s * off_w;
            let mut y = [0.0f32; MAX_CH];
            if self.act_live {
                let s = s_of(&ctl, 1.0);
                self.live.set_ratio((s / 1200.0).exp2());
                self.live.set_period(if active && self.tr.gate_open() { Some(self.tr.period()) } else { None });
                if ctl.gv > 0.0 {
                    self.recentred = false;
                } else if !self.recentred && s.abs() < 1e-9 {
                    self.recentred = true;
                    self.live.recentre();
                }
                y[..nch].copy_from_slice(&x[..nch]);
                self.live.process(mono, &mut y[..nch]);
            } else {
                // each grain asks for the control of its source cycle (input time): the ratio sets the
                // spacing to the next grain
                let (queue, q_head, q_len) = (&self.queue, self.q_head, self.q_len);
                let peek = |tau: f64| peek_in(queue, q_head, q_len, tau);
                let mut ctl = |src: f64, _per: f64| {
                    let cg = peek(src);
                    let r = (s_of(&cg, cg.gv) / 1200.0).exp2();
                    let alpha = if keep { 1.0 } else { (s_of(&cg, 1.0) / 1200.0).exp2() };
                    (r, alpha, 1.0f32)
                };
                for c in 0..nch {
                    let buf: &[f32] = if nch > 1 { &self.chbuf[c] } else { self.tr.history() };
                    y[c] = self.readers[c].process_with(t, buf, mask, &mut ctl);
                }
            }
            // bypass: the input delayed by the latency
            if self.bypass_g < byp_target {
                self.bypass_g = (self.bypass_g + byp_step).min(byp_target);
            } else if self.bypass_g > byp_target {
                self.bypass_g = (self.bypass_g - byp_step).max(byp_target);
            }
            let (gb, gs) = (self.bypass_g, self.setup_g);
            for c in 0..nch {
                let d = self.dry[c].tap(self.latency);
                buffers[c][i] = gs * (gb * d + (1.0 - gb) * y[c]);
            }
            self.t += 1;
        }
    }
}

impl Effect for Correct {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate;
        self.channels = channels.clamp(1, MAX_CH);
        let sr = sample_rate as f64;
        self.tr.prepare(sample_rate, 0.4);
        let len = self.tr.history().len();
        for b in self.chbuf.iter_mut() {
            *b = vec![0.0; len];
        }
        // the largest setup: Low
        let t_max_low = sr / REGISTERS[0].0 as f64;
        self.live.allocate(sample_rate, t_max_low);
        let max_lat = latency_samples(sample_rate, false, 0).max(latency_samples(sample_rate, true, 0));
        for d in self.dry.iter_mut() {
            d.prepare(max_lat + 4);
        }
        self.a20 = 1.0 - (-1.0 / (0.020 * sr)).exp();
        self.a30 = 1.0 - (-1.0 / (0.030 * sr)).exp();
        self.reset();
    }

    fn reset(&mut self) {
        self.update_allowed();
        self.tr.set_gate(self.v[p::SENS], self.v[p::THRESH]);
        self.setup_pending = false;
        self.setup_g = 1.0;
        self.apply_setup();
        for d in self.dry.iter_mut() {
            d.reset();
        }
        self.bypass_g = if self.v[p::BYPASS] >= 0.5 { 1.0 } else { 0.0 };
        self.corr_s = self.v[p::CORR].clamp(0.0, 1.0) as f64;
        self.vib_s = (self.v[p::VIBRATO] as f64 * 0.01).clamp(0.0, 2.0);
        self.off_s = self.v[p::OFFSET].clamp(-2.0, 2.0) as f64;
    }

    fn set_param(&mut self, id: &str, value: f32) {
        let Some(i) = PARAMS.iter().position(|pi| pi.id == id) else { return };
        self.v[i] = value;
        match i {
            p::KEY | p::SCALE => self.update_allowed(),
            k if (p::M00..p::M00 + 12).contains(&k) => self.update_allowed(),
            p::SENS | p::THRESH => self.tr.set_gate(self.v[p::SENS], self.v[p::THRESH]),
            _ => {}
        }
    }

    fn meter(&self, id: &str) -> Option<f32> {
        match id {
            "state" => Some(self.state() as f32),
            "note" => Some(self.note_m),
            "error" => Some(self.error_m),
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
