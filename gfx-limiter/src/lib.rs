// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-limiter — golem's peak limiter (design: README.md).
//!
//! Stands for golem's `limiter` block (ids `threshold`, `release`, with x42-dpl's meaning: `threshold`
//! is the output ceiling, no makeup). The promise: **no output sample above the threshold**, ever.
//!
//! - Zero latency (default): an envelope (fast attack toward threshold + a small knee, program-
//!   dependent release, a held low-frequency path so bass is not ridden cycle by cycle) does the
//!   work; whatever it cannot catch on the first cycle of a transient goes to a soft clipper with
//!   antiderivative antialiasing applied **to its residual only** (exactly zero below the knee:
//!   bit-transparent when nothing is limited); a hard clamp keeps the promise.
//! - Lookahead (1 / 2 / 5 ms, reported): a moving minimum of the required gain over the lookahead,
//!   then two cascaded box filters (Signalsmith), audio delayed: no sample overs by construction, and
//!   true-peak detection becomes possible.

use gfx_dsp::delay::DelayLine;
use gfx_dsp::envelope::SlidingMax;
use gfx_dsp::oversample::Oversampler2x;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::util::one_pole_coef;
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
mod plugin;

const MAX_CH: usize = 2;
const CTL: u64 = 32;
/// Delay of the true-peak detector: the interpolated points sit 5 – 6 samples back.
const TP_LAG: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Clean,
    Punchy,
    Clip,
}

impl Style {
    pub const ALL: [Style; 3] = [Style::Clean, Style::Punchy, Style::Clip];
    pub fn from_index(i: u32) -> Self {
        *Self::ALL.get(i as usize).unwrap_or(&Style::Clean)
    }
    /// (attack ms, envelope target above the threshold in dB) of the zero-latency envelope
    fn envelope(self) -> Option<(f32, f32)> {
        match self {
            // Clean aims just under the threshold: sustained material (bass) is handled by the
            // envelope alone, the clipper only meets the first cycle of a transient
            Style::Clean => Some((0.3, -0.1)),
            // Punchy aims above it: a share of every peak goes to the clipper, attacks survive
            Style::Punchy => Some((2.0, 1.5)),
            Style::Clip => None,
        }
    }
}

/// Lookahead options, in ms (index 0 = off).
pub const LOOKAHEAD_MS: [f32; 4] = [0.0, 1.0, 2.0, 5.0];

pub static PARAMS: [ParamInfo; 9] = [
    ParamInfo::float("threshold", "Threshold", -24.0, 0.0, -6.0),
    ParamInfo::float("release", "Release", 1.0, 1000.0, 50.0),
    ParamInfo::float("gain", "Input Gain", 0.0, 24.0, 0.0).with_exp(),
    ParamInfo::enumeration("style", "Style", 3, 0),
    ParamInfo::enumeration("lookahead", "Lookahead", 4, 0),
    ParamInfo::boolean("true_peak", "True Peak", false),
    ParamInfo::boolean("auto_release", "Auto Release", true),
    ParamInfo::float("clip", "Clip Softness", 0.0, 100.0, 50.0),
    ParamInfo::float("link", "Stereo Link", 0.0, 100.0, 100.0),
];

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub threshold: f32,
    pub release: f32,
    pub gain: f32,
    pub style: Style,
    /// index into LOOKAHEAD_MS
    pub lookahead: u32,
    pub true_peak: bool,
    pub auto_release: bool,
    pub clip: f32,
    pub link: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            threshold: -6.0,
            release: 50.0,
            gain: 0.0,
            style: Style::Clean,
            lookahead: 0,
            true_peak: false,
            auto_release: true,
            clip: 50.0,
            link: 100.0,
        }
    }
}

/// The soft clipper: identity up to `k0`, then a tanh knee reaching the ceiling `c`.
#[derive(Clone, Copy, Debug)]
struct Clipper {
    k0: f64,
    c: f64,
}

impl Clipper {
    fn new(c: f32, softness: f32) -> Self {
        let c = c as f64;
        let k0 = c * 10f64.powf(-6.0 * softness.clamp(0.0, 1.0) as f64 / 20.0);
        Self { k0, c }
    }

    /// Antiderivative of the residual r(x) = f(x) − x: exactly 0 for |x| ≤ k0.
    #[inline]
    fn residual_antiderivative(&self, x: f64) -> f64 {
        let a = x.abs();
        if a <= self.k0 {
            return 0.0;
        }
        let d = self.c - self.k0;
        let f_int = if d < 1e-9 {
            // hard clip: F(a) = c·a − c²/2
            self.c * a - 0.5 * self.c * self.c
        } else {
            let u = (a - self.k0) / d;
            let ln_cosh = u + (-2.0 * u).exp().ln_1p() - std::f64::consts::LN_2;
            0.5 * self.k0 * self.k0 + self.k0 * (a - self.k0) + d * d * ln_cosh
        };
        f_int - 0.5 * a * a
    }

    #[inline]
    fn residual(&self, x: f64) -> f64 {
        let a = x.abs();
        if a <= self.k0 {
            return 0.0;
        }
        let d = self.c - self.k0;
        let f = if d < 1e-9 { self.c } else { self.k0 + d * ((a - self.k0) / d).tanh() };
        (f - a) * x.signum()
    }
}

#[derive(Clone, Default)]
struct Channel {
    lp: Svf,
    clip_x1: f64,
    clip_r1: f64,
    delay: DelayLine,
    /// Clip style: the clipper runs at 2× (minimum phase, unreported group delay of a few samples).
    os: Oversampler2x,
    os_x1: f64,
    os_r1: f64,
}

pub struct Limiter {
    s: Settings,
    sr: f32,
    ch: [Channel; MAX_CH],
    gain_db: ExpSmoother,
    threshold: OnePole,
    clip: OnePole,
    link: OnePole,
    // zero-latency envelope (linear gain ≤ 1)
    env: f32,
    sustain: f32,
    lf_max: SlidingMax,
    // lookahead: `la_len` = the lookahead itself, `delay_len` = audio delay = reported latency
    la_len: usize,
    delay_len: usize,
    held: SlidingMax,
    rel_env: f32,
    box1: BoxFilter,
    box2: BoxFilter,
    tp_hist: [[f32; 12]; MAX_CH],
    // control rate
    att: f32,
    rel: f32,
    rel_slow: f32,
    sustain_coef: f32,
    clock: u64,
    gr_meter: f32,
    /// Style changes crossfade (10 ms) between the old path and the new one.
    style_fade: LinearRamp,
    prev_style: Style,
    /// A lookahead change moves the audio in time: the output dips (5 ms out, switch, 5 ms in)
    /// instead of jumping. `la_pending` = a reconfiguration waiting for the dip's bottom.
    la_gate: LinearRamp,
    la_pending: bool,
}

/// Moving average of a reduction (1 − gain) with an exact zero when the window holds only zeros.
#[derive(Clone, Default)]
struct BoxFilter {
    buf: Vec<f64>,
    pos: usize,
    sum: f64,
    nonzero: usize,
}

impl BoxFilter {
    fn prepare(&mut self, len: usize) {
        self.buf = vec![0.0; len.max(1)];
        self.reset();
    }
    fn reset(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
        self.pos = 0;
        self.sum = 0.0;
        self.nonzero = 0;
    }
    #[inline]
    fn next(&mut self, v: f64) -> f64 {
        let old = self.buf[self.pos];
        if old != 0.0 {
            self.nonzero -= 1;
        }
        if v != 0.0 {
            self.nonzero += 1;
        }
        self.buf[self.pos] = v;
        self.pos = (self.pos + 1) % self.buf.len();
        self.sum += v - old;
        if self.nonzero == 0 {
            self.sum = 0.0;
            0.0
        } else {
            (self.sum / self.buf.len() as f64).max(0.0)
        }
    }
}

/// 4× interpolation taps (windowed sinc, 12 per phase) for the true-peak detector (ITU-R BS.1770
/// style): the values at ¼, ½, ¾ between history[5] and history[6].
fn tp_taps() -> [[f32; 12]; 3] {
    let mut t = [[0.0f32; 12]; 3];
    for (p, row) in t.iter_mut().enumerate() {
        let frac = (p + 1) as f64 / 4.0;
        for (k, v) in row.iter_mut().enumerate() {
            // tap k multiplies history[k] (k = 0 newest); the interpolated point sits at 5 + frac
            // samples back... expressed as distance from tap k
            let x = (5.0 + frac) - k as f64;
            let sinc = if x.abs() < 1e-9 { 1.0 } else { (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x) };
            let w = 0.5 + 0.5 * (std::f64::consts::PI * (x / 6.5)).cos();
            *v = (sinc * if x.abs() < 6.5 { w } else { 0.0 }) as f32;
        }
    }
    t
}

impl Limiter {
    pub fn new() -> Self {
        let d = Settings::default();
        Self {
            s: d,
            sr: 48000.0,
            ch: Default::default(),
            gain_db: ExpSmoother::new(0.0, 24.0, d.gain),
            threshold: OnePole::new(20.0, d.threshold, 1e-6),
            clip: OnePole::new(20.0, d.clip, 1e-5),
            link: OnePole::new(20.0, d.link, 1e-5),
            env: 1.0,
            sustain: 0.0,
            lf_max: SlidingMax::new(),
            la_len: 0,
            delay_len: 0,
            held: SlidingMax::new(),
            rel_env: 0.0,
            box1: BoxFilter::default(),
            box2: BoxFilter::default(),
            tp_hist: [[0.0; 12]; MAX_CH],
            att: 0.0,
            rel: 0.0,
            rel_slow: 0.0,
            sustain_coef: 0.0,
            clock: 0,
            gr_meter: 0.0,
            style_fade: LinearRamp::new(10.0, 1.0),
            prev_style: d.style,
            la_gate: LinearRamp::new(5.0, 1.0),
            la_pending: false,
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn gain_reduction_db(&self) -> f32 {
        self.gr_meter
    }

    /// Reported latency for the current lookahead setting, in samples.
    pub fn latency_samples(&self) -> usize {
        self.delay_len
    }

    fn la_samples(&self, idx: u32) -> usize {
        let ms = LOOKAHEAD_MS[(idx as usize).min(3)];
        // even, so the two box filters split it exactly
        (((ms * 0.001 * self.sr) as usize) / 2) * 2
    }

    pub fn set(&mut self, s: Settings) {
        if s.gain != self.s.gain {
            self.gain_db.set_target(s.gain.clamp(0.0, 24.0), self.clock);
        }
        self.threshold.set_target(s.threshold.min(0.0));
        self.clip.set_target(s.clip);
        self.link.set_target(s.link);
        let la_changed = s.lookahead != self.s.lookahead || s.true_peak != self.s.true_peak;
        if s.style != self.s.style {
            self.prev_style = self.s.style;
            self.style_fade.set_immediate(0.0);
            self.style_fade.set_target(1.0);
            if s.style == Style::Clip {
                // entering the 2× path: it starts from silence, its warm-up sits under the fade
                // (leaving it, it carries on from its running state while it fades out)
                for c in self.ch.iter_mut() {
                    c.os.reset();
                    c.os_x1 = 0.0;
                    c.os_r1 = 0.0;
                }
            }
        }
        self.s = s;
        if la_changed {
            // latency changes only at preset level; the host re-aligns. The switch happens at the
            // bottom of a 5 ms dip.
            self.la_pending = true;
            self.la_gate.set_target(0.0);
        }
    }

    /// Alignment (Signalsmith): two boxes of h = la / 2 make a ramp of 2h − 1 samples; the moving
    /// minimum holds for la + 2; the audio is delayed by la plus the detector's own lag. Then, at the
    /// moment a sample leaves the delay, the smoothed gain only averages held values that cover it.
    fn configure_lookahead(&mut self) {
        self.la_len = self.la_samples(self.s.lookahead);
        let lag = if self.la_len > 0 && self.s.true_peak { TP_LAG } else { 0 };
        self.delay_len = if self.la_len > 0 { self.la_len + lag } else { 0 };
        let half = (self.la_len / 2).max(1);
        self.box1.prepare(half);
        self.box2.prepare(half);
        self.held.reset();
        self.held.set_window(self.la_len + 2);
        // the delay line is fed all the time (even with the lookahead off), so after a change it
        // already holds the recent past: the output comes back from the dip with real audio
        self.rel_env = 0.0;
    }

    fn control(&mut self) {
        let sr = self.sr as f64;
        let att_ms = self.s.style.envelope().map_or(0.3, |e| e.0);
        self.att = one_pole_coef(att_ms as f64 * 0.001 * sr) as f32;
        self.rel = one_pole_coef(self.s.release.max(1.0) as f64 * 0.001 * sr) as f32;
        self.rel_slow = one_pole_coef(self.s.release.max(1.0) as f64 * 5.0 * 0.001 * sr) as f32;
        self.sustain_coef = one_pole_coef(0.5 * sr) as f32;
        for c in self.ch.iter_mut() {
            c.lp.set(SvfMode::LowPass, 500.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, self.sr);
        }
    }

    #[inline]
    fn release_coef(&self) -> f32 {
        if self.s.auto_release {
            // fast after short peaks, slow after sustained limiting
            self.rel + (self.rel_slow - self.rel) * self.sustain
        } else {
            self.rel
        }
    }

    #[allow(clippy::needless_range_loop)]
    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(MAX_CH);
        let n = buffers.first().map_or(0, |b| b.len());
        let taps = tp_taps();
        let mut meter = 0.0f32;
        for i in 0..n {
            if self.clock.is_multiple_of(CTL) {
                for _ in 0..CTL {
                    self.threshold.next();
                    self.clip.next();
                    self.link.next();
                }
                self.control();
            }
            self.clock += 1;
            let g_in = {
                let db = self.gain_db.next();
                if db == 0.0 { 1.0 } else { 10f32.powf(db * 0.05) }
            };
            let ceiling = 10f32.powf(self.threshold.value() * 0.05);
            // Clean: the envelope keeps sustained material just under the ceiling, so the clipper is a
            // near-hard safety above that (it must not colour every bass peak). Punchy and Clip: the
            // `clip` softness sets a knee starting up to 6 dB under the ceiling.
            let softness = if self.s.style == Style::Clean { 0.02 } else { self.clip.value() * 0.01 };
            let clipper = Clipper::new(ceiling, softness);
            let clip_soft = self.clip.value() * 0.01;
            let link = self.link.value() * 0.01;

            // input, scrubbed and driven
            let mut x = [0.0f32; MAX_CH];
            for c in 0..nch {
                let v = buffers[c][i];
                x[c] = if v.is_finite() { v * g_in } else { 0.0 };
            }

            // required gain per channel (≤ 1)
            let env_target_db = self.s.style.envelope().map_or(-0.1, |e| e.1);
            // While the threshold falls or the input gain rises, the envelope aims ahead of them
            // (30 % and 50 % of the way to their targets): its short attack then never lags behind the
            // ceiling, and the clipper is not left to do the descent.
            let thr_lead = (self.s.threshold.min(0.0) - self.threshold.value()).min(0.0) * 0.3;
            let gain_lead = (self.gain_db.target() - self.gain_db.value()).max(0.0) * 0.5;
            let target_level = ceiling * 10f32.powf((env_target_db.min(60.0) + thr_lead - gain_lead) * 0.05);
            let mut need = [1.0f32; MAX_CH];
            let mut lf_need = 1.0f32;
            for c in 0..nch {
                let mut peak = x[c].abs();
                if self.la_len > 0 && self.s.true_peak {
                    let h = &mut self.tp_hist[c];
                    h.rotate_right(1);
                    h[0] = x[c];
                    for row in taps.iter() {
                        let v: f32 = row.iter().zip(h.iter()).map(|(a, b)| a * b).sum();
                        peak = peak.max(v.abs());
                    }
                }
                let reference = if self.la_len > 0 { ceiling } else { target_level };
                need[c] = if peak > reference { reference / peak } else { 1.0 };
                let lf = self.ch[c].lp.process(x[c]).abs();
                if self.s.auto_release && lf > target_level {
                    lf_need = lf_need.min(target_level / lf);
                }
            }
            // the low-frequency path is held for one LF period: bass is not ridden cycle by cycle
            let lf_red = self.lf_max.next(1.0 - lf_need);
            let nmin = need[..nch].iter().cloned().fold(1.0f32, f32::min);
            let required = |c: usize| (link * nmin + (1.0 - link) * need[c]).min(1.0 - lf_red);
            let req = required(0).min(if nch > 1 { required(1) } else { 1.0 });

            // envelope
            let gain = if self.la_len > 0 {
                // lookahead: moving minimum over the window, release, then two boxes
                let red = self.held.next(1.0 - req);
                let rc = self.release_coef();
                self.rel_env = if red > self.rel_env { red } else { red + (self.rel_env - red) * rc };
                let b = self.box2.next(self.box1.next(self.rel_env as f64));
                (1.0 - b) as f32
            } else {
                // tracked in every style (Clip applies none of it), so a style change starts from a
                // sensible state
                let rc = self.release_coef();
                let target = req;
                self.env = if target < self.env { target + (self.env - target) * self.att } else { target + (self.env - target) * rc };
                if (self.env - 1.0).abs() < 1e-7 && target == 1.0 {
                    self.env = 1.0;
                }
                if self.s.style == Style::Clip && self.style_fade.is_settled() { 1.0 } else { self.env }
            };
            let fade = self.style_fade.next();
            let la_g = self.la_gate.next();
            if self.la_pending && self.la_gate.is_settled() {
                self.la_pending = false;
                self.configure_lookahead();
                self.la_gate.set_target(1.0);
            }
            let busy = if req < 0.98 { 1.0 } else { 0.0 };
            self.sustain = busy + (self.sustain - busy) * self.sustain_coef;
            meter = meter.max(-20.0 * gain.max(1e-9).log10());

            for c in 0..nch {
                let ch = &mut self.ch[c];
                ch.delay.push(x[c]);
                let v = if self.la_len > 0 { ch.delay.tap(self.delay_len) } else { x[c] };
                let g_env = if gain == 1.0 { v } else { v * gain };
                let clip_path = |ch: &mut Channel, g: f32| -> f32 {
                    // at 2×, under the ceiling by the down-sampler's worst overshoot on a clipped
                    // wave (measured ~22 %): the final clamp then stays idle, which is what keeps
                    // the aliasing down. Clip-style peaks land between −1.7 dB and 0 dB under the
                    // threshold, never above.
                    let cl = Clipper::new(ceiling * 0.82, clip_soft);
                    let up = ch.os.up(g);
                    let mut o = [0.0f32; 2];
                    for j in 0..2 {
                        let xd = up[j] as f64;
                        let r_int = cl.residual_antiderivative(xd);
                        let dlt = xd - ch.os_x1;
                        let res = if dlt.abs() < 1e-9 { cl.residual(0.5 * (xd + ch.os_x1)) } else { (r_int - ch.os_r1) / dlt };
                        // the ADAA residual is centred half a sample back: so is the linear part
                        let xh = 0.5 * (xd + ch.os_x1);
                        ch.os_x1 = xd;
                        ch.os_r1 = r_int;
                        o[j] = (xh + res) as f32;
                    }
                    ch.os.down(o)
                };
                let base_path = |ch: &mut Channel, g: f32| -> f32 {
                    // ADAA on the clipper's residual: exactly zero below the knee
                    let xd = g as f64;
                    let r_int = clipper.residual_antiderivative(xd);
                    let dlt = xd - ch.clip_x1;
                    let res = if r_int == 0.0 && ch.clip_r1 == 0.0 {
                        0.0
                    } else if dlt.abs() < 1e-9 {
                        clipper.residual(0.5 * (xd + ch.clip_x1))
                    } else {
                        (r_int - ch.clip_r1) / dlt
                    };
                    ch.clip_x1 = xd;
                    ch.clip_r1 = r_int;
                    if res == 0.0 { g } else { (xd + res) as f32 }
                };
                let run = |style: Style, ch: &mut Channel| -> f32 {
                    if style == Style::Clip { clip_path(ch, v) } else { base_path(ch, g_env) }
                };
                let y = if fade < 1.0 {
                    let old = run(self.prev_style, ch);
                    let new = run(self.s.style, ch);
                    old + (new - old) * fade
                } else {
                    run(self.s.style, ch)
                };
                let y = if la_g < 1.0 { y * la_g } else { y };
                // the promise
                buffers[c][i] = y.clamp(-ceiling, ceiling);
            }
        }
        self.gr_meter = meter;
    }
}

impl Default for Limiter {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Limiter {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        self.gain_db.prepare(sample_rate);
        self.style_fade.prepare(sample_rate);
        self.la_gate.prepare(sample_rate);
        for s in [&mut self.threshold, &mut self.clip, &mut self.link] {
            s.prepare(sample_rate);
        }
        let max_la = (0.0051 * sample_rate) as usize + TP_LAG + 4;
        for c in self.ch.iter_mut() {
            c.delay.prepare(max_la);
        }
        self.held.prepare(max_la);
        // LF hold: one period of 30 Hz
        self.lf_max.prepare((0.034 * sample_rate) as usize);
        self.lf_max.set_window((0.033 * sample_rate) as usize);
        self.configure_lookahead();
    }

    fn reset(&mut self) {
        self.gain_db.reset();
        for s in [&mut self.threshold, &mut self.clip, &mut self.link] {
            s.settle();
        }
        for c in self.ch.iter_mut() {
            c.lp.reset();
            c.clip_x1 = 0.0;
            c.clip_r1 = 0.0;
            c.delay.reset();
            c.os.reset();
            c.os_x1 = 0.0;
            c.os_r1 = 0.0;
        }
        self.env = 1.0;
        self.sustain = 0.0;
        self.style_fade.set_immediate(1.0);
        self.prev_style = self.s.style;
        if self.la_pending {
            self.la_pending = false;
            self.configure_lookahead();
        }
        self.la_gate.set_immediate(1.0);
        self.lf_max.reset();
        self.held.reset();
        self.box1.reset();
        self.box2.reset();
        self.rel_env = 0.0;
        self.tp_hist = [[0.0; 12]; MAX_CH];
        self.clock = 0;
        self.gr_meter = 0.0;
        self.control();
    }

    fn set_param(&mut self, id: &str, v: f32) {
        let mut s = self.s;
        match id {
            "threshold" => s.threshold = v,
            "release" => s.release = v,
            "gain" => s.gain = v,
            "style" => s.style = Style::from_index(v.round().max(0.0) as u32),
            "lookahead" => s.lookahead = v.round().clamp(0.0, 3.0) as u32,
            "true_peak" => s.true_peak = v >= 0.5,
            "auto_release" => s.auto_release = v >= 0.5,
            "clip" => s.clip = v,
            "link" => s.link = v,
            _ => return,
        }
        self.set(s);
    }

    fn latency(&self) -> usize {
        self.delay_len
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_block(buffers);
    }
}

#[cfg(test)]
mod tests;
