// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-comp — golem's compressor (design: README.md).
//!
//! Stands for golem's `comp` block (ids `thresh`, `ratio`, `makeup`, `attack`, `release` kept).
//! Five characters share **one static curve** (thresh, ratio, knee): a steady tone comes out at the same
//! level in all of them; only the dynamics and the colour change.
//!
//! - Feed-forward (Clean, Bus): gain computer on the detected input level, ballistics on the gain
//!   reduction in dB. Clean runs a peak path (6 dB above the RMS threshold, fast release) and an RMS
//!   path (program-dependent release), Kotelnikov-style.
//! - Feedback (FET, Opto, Pedal): the gain computer reads the **output** level, y = x + g (dB), through
//!   the inverse of the static curve, so the loop settles exactly on the same curve. The loop is solved
//!   implicitly each sample (Newton), stable at any attack and ratio.
//! - Colour: an antiderivative-antialiased shaper inside 2× minimum-phase oversampling, crossfaded in
//!   only when the character or `drive` asks for it; the dry of `mix` goes through the same filters.
//!
//! Levels are **RMS-calibrated**, as ReaComp's and most compressors' thresholds: an RMS detector reads
//! the RMS, a peak detector reads peak − 3 dB, so a steady sine reads its RMS in every character (golem's
//! presets were set by ear on ReaComp's RMS detector; measured by the rack session 2026-11-12: a
//! peak-calibrated reading compressed 2.6 dB harder at the same settings). Zero latency.

use gfx_dsp::adaa::{Adaa1, HalfSample, Shaper};
use gfx_dsp::envelope::SlidingMax;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::lfo::Rng;
use gfx_dsp::oversample::Oversampler2x;
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::util::one_pole_coef;
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
mod plugin;

const MAX_CH: usize = 2;
const CTL: u64 = 32;
/// Peak − RMS of a sine (dB): peak detectors are calibrated to read a sine's RMS.
const SINE_CREST_DB: f32 = 3.010_3;
/// Longest detector window of [`Comp::set_detector_low_hz`] (s), and the shortest (gfx-comp's own).
const LOW_WINDOW_MAX: f32 = 0.040;
const LOW_WINDOW_MIN: f32 = 0.005;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Character {
    Clean,
    Bus,
    Fet,
    Opto,
    Pedal,
}

impl Character {
    pub const ALL: [Character; 5] = [Character::Clean, Character::Bus, Character::Fet, Character::Opto, Character::Pedal];

    pub fn from_index(i: u32) -> Self {
        *Self::ALL.get(i as usize).unwrap_or(&Character::Clean)
    }

    /// Intrinsic colour amount (0 = none) and shaper.
    fn colour(self) -> (f32, Shaper) {
        match self {
            Character::Clean => (0.0, Shaper::Tanh),
            Character::Bus => (0.12, Shaper::Asym(0.15)),
            Character::Fet => (0.25, Shaper::Tanh),
            Character::Opto => (0.18, Shaper::Asym(0.35)),
            Character::Pedal => (0.3, Shaper::Asym(0.2)),
        }
    }
}

pub static PARAMS: [ParamInfo; 14] = [
    ParamInfo::float("thresh", "Threshold", -60.0, 0.0, -18.0).with_exp(),
    ParamInfo::float("ratio", "Ratio", 1.0, 30.0, 4.0),
    ParamInfo::float("makeup", "Makeup", -12.0, 24.0, 3.0).with_exp(),
    ParamInfo::float("attack", "Attack", 0.02, 200.0, 15.0),
    ParamInfo::float("release", "Release", 5.0, 3000.0, 150.0),
    ParamInfo::enumeration("character", "Character", 5, 0),
    ParamInfo::float("knee", "Knee", 0.0, 24.0, 3.0),
    ParamInfo::boolean("auto_release", "Auto Release", true),
    ParamInfo::float("sc_hpf", "Sidechain Low Cut", 0.0, 500.0, 0.0),
    ParamInfo::float("range", "Range", 0.0, 60.0, 60.0),
    ParamInfo::float("drive", "Drive", 0.0, 100.0, 0.0),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::float("link", "Stereo Link", 0.0, 100.0, 100.0),
    ParamInfo::boolean("sc_listen", "Sidechain Listen", false),
];

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub thresh: f32,
    pub ratio: f32,
    pub makeup: f32,
    pub attack: f32,
    pub release: f32,
    pub character: Character,
    pub knee: f32,
    pub auto_release: bool,
    pub sc_hpf: f32,
    pub range: f32,
    pub drive: f32,
    pub mix: f32,
    pub link: f32,
    pub sc_listen: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            thresh: -18.0,
            ratio: 4.0,
            makeup: 3.0,
            attack: 15.0,
            release: 150.0,
            character: Character::Clean,
            knee: 3.0,
            auto_release: true,
            sc_hpf: 0.0,
            range: 60.0,
            drive: 0.0,
            mix: 100.0,
            link: 100.0,
            sc_listen: false,
        }
    }
}

/// The static curve: output level for an input level (dB), Giannoulis' quadratic soft knee.
#[inline]
pub fn static_curve(x: f32, t: f32, r: f32, w: f32) -> f32 {
    let d = x - t;
    if 2.0 * d < -w {
        x
    } else if w > 0.0 && 2.0 * d.abs() <= w {
        x + (1.0 / r - 1.0) * (d + w * 0.5).powi(2) / (2.0 * w)
    } else {
        t + d / r
    }
}

/// Gain (dB, ≤ 0) as a function of the **output** level y, such that y = x + g lands on the static
/// curve: the inverse of the curve, used by the feedback characters. Returns (g, dg/dy).
#[inline]
fn feedback_gain(y: f32, t: f32, r: f32, w: f32) -> (f32, f32) {
    let knee_lo = t - w * 0.5;
    let knee_hi = t + w * 0.5 / r;
    if y <= knee_lo {
        (0.0, 0.0)
    } else if w <= 0.0 || y >= knee_hi {
        ((1.0 - r) * (y - t), 1.0 - r)
    } else {
        // y − knee_lo = u + a u², u = x − knee_lo, a = (1/r − 1) / (2w) < 0
        let a = (1.0 / r - 1.0) / (2.0 * w);
        let v = y - knee_lo;
        let disc = (1.0 + 4.0 * a * v).max(0.0).sqrt();
        let u = (-1.0 + disc) / (2.0 * a);
        let g = a * u * u;
        let dudy = 1.0 / (1.0 + 2.0 * a * u).max(1e-6);
        (g, 2.0 * a * u * dudy)
    }
}

#[inline]
fn db(x: f32) -> f32 {
    if x <= 1e-10 {
        -200.0
    } else {
        20.0 * x.log10()
    }
}

/// dB → gain, exact 1.0 at 0 dB.
#[inline]
fn gain(g: f32) -> f32 {
    if g == 0.0 {
        1.0
    } else {
        10f32.powf(g * 0.05)
    }
}

#[derive(Clone, Default)]
struct Channel {
    sc_hp: Svf,
    /// Peak detector: maximum over 5 ms (ripple-free above 200 Hz, like a rectifier and its cap).
    pk: SlidingMax,
    /// The RMS reading's sliding maximum (only with `detector_low_hz`).
    rms_max: SlidingMax,
    /// The larger of the two readings of the last sample (peak − 3 dB, RMS), dB.
    level: f32,
    /// Compensates the half-sample low-pass of ADAA at 2× (fitted: within 0.04 dB to 20 kHz).
    hf_comp: Svf,
    ms: f64,
    // gain reduction states (dB, ≤ 0)
    g: f32,
    g_peak: f32,
    g_fast: f32,
    g_slow: f32,
    sustain: f32,
    charge: f32,
    os: Oversampler2x,
    os_dry: Oversampler2x,
    adaa: [Adaa1; 2],
    half_dry: HalfSample,
}

pub struct Comp {
    s: Settings,
    sr: f32,
    ch: [Channel; MAX_CH],
    rngs: [Rng; MAX_CH],
    thresh: ExpSmoother,
    makeup: ExpSmoother,
    mix: ExpSmoother,
    ratio: OnePole,
    knee: OnePole,
    range: OnePole,
    drive: OnePole,
    link: OnePole,
    /// Time constants glide (in log-ms over 50 ms): a jump from 200 ms to 0.02 ms must not snap the
    /// gain down in one sample.
    log_attack: OnePole,
    log_release: OnePole,
    colour_mix: LinearRamp,
    listen_mix: LinearRamp,
    char_fade: LinearRamp,
    prev_char: Character,
    // control-rate coefficients
    att: f32,
    rel: f32,
    rms_coef: f64,
    sustain_coef: f32,
    sustain_fall: f32,
    charge_coef: f32,
    hp_on: bool,
    clock: u64,
    gr_meter: f32,
    /// Library-only setting (not a plugin parameter): `false` keeps the characters' dynamics and drops
    /// their colour stage (shaper, 2× oversampler, Pedal's hiss). The engine is then a pure gain with
    /// zero latency, coherent with any path that does not run through it (gfx-bassdi's dirt path).
    colour: bool,
    /// Library-only setting: the lowest frequency the detector must read without ripple (gfx-mbcomp
    /// §2.4). `None` = the plugin's detector, bit for bit.
    low_hz: Option<f32>,
}

/// One sample of the engine's clock: the linked gains and the smoothed values the audio path uses.
#[derive(Clone, Copy, Debug)]
struct Tick {
    /// Per channel, after `link` (dB, ≤ 0).
    g: [f32; MAX_CH],
    /// The most reduction over the channels (dB, ≤ 0).
    gmin: f32,
    makeup: f32,
    mix: f32,
    cm: f32,
    fade: f32,
    lm: f32,
}

impl Comp {
    pub fn new() -> Self {
        let d = Settings::default();
        Self {
            s: d,
            sr: 48000.0,
            ch: Default::default(),
            rngs: [Rng::new(0x5EED_0001), Rng::new(0x5EED_0002)],
            thresh: ExpSmoother::new(-60.0, 0.0, d.thresh),
            makeup: ExpSmoother::new(-12.0, 24.0, d.makeup),
            mix: ExpSmoother::new(0.0, 100.0, d.mix),
            ratio: OnePole::new(20.0, d.ratio, 1e-5),
            knee: OnePole::new(20.0, d.knee, 1e-5),
            range: OnePole::new(20.0, d.range, 1e-5),
            drive: OnePole::new(20.0, d.drive, 1e-5),
            link: OnePole::new(20.0, d.link, 1e-5),
            log_attack: OnePole::new(50.0, d.attack.ln(), 1e-6),
            log_release: OnePole::new(50.0, d.release.ln(), 1e-6),
            colour_mix: LinearRamp::new(20.0, 0.0),
            listen_mix: LinearRamp::new(10.0, 0.0),
            char_fade: LinearRamp::new(20.0, 1.0),
            prev_char: d.character,
            att: 0.0,
            rel: 0.0,
            rms_coef: 0.0,
            sustain_coef: 0.0,
            sustain_fall: 0.0,
            charge_coef: 0.0,
            hp_on: false,
            clock: 0,
            gr_meter: 0.0,
            colour: true,
            low_hz: None,
        }
    }

    /// Sizes the detector for content down to `f` Hz (gfx-mbcomp §2.4): both readings, the peak and
    /// the one-pole RMS, take their maximum over W = clamp(1.2 / f, 5 ms, 40 ms), gfx-gate's rule, so
    /// a low note's period no longer ripples the gain. `None` = the plugin's detector, bit for bit
    /// (5 ms peak window, the RMS as it is). A library setting, not a plugin parameter.
    pub fn set_detector_low_hz(&mut self, f: Option<f32>) {
        self.low_hz = f.map(|f| f.max(1.0));
        self.apply_windows();
    }

    fn apply_windows(&mut self) {
        let sr = self.sr;
        let w = match self.low_hz {
            None => (0.005 * sr) as usize,
            Some(f) => ((1.2 / f).clamp(LOW_WINDOW_MIN, LOW_WINDOW_MAX) * sr).round() as usize,
        };
        for c in self.ch.iter_mut() {
            c.pk.set_window(w);
            c.rms_max.set_window(w);
        }
    }

    /// The larger of channel `ch`'s two readings at the last sample (peak − 3 dB, RMS; dB): the level
    /// the detector compared with the threshold.
    pub fn level_db(&self, ch: usize) -> f32 {
        self.ch[ch.min(MAX_CH - 1)].level
    }

    /// Turns the colour stage on (the default, the plugin's behaviour) or off (dynamics only).
    /// Off is a library setting for engines embedding the compressor: `drive` is ignored too.
    pub fn set_colour(&mut self, on: bool) {
        self.colour = on;
        self.colour_mix.set_target(if self.colour_amount() > 0.0 { 1.0 } else { 0.0 });
        if !on {
            self.colour_mix.settle();
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    /// Largest gain reduction over the channels after the last block, dB (≥ 0).
    pub fn gain_reduction_db(&self) -> f32 {
        self.gr_meter
    }

    pub fn set(&mut self, s: Settings) {
        let now = self.clock;
        if s.thresh != self.s.thresh {
            self.thresh.set_target(s.thresh, now);
        }
        if s.makeup != self.s.makeup {
            self.makeup.set_target(s.makeup, now);
        }
        if s.mix != self.s.mix {
            self.mix.set_target(s.mix, now);
        }
        self.ratio.set_target(s.ratio.max(1.0));
        self.knee.set_target(s.knee.max(0.0));
        self.range.set_target(s.range.max(0.0));
        self.drive.set_target(s.drive);
        self.link.set_target(s.link);
        self.log_attack.set_target(s.attack.max(0.02).ln());
        self.log_release.set_target(s.release.max(5.0).ln());
        if s.character != self.s.character {
            // structural change: dip-free crossfade of the gain states is not needed (they are dB
            // values of the same curve); the colour crossfades through `colour_mix`
            self.prev_char = self.s.character;
            self.char_fade.set_immediate(0.0);
            self.char_fade.set_target(1.0);
            // the new detector starts from the gain reduction in force: it moves on from there
            // with its own ballistics instead of jumping
            for c in self.ch.iter_mut() {
                // the old shaper carries on from the running state while it fades out
                c.adaa[1] = c.adaa[0].clone();
                c.g_peak = c.g;
                c.g_fast = c.g;
                c.g_slow = c.g;
            }
        }
        self.s = s;
        self.colour_mix.set_target(if self.colour_amount() > 0.0 { 1.0 } else { 0.0 });
        self.listen_mix.set_target(if s.sc_listen { 1.0 } else { 0.0 });
    }

    fn colour_amount(&self) -> f32 {
        if !self.colour {
            return 0.0;
        }
        self.s.character.colour().0 + self.s.drive / 100.0
    }

    fn control(&mut self) {
        let sr = self.sr as f64;
        let ch = self.s.character;
        let mut attack = self.log_attack.value().exp().max(0.02);
        if ch == Character::Pedal {
            attack = attack.min(5.0);
        }
        let mut release = self.log_release.value().exp().max(5.0);
        if ch == Character::Pedal {
            release *= 2.0;
        }
        self.att = one_pole_coef(attack as f64 * 0.001 * sr) as f32;
        self.rel = one_pole_coef(release as f64 * 0.001 * sr) as f32;
        let rms_ms = match ch {
            Character::Bus => 5.0,
            Character::Opto => 10.0,
            _ => 10.0,
        };
        self.rms_coef = one_pole_coef(rms_ms * 0.001 * sr);
        self.sustain_coef = one_pole_coef(0.3 * sr) as f32;
        self.sustain_fall = one_pole_coef(3.0 * sr) as f32;
        self.charge_coef = one_pole_coef(2.0 * sr) as f32;
        self.hp_on = self.s.sc_hpf >= 10.0;
        let f = self.s.sc_hpf.max(10.0);
        for c in self.ch.iter_mut() {
            c.sc_hp.set(SvfMode::HighPass, f, std::f32::consts::FRAC_1_SQRT_2, 0.0, self.sr);
            c.hf_comp.set(SvfMode::HighShelf, 0.375 * self.sr, 0.4, 3.2, self.sr);
        }
    }

    /// One channel's gain reduction (dB, ≤ 0) for this sample.
    #[inline]
    fn detect(&mut self, i: usize, sc: f32, t: f32, r: f32, w: f32, floor: f32) -> f32 {
        let character = self.s.character;
        let auto = self.s.auto_release;
        let (att, rel) = (self.att, self.rel);
        let rms_coef = self.rms_coef;
        let (sustain_coef, sustain_fall) = (self.sustain_coef, self.sustain_fall);
        let charge_coef = self.charge_coef;
        let release_ms = self.log_release.value().exp().max(5.0);
        let sr = self.sr as f64;
        let low = self.low_hz.is_some();
        // program-dependent release coefficients computed here when needed
        let c = &mut self.ch[i];
        c.ms = (sc as f64) * (sc as f64) + (c.ms - (sc as f64) * (sc as f64)) * rms_coef;
        let peak = db(c.pk.next(sc)) - SINE_CREST_DB;
        let rms = if low { db(c.rms_max.next(c.ms.sqrt() as f32)) } else { db((c.ms.sqrt()) as f32) };
        c.level = rms.max(peak);
        // how long and how hard it has been compressing: rises in 300 ms, falls in 3 s
        // busy = the input itself asks for reduction (not the release still recovering from it)
        let busy = if static_curve(rms.max(peak), t, r, w) - rms.max(peak) < -1.0 { 1.0 } else { 0.0 };
        let k_s = if busy > c.sustain { sustain_coef } else { sustain_fall };
        c.sustain = busy + (c.sustain - busy) * k_s;
        let into = |g: f32, target: f32, a: f32, r: f32| if target < g { target + (g - target) * a } else { target + (g - target) * r };
        match character {
            Character::Clean => {
                let tp = (static_curve(peak - 6.0, t, r, w) - (peak - 6.0)).max(floor);
                let tr = (static_curve(rms, t, r, w) - rms).max(floor);
                let rel_p = if auto { rel.powf(1.0 / 0.3) } else { rel };
                let rel_r = if auto { rel.powf(1.0 / (0.5 + 3.5 * c.sustain)) } else { rel };
                c.g_peak = into(c.g_peak, tp, att, rel_p);
                c.g_slow = into(c.g_slow, tr, att, rel_r);
                c.g = c.g_peak.min(c.g_slow);
            }
            Character::Bus => {
                let tr = (static_curve(rms, t, r, w) - rms).max(floor);
                let rel_r = if auto { rel.powf(1.0 / (1.0 + c.sustain)) } else { rel };
                c.g = into(c.g, tr, att, rel_r);
            }
            Character::Fet | Character::Opto | Character::Pedal => {
                let x = if character == Character::Opto { rms } else { peak };
                // release coefficient of the character
                let (rel_a, rel_b) = match character {
                    Character::Fet => {
                                let k = if auto { 0.25 + 0.75 * c.sustain } else { 1.0 };
                        let r1 = rel.powf(1.0 / k);
                        (r1, r1)
                    }
                    Character::Opto => {
                        c.charge = (-c.g / 20.0).clamp(0.0, 1.0) + (c.charge - (-c.g / 20.0).clamp(0.0, 1.0)) * charge_coef;
                        if auto {
                            let fast = rel.powf(2.0);
                            let slow_ms = release_ms * (2.0 + 20.0 * c.charge);
                            let slow = one_pole_coef(slow_ms as f64 * 0.001 * sr) as f32;
                            (fast, slow)
                        } else {
                            (rel, rel)
                        }
                    }
                    _ => (rel, rel),
                };
                // implicit feedback solve: g = a·g_old + (1 − a)·h(x + g)
                let g_old = c.g;
                let (h0, _) = feedback_gain(x + g_old, t, r, w);
                let attacking = h0 < g_old;
                if attacking || character != Character::Opto || !auto {
                    let a = if attacking { att } else { rel_a };
                    let mut g = g_old;
                    for _ in 0..3 {
                        let (h, dh) = feedback_gain(x + g, t, r, w);
                        let f = g - a * g_old - (1.0 - a) * h.max(floor);
                        let df = 1.0 - (1.0 - a) * if h > floor { dh } else { 0.0 };
                        g -= f / df.max(1e-6);
                    }
                    c.g = g.min(0.0).max(floor);
                    c.g_fast = c.g;
                    c.g_slow = c.g;
                } else {
                    // Opto release: half recovers fast, half on the slow tail charged by the work done
                    let target = h0.max(floor);
                    c.g_fast = target + (c.g_fast - target) * rel_a;
                    c.g_slow = target + (c.g_slow - target) * rel_b;
                    c.g = (0.5 * (c.g_fast + c.g_slow)).min(0.0);
                }
            }
        }
        c.g
    }

    /// Advances the engine's clock and smoothers by one sample and runs the detector on each
    /// channel's sidechain sample (`sc_hpf` applied): the linked gains and the smoothed values.
    #[inline]
    #[allow(clippy::needless_range_loop)]
    fn tick(&mut self, input: &[f32], sc: &mut [f32; MAX_CH]) -> Tick {
        let nch = input.len().min(MAX_CH);
        if self.clock.is_multiple_of(CTL) {
            for _ in 0..CTL {
                self.ratio.next();
                self.knee.next();
                self.range.next();
                self.drive.next();
                self.link.next();
                self.log_attack.next();
                self.log_release.next();
            }
            self.control();
        }
        self.clock += 1;
        let t = self.thresh.next();
        let makeup = gain(self.makeup.next());
        let mix = self.mix.next() * 0.01;
        let (r, w) = (self.ratio.value(), self.knee.value());
        let floor = -self.range.value();
        let link = self.link.value() * 0.01;
        let cm = self.colour_mix.next();
        let fade = self.char_fade.next();
        let lm = self.listen_mix.next();

        // detection
        let mut g = [0.0f32; MAX_CH];
        for c in 0..nch {
            let x = input[c];
            let s = if self.hp_on { self.ch[c].sc_hp.process(x) } else { x };
            sc[c] = s;
            g[c] = self.detect(c, s, t, r, w, floor);
        }
        let gmin = g[..nch].iter().cloned().fold(0.0f32, f32::min);
        for c in 0..nch {
            g[c] = link * gmin + (1.0 - link) * g[c];
        }
        Tick { g, gmin, makeup, mix, cm, fade, lm }
    }

    /// The gain API (gfx-mbcomp §2.5): one sample of the engine for each channel's sidechain sample
    /// (`sc.len()` channels, at most 2), returning each channel's gain in dB (≤ 0) after `link`
    /// (`sc_hpf` applies to the sidechain). It advances the clock and the smoothers exactly as
    /// [`Comp::process_block`] does, which is built on it. No audio passes through the engine, so
    /// there is no colour.
    #[inline]
    pub fn next_gain_db(&mut self, sc: &[f32]) -> [f32; MAX_CH] {
        let mut s = [0.0f32; MAX_CH];
        self.tick(sc, &mut s).g
    }

    #[allow(clippy::needless_range_loop)]
    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(MAX_CH);
        let n = buffers.first().map_or(0, |b| b.len());
        let mut meter = 0.0f32;
        for i in 0..n {
            let mut x = [0.0f32; MAX_CH];
            for c in 0..nch {
                x[c] = buffers[c][i];
            }
            let mut sc = [0.0f32; MAX_CH];
            let Tick { g, gmin, makeup, mix, cm, fade, lm } = self.tick(&x[..nch], &mut sc);
            let (base, shaper) = self.s.character.colour();
            let (pbase, pshaper) = self.prev_char.colour();
            let amount = (base + self.drive.value() * 0.01) * fade + (pbase + self.drive.value() * 0.01) * (1.0 - fade);
            let k = 1.0 + 3.0 * amount;
            meter = meter.max(-gmin);

            for c in 0..nch {
                let x = buffers[c][i];
                let gc = g[c];
                let lin = gain(gc);
                let wet = x * lin;
                let ch = &mut self.ch[c];
                // direct path (no colour): exact, bit-transparent when nothing is compressed
                let direct = if cm < 1.0 {
                    let wet_m = if makeup == 1.0 { wet } else { wet * makeup };
                    if mix >= 1.0 {
                        wet_m
                    } else {
                        x + (wet_m - x) * mix
                    }
                } else {
                    0.0
                };
                // colour path at 2×: the shaper (ADAA) delays by half a sample, so the dry gets the
                // same half-sample delay and the same up / down filters (the parallel mix never combs);
                // a fixed shelf compensates the half-sample's top-end loss
                let out = if cm > 0.0 {
                    let up = ch.os.up(wet);
                    let mut y = [0.0f32; 2];
                    for j in 0..2 {
                        let cur = ch.adaa[0].process(shaper, up[j] * k) / k;
                        // character change: the old shaper fades out instead of switching
                        y[j] = if fade < 1.0 { let old = ch.adaa[1].process(pshaper, up[j] * k) / k; old + (cur - old) * fade } else { cur };
                    }
                    let wet_c = ch.os.down(y);
                    let dup = ch.os_dry.up(x);
                    let d = [ch.half_dry.process(dup[0]), ch.half_dry.process(dup[1])];
                    let dry_os = ch.os_dry.down(d);
                    let mut o = dry_os + (wet_c * makeup - dry_os) * mix;
                    if self.s.character == Character::Pedal {
                        // OTA hiss, rising with the squash (seeded: deterministic)
                        let level = 10f32.powf((-100.0 - gc) * 0.05) * fade;
                        o += self.rngs[c].bipolar() * level;
                    }
                    let o = ch.hf_comp.process(o);
                    if cm < 1.0 {
                        direct + (o - direct) * cm
                    } else {
                        o
                    }
                } else {
                    direct
                };
                buffers[c][i] = if lm > 0.0 { out + (sc[c] - out) * lm } else { out };
            }
        }
        self.gr_meter = meter;
    }
}

impl Default for Comp {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Comp {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        self.thresh.prepare(sample_rate);
        self.makeup.prepare(sample_rate);
        self.mix.prepare(sample_rate);
        for s in [&mut self.ratio, &mut self.knee, &mut self.range, &mut self.drive, &mut self.link, &mut self.log_attack, &mut self.log_release] {
            s.prepare(sample_rate);
        }
        self.colour_mix.prepare(sample_rate);
        self.listen_mix.prepare(sample_rate);
        for c in self.ch.iter_mut() {
            c.pk.prepare(((LOW_WINDOW_MAX + 0.001) * sample_rate) as usize);
            c.rms_max.prepare(((LOW_WINDOW_MAX + 0.001) * sample_rate) as usize);
        }
        self.apply_windows();
        self.char_fade.prepare(sample_rate);
    }

    fn reset(&mut self) {
        self.thresh.reset();
        self.makeup.reset();
        self.mix.reset();
        for s in [&mut self.ratio, &mut self.knee, &mut self.range, &mut self.drive, &mut self.link, &mut self.log_attack, &mut self.log_release] {
            s.settle();
        }
        self.colour_mix.settle();
        self.listen_mix.settle();
        self.char_fade.set_immediate(1.0);
        self.prev_char = self.s.character;
        let (_, shaper) = self.s.character.colour();
        for c in self.ch.iter_mut() {
            c.sc_hp.reset();
            c.ms = 0.0;
            c.g = 0.0;
            c.g_peak = 0.0;
            c.g_fast = 0.0;
            c.g_slow = 0.0;
            c.sustain = 0.0;
            c.charge = 0.0;
            c.os.reset();
            c.os_dry.reset();
            c.adaa[0].reset(shaper);
            c.adaa[1].reset(shaper);
            c.half_dry.reset();
            c.pk.reset();
            c.rms_max.reset();
            c.level = -200.0;
            c.hf_comp.reset();
        }
        self.rngs.iter_mut().for_each(Rng::reset);
        self.clock = 0;
        self.gr_meter = 0.0;
        self.control();
    }

    fn set_param(&mut self, id: &str, v: f32) {
        let mut s = self.s;
        match id {
            "thresh" => s.thresh = v,
            "ratio" => s.ratio = v,
            "makeup" => s.makeup = v,
            "attack" => s.attack = v,
            "release" => s.release = v,
            "character" => s.character = Character::from_index(v.round().max(0.0) as u32),
            "knee" => s.knee = v,
            "auto_release" => s.auto_release = v >= 0.5,
            "sc_hpf" => s.sc_hpf = v,
            "range" => s.range = v,
            "drive" => s.drive = v,
            "mix" => s.mix = v,
            "link" => s.link = v,
            "sc_listen" => s.sc_listen = v >= 0.5,
            _ => return,
        }
        self.set(s);
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_block(buffers);
    }
}

#[cfg(test)]
mod tests;
