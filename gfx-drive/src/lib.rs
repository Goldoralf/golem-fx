// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-drive — golem's drive (design: README.md).
//!
//! Stands for golem's `guitar-drive` block (ids `gain`, `hardness`, `ceiling` kept). One parametric
//! engine, filter → shaper → filter (Yeh et al., DAFx 2007; Eichas & Zölzer, JAES 2018). The voices preset
//! the filters and the slew:
//! - Overdrive: a TS-style mid clip, with the unclipped signal summed back;
//! - Distortion: RAT-style slew-rounded highs;
//! - Fuzz: two stages with an inter-stage high-pass;
//! - Legacy: the JS curve exactly.
//!
//! - **Antialiasing**: the shaper runs ADAA1 inside 4× minimum-phase polyphase oversampling (two half-band
//!   stages; 2× left aliases near −77 dBFS at 50 dB of gain).
//! - **Shaper family** along `hardness`: tanh → hard clip, crossfaded. ADAA of a crossfade is the
//!   crossfade of the ADAAs, and the antiderivatives are closed-form. `asym` biases the input.
//! - **Clean path** (`blend`, `bass_keep`): through the same oversampling filters and the same
//!   half-sample delay as the shaper, so the blend never combs.
//!
//! Zero reported latency (the minimum-phase filters and ADAA's quarter base sample are not reported).

#![allow(clippy::needless_range_loop)]

use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::oversample::Oversampler2x;
use gfx_dsp::smooth::OnePole;
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::util::one_pole_coef;
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
mod plugin;

const MAX_CH: usize = 2;
const CTRL: u32 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voice {
    Overdrive,
    Distortion,
    Fuzz,
    Legacy,
}

impl Voice {
    pub const ALL: [Voice; 4] = [Voice::Overdrive, Voice::Distortion, Voice::Fuzz, Voice::Legacy];

    /// (own pre-clip high-pass, pre-clip low-pass, slew-limited)
    fn filters(self) -> (f32, f32, bool) {
        match self {
            Voice::Overdrive => (720.0, 6000.0, false),
            Voice::Distortion => (100.0, 5000.0, true),
            Voice::Fuzz => (40.0, 8000.0, false),
            Voice::Legacy => (0.0, 0.0, false),
        }
    }
}

pub static PARAMS: [ParamInfo; 10] = [
    ParamInfo::float("gain", "Gain", 0.0, 50.0, 20.0).with_exp(),
    ParamInfo::float("hardness", "Hardness", 1.0, 10.0, 6.0).with_exp(),
    ParamInfo::float("ceiling", "Ceiling", -40.0, 0.0, -12.0),
    ParamInfo::enumeration("voice", "Voice", 4, 0),
    ParamInfo::float("tight", "Tight", 20.0, 1000.0, 20.0),
    ParamInfo::float("tone", "Tone", -100.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("asym", "Asymmetry", 0.0, 100.0, 0.0),
    ParamInfo::float("sag", "Sag", 0.0, 100.0, 0.0),
    ParamInfo::float("blend", "Blend", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("bass_keep", "Bass Keep", 0.0, 500.0, 0.0),
];

pub mod p {
    pub const GAIN: usize = 0;
    pub const HARDNESS: usize = 1;
    pub const CEILING: usize = 2;
    pub const VOICE: usize = 3;
    pub const TIGHT: usize = 4;
    pub const TONE: usize = 5;
    pub const ASYM: usize = 6;
    pub const SAG: usize = 7;
    pub const BLEND: usize = 8;
    pub const BASS_KEEP: usize = 9;
    pub const N: usize = 10;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub v: [f32; p::N],
}

impl Default for Settings {
    fn default() -> Self {
        Self { v: core::array::from_fn(|i| PARAMS[i].default) }
    }
}

impl Settings {
    pub fn with(mut self, i: usize, value: f32) -> Self {
        self.v[i] = value;
        self
    }
}

#[inline]
fn idx(v: f32, n: usize) -> usize {
    (v.round().max(0.0) as usize).min(n - 1)
}

// ---- shapers: f and its antiderivative F --------------------------------------------------------------

#[inline]
fn ln_cosh(x: f64) -> f64 {
    let a = x.abs();
    a + (-2.0 * a).exp().ln_1p() - std::f64::consts::LN_2
}

#[inline]
fn clip_f(x: f64) -> f64 {
    x.clamp(-1.0, 1.0)
}

#[inline]
fn clip_af(x: f64) -> f64 {
    if x.abs() <= 1.0 {
        0.5 * x * x
    } else {
        x.abs() - 0.5
    }
}

/// The JS curve (`guitar/distortion`): linear to `m`, then `m + d / (soft + d)`.
#[inline]
pub fn legacy_f(x: f64, m: f64, soft: f64) -> f64 {
    let a = x.abs();
    if a <= m {
        x
    } else {
        let d = a - m;
        x.signum() * (m + d / (soft + d))
    }
}

#[inline]
fn legacy_af(x: f64, m: f64, soft: f64) -> f64 {
    let a = x.abs();
    if a <= m {
        0.5 * x * x
    } else {
        let d = a - m;
        0.5 * m * m + m * d + d - soft * ((soft + d) / soft).ln()
    }
}

/// The new voices' shaper at hardness weight w (0 tanh … 1 hard clip), biased by b.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub w: f64,
    pub b: f64,
}

impl Shape {
    #[inline]
    fn raw(self, x: f64) -> f64 {
        (1.0 - self.w) * x.tanh() + self.w * clip_f(x)
    }
    #[inline]
    fn raw_af(self, x: f64) -> f64 {
        (1.0 - self.w) * ln_cosh(x) + self.w * clip_af(x)
    }
    /// f(x) = s(x + b) − s(b): zero at rest, DC-free at small signals
    #[inline]
    pub fn f(self, x: f64) -> f64 {
        self.raw(x + self.b) - self.raw(self.b)
    }
    #[inline]
    pub fn af(self, x: f64) -> f64 {
        self.raw_af(x + self.b) - x * self.raw(self.b)
    }
}

/// ADAA1 whose shaper may change every sample: both ends are evaluated with the current shaper.
#[derive(Clone, Copy, Debug, Default)]
struct Adaa {
    x1: f64,
}

impl Adaa {
    #[inline]
    fn run(&mut self, x: f64, f: impl Fn(f64) -> f64, af: impl Fn(f64) -> f64) -> f64 {
        let d = x - self.x1;
        let y = if d.abs() < 1e-6 { f(0.5 * (x + self.x1)) } else { (af(x) - af(self.x1)) / d };
        self.x1 = x;
        y
    }
}

/// One voice's running state (two slots per channel, so a voice change crossfades two live engines).
#[derive(Clone, Default)]
struct Engine {
    os: Oversampler2x,
    os2: Oversampler2x,
    adaa1: Adaa,
    adaa2: Adaa,
    inter_hp: f32,
    inter_x1: f32,
    /// the voice's own pre-clip filters (each engine keeps its voice's, so a crossfade is clean)
    hp1_x: f32,
    hp1_y: f32,
    pre_lp: Svf,
    slew_lp: f32,
    slew_env: f32,
}

#[derive(Clone, Default)]
struct Channel {
    eng: [Engine; 2],
    os_clean: Oversampler2x,
    os_clean2: Oversampler2x,
    clean_lo: [Svf; 4],
    hi_hp: [Svf; 4],
    clean_hp: [Svf; 4],
    tight_user: Svf,
    tone_lp: Svf,
    tone_hs: Svf,
    half: f32,
    dc_x: f32,
    dc_y: f32,
    env: f32,
}

pub struct Drive {
    s: Settings,
    sr: f32,
    // the slew envelope's decay, fixed in time (0.999 per sample at 48 kHz, as before; it was that at every rate)
    slew_k: f32,
    ch: [Channel; MAX_CH],
    gain: ExpSmoother,
    hardness: ExpSmoother,
    tone: ExpSmoother,
    blend: ExpSmoother,
    ceiling: OnePole,
    asym: OnePole,
    sag: OnePole,
    tight: OnePole,
    bass: OnePole,
    bass_amt: OnePole,
    /// voice crossfade (the old voice fades out over 20 ms)
    voice: Voice,
    prev_voice: Voice,
    vfade: OnePole,
    /// the engine slot of the current voice (the other one plays the previous voice while it fades)
    slot: usize,
    ctrl: u32,
    clock: u64,
}

impl Drive {
    pub fn new() -> Self {
        let s = Settings::default();
        let v = s.v;
        Self {
            s,
            sr: 48000.0,
            slew_k: 0.999,
            ch: Default::default(),
            gain: ExpSmoother::new(0.0, 50.0, v[p::GAIN]),
            hardness: ExpSmoother::new(1.0, 10.0, v[p::HARDNESS]),
            tone: ExpSmoother::new(-100.0, 100.0, v[p::TONE]),
            blend: ExpSmoother::new(0.0, 100.0, v[p::BLEND]),
            ceiling: OnePole::new(60.0, v[p::CEILING], 1e-4),
            asym: OnePole::new(30.0, v[p::ASYM], 1e-4),
            sag: OnePole::new(30.0, v[p::SAG], 1e-4),
            tight: OnePole::new(30.0, v[p::TIGHT], 1e-3),
            bass: OnePole::new(30.0, 100.0, 1e-3),
            bass_amt: OnePole::new(20.0, 0.0, 1e-6),
            voice: Voice::Overdrive,
            prev_voice: Voice::Overdrive,
            vfade: OnePole::new(20.0, 1.0, 1e-5),
            slot: 0,
            ctrl: 0,
            clock: 0,
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn set(&mut self, s: Settings) {
        let v = &s.v;
        let old = self.s.v;
        if v[p::GAIN] != old[p::GAIN] {
            self.gain.set_target(v[p::GAIN].clamp(0.0, 50.0), self.clock);
        }
        if v[p::HARDNESS] != old[p::HARDNESS] {
            self.hardness.set_target(v[p::HARDNESS].clamp(1.0, 10.0), self.clock);
        }
        if v[p::TONE] != old[p::TONE] {
            self.tone.set_target(v[p::TONE].clamp(-100.0, 100.0), self.clock);
        }
        if v[p::BLEND] != old[p::BLEND] {
            self.blend.set_target(v[p::BLEND].clamp(0.0, 100.0), self.clock);
        }
        self.ceiling.set_target(v[p::CEILING]);
        self.asym.set_target(v[p::ASYM]);
        self.sag.set_target(v[p::SAG]);
        self.tight.set_target(v[p::TIGHT]);
        if v[p::BASS_KEEP] >= 20.0 {
            self.bass.set_target(v[p::BASS_KEEP]);
        }
        self.bass_amt.set_target(if v[p::BASS_KEEP] >= 20.0 { 1.0 } else { 0.0 });
        let voice = Voice::ALL[idx(v[p::VOICE], 4)];
        if voice != self.voice {
            self.prev_voice = self.voice;
            self.voice = voice;
            self.vfade.set_immediate(0.0);
            self.vfade.set_target(1.0);
            self.slot ^= 1;
            let (slot, sr) = (self.slot, self.sr);
            for c in self.ch.iter_mut() {
                c.eng[slot] = Engine::default();
                Self::set_voice_filters(&mut c.eng[slot], voice, sr);
            }
        }
        self.s = s;
    }

    fn set_voice_filters(c: &mut Engine, voice: Voice, sr: f32) {
        let (_, lp, _) = voice.filters();
        c.pre_lp.set(SvfMode::LowPass, if lp > 0.0 { lp.min(0.45 * sr) } else { 0.45 * sr }, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
    }

    fn control(&mut self) {
        let sr = self.sr;
        let tight = self.tight.next();
        let bk = self.bass.next();
        let tone = self.tone.value() * 0.01;
        for c in self.ch.iter_mut() {
            c.tight_user.set(SvfMode::HighPass, tight.max(10.0), std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            // Linkwitz-Riley 8 (a 4th-order Butterworth, twice): at 50 dB of gain a gentler split would
            // still leak enough lows into the drive to clip them
            const QS: [f32; 4] = [0.5412, 1.3066, 0.5412, 1.3066];
            for (k, f) in c.clean_lo.iter_mut().enumerate() {
                f.set(SvfMode::LowPass, bk, QS[k], 0.0, sr);
            }
            for (k, f) in c.hi_hp.iter_mut().enumerate() {
                f.set(SvfMode::HighPass, bk, QS[k], 0.0, sr);
            }
            for (k, f) in c.clean_hp.iter_mut().enumerate() {
                f.set(SvfMode::HighPass, bk, QS[k], 0.0, sr);
            }
            // tone: below 0 a low-pass from 10 kHz down to 1 kHz; above 0 a high shelf up to +6 dB at 2 kHz
            let lp = 10000.0 * 10f32.powf(tone.min(0.0));
            c.tone_lp.set(SvfMode::LowPass, lp.min(0.45 * sr), std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            c.tone_hs.set(SvfMode::HighShelf, 2000.0, std::f32::consts::FRAC_1_SQRT_2, 6.0 * tone.max(0.0), sr);
        }
    }

    /// The driven signal (oversampled shaper) of one voice for one oversampled sample.
    #[inline]
    fn shape(c: &mut Engine, voice: Voice, z: f32, shape: Shape, legacy: (f64, f64), inter_k: f32) -> f32 {
        match voice {
            Voice::Legacy => {
                let (m, soft) = legacy;
                c.adaa1.run(z as f64, |x| legacy_f(x, m, soft), |x| legacy_af(x, m, soft)) as f32
            }
            Voice::Fuzz => {
                // two stages: the first takes ~60 % of the gain, an inter-stage high-pass, the second the rest
                let y1 = c.adaa1.run(z as f64 * 2.0, |x| shape.f(x), |x| shape.af(x)) as f32;
                c.inter_hp = inter_k * (c.inter_hp + y1 - c.inter_x1);
                c.inter_x1 = y1;
                let s2 = Shape { w: shape.w, b: shape.b * 0.5 };
                c.adaa2.run(c.inter_hp as f64 * 3.0, |x| s2.f(x), |x| s2.af(x)) as f32
            }
            Voice::Distortion | Voice::Overdrive => {
                c.adaa1.run(z as f64, |x| shape.f(x), |x| shape.af(x)) as f32
            }
        }
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(MAX_CH);
        let n = buffers.first().map_or(0, |b| b.len());
        let sr = self.sr as f64;
        let sr_os = 4.0 * sr;
        let att = one_pole_coef(0.005 * sr) as f32;
        let rel = one_pole_coef(0.15 * sr) as f32;
        let inter_k = one_pole_coef(sr_os / (std::f64::consts::TAU * 150.0)) as f32;
        let dc_k = one_pole_coef(sr / (std::f64::consts::TAU * 10.0)) as f32;
        for i in 0..n {
            self.clock += 1;
            if self.ctrl == 0 {
                self.control();
            }
            self.ctrl = (self.ctrl + 1) % CTRL;
            let gain_db = self.gain.next();
            let hard = self.hardness.next() as f64;
            let tone = self.tone.next() * 0.01;
            let blend = self.blend.next() * 0.01;
            let ceil_db = self.ceiling.next();
            let asym = self.asym.next() as f64 * 0.01;
            let sag = self.sag.next() * 0.01;
            let bk = self.bass_amt.next();
            let vf = self.vfade.next();
            let c_lin = 10f32.powf(ceil_db / 20.0);
            let g = 10f32.powf(gain_db / 20.0);
            let w = ((hard - 1.0) / 9.0).clamp(0.0, 1.0);
            // Legacy: the JS's own units
            let legacy = (2f64.powf(ceil_db as f64 / 6.0), 2f64.powf(hard));
            let g_legacy = 2f32.powf(gain_db / 6.0);
            // tone is exactly flat at 0 (its filters fade in over the first 5 %)
            let tone_amt = (tone.abs() * 20.0).min(1.0);
            for c in 0..nch {
                let x = buffers[c][i];
                let ch = &mut self.ch[c];
                // bass keep: Linkwitz-Riley split; with it off the drive gets the input untouched
                let mut h1 = x;
                for f in ch.hi_hp.iter_mut() {
                    h1 = f.process(h1);
                }
                let hi = x + bk * (h1 - x);
                // sag: hard hits lower the gain and move the bias
                let e_in = (hi * g / c_lin).abs();
                let k = if e_in > ch.env { att } else { rel };
                ch.env = e_in + (ch.env - e_in) * k;
                let over = (ch.env - 1.0).max(0.0);
                let g_eff = g / (1.0 + 0.5 * sag * over);
                let shape = Shape { w, b: 0.6 * asym + 0.3 * (sag * over.min(2.0)) as f64 };
                // pre-clip filters: the voice's own high-pass, the user's tight, the voice's low-pass
                let t = ch.tight_user.process(hi);
                let srf = self.sr;
                let slew_k = self.slew_k;
                let run = |voice: Voice, e: &mut Engine| -> f32 {
                    // the voice's own first-order high-pass (the TS's 720 Hz gain-leg corner) and low-pass
                    let vhp = voice.filters().0;
                    let hp1 = if vhp > 0.0 {
                        let k = (-std::f32::consts::TAU * vhp / srf).exp();
                        let y = k * (e.hp1_y + t - e.hp1_x);
                        e.hp1_x = t;
                        e.hp1_y = y;
                        y
                    } else {
                        t
                    };
                    let mut pre = e.pre_lp.process(hp1);
                    if voice == Voice::Distortion {
                        // slew: the bigger the swing, the lower the corner (6 kHz at the clip level)
                        let a = (pre * g_eff / c_lin).abs();
                        e.slew_env = a.max(e.slew_env * slew_k);
                        let fc = (6000.0 / e.slew_env.max(1.0)).clamp(800.0, 0.45 * srf);
                        let k = 1.0 - (-std::f32::consts::TAU * fc / srf).exp();
                        e.slew_lp += k * (pre - e.slew_lp);
                        pre = e.slew_lp;
                    }
                    let (z_in, scale) = match voice {
                        Voice::Legacy => (hi * g_legacy, 1.0),
                        Voice::Overdrive => (pre * g_eff / c_lin, c_lin),
                        _ => (pre * g_eff / c_lin, c_lin),
                    };
                    // 4×: two polyphase half-band stages
                    let [a, b] = e.os.up(z_in);
                    let mut half = [0.0f32; 2];
                    for (k, v) in [a, b].into_iter().enumerate() {
                        let [c0, c1] = e.os2.up(v);
                        let y0 = Self::shape(e, voice, c0, shape, legacy, inter_k);
                        let y1 = Self::shape(e, voice, c1, shape, legacy, inter_k);
                        half[k] = e.os2.down([y0, y1]);
                    }
                    e.os.down(half) * scale
                };
                let slot = self.slot;
                let mut driven = run(self.voice, &mut ch.eng[slot]);
                if vf < 0.999 {
                    let old = run(self.prev_voice, &mut ch.eng[slot ^ 1]);
                    driven = old + (driven - old) * vf;
                }
                // the clean path: the same oversampling filters and the shaper's half-sample delay
                let [ca, cb] = ch.os_clean.up(x);
                let mut halfc = [0.0f32; 2];
                for (k, v) in [ca, cb].into_iter().enumerate() {
                    let [c0, c1] = ch.os_clean2.up(v);
                    let h0 = 0.5 * (c0 + ch.half);
                    let h1 = 0.5 * (c1 + c0);
                    ch.half = c1;
                    halfc[k] = ch.os_clean2.down([h0, h1]);
                }
                let clean = ch.os_clean.down(halfc);
                // the lows the drive never saw, from the phase-matched clean path
                let mut cl = clean;
                for f in ch.clean_lo.iter_mut() {
                    cl = f.process(cl);
                }
                let clean_low = cl * bk;
                let mut c1 = clean;
                for f in ch.clean_hp.iter_mut() {
                    c1 = f.process(c1);
                }
                let clean_hi = clean + bk * (c1 - clean);
                // TS: the unclipped signal is summed back (Overdrive)
                let ts_clean = if self.voice == Voice::Overdrive { clean_hi * vf } else if self.prev_voice == Voice::Overdrive { clean_hi * (1.0 - vf) } else { 0.0 };
                let mut y = driven + ts_clean;
                // DC block, tone
                let yd = y - ch.dc_x + dc_k * ch.dc_y;
                ch.dc_x = y;
                ch.dc_y = yd;
                y = yd;
                let toned = ch.tone_hs.process(ch.tone_lp.process(y));
                y += (toned - y) * tone_amt;
                // blend (Klon) and the clean lows
                buffers[c][i] = y + (clean_hi - y) * blend + clean_low;
            }
        }
    }
}

impl Default for Drive {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Drive {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        self.slew_k = 0.999f64.powf(48000.0 / sample_rate as f64) as f32;
        for e in [&mut self.gain, &mut self.hardness, &mut self.tone, &mut self.blend] {
            e.prepare(sample_rate);
        }
        for o in [&mut self.ceiling, &mut self.asym, &mut self.sag, &mut self.bass_amt, &mut self.vfade] {
            o.prepare(sample_rate);
        }
        for o in [&mut self.tight, &mut self.bass] {
            o.prepare(sample_rate / CTRL as f32);
        }
        let v = self.voice;
        for c in self.ch.iter_mut() {
            for e in c.eng.iter_mut() {
                Self::set_voice_filters(e, v, sample_rate);
            }
        }
        self.control();
    }

    fn reset(&mut self) {
        let v = self.s.v;
        self.gain.settle(v[p::GAIN]);
        self.hardness.settle(v[p::HARDNESS]);
        self.tone.settle(v[p::TONE]);
        self.blend.settle(v[p::BLEND]);
        for o in [&mut self.ceiling, &mut self.asym, &mut self.sag, &mut self.tight, &mut self.bass, &mut self.bass_amt] {
            o.settle();
        }
        self.voice = Voice::ALL[idx(v[p::VOICE], 4)];
        self.prev_voice = self.voice;
        self.vfade.set_immediate(1.0);
        self.slot = 0;
        let (sr, voice) = (self.sr, self.voice);
        for c in self.ch.iter_mut() {
            *c = Channel::default();
            for e in c.eng.iter_mut() {
                Self::set_voice_filters(e, voice, sr);
            }
        }
        self.ctrl = 0;
        self.clock = 0;
        self.control();
    }

    fn set_param(&mut self, id: &str, value: f32) {
        if let Some(i) = PARAMS.iter().position(|pi| pi.id == id) {
            let s = self.s.with(i, value);
            self.set(s);
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_block(buffers);
    }
}

#[cfg(test)]
mod tests;
