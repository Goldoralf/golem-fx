// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-shimmer — golem's shimmer reverb (design: README.md).
//!
//! Stands for golem's `reverb-shimmer` block (ids `mix`, `decay`, `shimmer`, `pitch`, `hicut` kept;
//! `pitch` widened to ±24 st so the octave is reachable).
//!
//! The shifter sits in the feedback of the reverb (Eno / Lanois, ValhallaShimmer), so every pass climbs.
//! - **Loop**: the tank's output → loop low-cut → loop high-cut (clamped against aliasing) → two
//!   detuned voices → back into the tank.
//! - **Shifter**: gfx-dsp's loop shifter, equal-power heads and seeded jitter, forward (Grainy) or reverse
//!   grains (Smooth).
//! - **Tank**: gfx-dsp's reverb engine (Hall, Cathedral, Cloud FDN, or the Plate tank).
//!
//! Around the loop:
//! - placement Regen / Input / Both;
//! - freeze that stops the climb (the shifters leave the loop); infinite;
//! - ducking, swell, pitch glide;
//! - trails through the VST3 bypass parameter.
//!
//! The shifter's delay is part of the tail: latency 0. Deterministic.

#![allow(clippy::needless_range_loop)]

use gfx_dsp::delay::{DelayLine, Interp};
use gfx_dsp::envelope::PeakFollower;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::pitch::LoopShifter;
use gfx_dsp::reverb::{Diffuser, Fdn, Plate};
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
mod plugin;

const GRID: u32 = 16;

pub static PARAMS: [ParamInfo; 29] = [
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 40.0).with_exp(),
    ParamInfo::float("decay", "Decay", 0.5, 20.0, 4.0).with_exp(),
    ParamInfo::float("shimmer", "Shimmer", 0.0, 100.0, 50.0).with_exp(),
    ParamInfo::float("pitch", "Pitch", -24.0, 24.0, 12.0),
    ParamInfo::float("hicut", "High Cut", 1000.0, 16000.0, 6000.0).with_exp(),
    ParamInfo::float("pitch2", "Pitch 2", -24.0, 24.0, 7.0),
    ParamInfo::float("voice2", "Voice 2 Level", 0.0, 100.0, 0.0),
    ParamInfo::float("detune", "Detune", 0.0, 50.0, 6.0),
    ParamInfo::float("lowcut", "Low Cut", 20.0, 1000.0, 150.0),
    ParamInfo::enumeration("placement", "Placement", 3, 0),
    ParamInfo::float("size", "Size", 0.0, 100.0, 60.0),
    ParamInfo::float("predelay", "Predelay", 0.0, 500.0, 20.0),
    ParamInfo::float("bloom", "Bloom", 0.0, 100.0, 70.0),
    ParamInfo::float("moddepth", "Modulation", 0.0, 100.0, 25.0),
    ParamInfo::enumeration("character", "Character", 2, 0),
    ParamInfo::enumeration("space", "Space", 4, 0),
    ParamInfo::float("mod_rate", "Modulation Rate", 0.0, 100.0, 50.0),
    ParamInfo::float("swell", "Swell", 0.0, 2000.0, 0.0),
    ParamInfo::float("voice_pan", "Voice Spread", 0.0, 100.0, 50.0),
    ParamInfo::float("bass_mult", "Bass Decay", 0.5, 2.0, 1.0),
    ParamInfo::float("tone", "Tone", 0.0, 100.0, 50.0),
    ParamInfo::float("glide", "Pitch Glide", 0.0, 2000.0, 50.0),
    ParamInfo::float("duck", "Ducking", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::enumeration("freeze", "Freeze", 3, 0),
    ParamInfo::float("width", "Width", 0.0, 100.0, 100.0),
    ParamInfo::boolean("kill_dry", "Kill Dry", false),
    ParamInfo::boolean("trails", "Trails", true),
    ParamInfo::boolean("bypass", "Bypass", false),
    ParamInfo::float("wet_level", "Wet Level", -24.0, 24.0, 0.0),
];

pub mod p {
    pub const MIX: usize = 0;
    pub const DECAY: usize = 1;
    pub const SHIMMER: usize = 2;
    pub const PITCH: usize = 3;
    pub const HICUT: usize = 4;
    pub const PITCH2: usize = 5;
    pub const VOICE2: usize = 6;
    pub const DETUNE: usize = 7;
    pub const LOWCUT: usize = 8;
    pub const PLACEMENT: usize = 9;
    pub const SIZE: usize = 10;
    pub const PREDELAY: usize = 11;
    pub const BLOOM: usize = 12;
    pub const MODDEPTH: usize = 13;
    pub const CHARACTER: usize = 14;
    pub const SPACE: usize = 15;
    pub const MOD_RATE: usize = 16;
    pub const SWELL: usize = 17;
    pub const VOICE_PAN: usize = 18;
    pub const BASS_MULT: usize = 19;
    pub const TONE: usize = 20;
    pub const GLIDE: usize = 21;
    pub const DUCK: usize = 22;
    pub const FREEZE: usize = 23;
    pub const WIDTH: usize = 24;
    pub const KILL_DRY: usize = 25;
    pub const TRAILS: usize = 26;
    pub const BYPASS: usize = 27;
    pub const WET_LEVEL: usize = 28;
    pub const N: usize = 29;
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

/// The spaces: FDN scale at size 100 % (Hall, Cathedral, Cloud) or the Plate tank.
const SPACE_SCALE: [f64; 4] = [1.3, 2.2, 3.4, 1.0];

pub struct Shimmer {
    s: Settings,
    sr: f32,
    space: usize,
    character: usize,
    pending: bool,
    gate: LinearRamp,
    pre: [DelayLine; 2],
    diff: Diffuser,
    bloom: Diffuser,
    bloom_fb: [f32; 8],
    fdn: Fdn,
    plate: Plate,
    plate_in: f32,
    /// [channel][voice]: regeneration shifters; input-placement shifters
    regen: [[LoopShifter; 2]; 2],
    input_sh: [[LoopShifter; 2]; 2],
    loop_hp: [Svf; 2],
    loop_lp: [Svf; 2],
    loop_dc: [(f32, f32); 2],
    fb: [f32; 2],
    tone_lo: [Svf; 2],
    tone_hi: [Svf; 2],
    duck_env: PeakFollower,
    duck_gr: f32,
    swell_g: f32,
    env_fast: f32,
    env_slow: f32,
    mix: ExpSmoother,
    decay: ExpSmoother,
    shimmer: ExpSmoother,
    hicut: ExpSmoother,
    duck: ExpSmoother,
    pitch: OnePole,
    pitch2: OnePole,
    voice2: OnePole,
    size: OnePole,
    predelay: OnePole,
    width: OnePole,
    dry_g: OnePole,
    freeze_in: OnePole,
    regen_amt: OnePole,
    input_amt: OnePole,
    in_g: LinearRamp,
    wet_g: LinearRamp,
    was_on: bool,
    tail_in: LinearRamp,
    tone: OnePole,
    bloom_s: OnePole,
    wet_level: OnePole,
    /// a character change only restarts the shifters; a space change clears the tank too
    pending_space: bool,
    key: [f64; 11],
    v2_idle: [bool; 2],
    ctrl: u32,
    clock: u64,
}

impl Shimmer {
    pub fn new() -> Self {
        let s = Settings::default();
        let v = s.v;
        let sh = |seed: u64| LoopShifter::new(seed);
        Self {
            s,
            sr: 48000.0,
            space: 0,
            character: 0,
            pending: false,
            gate: LinearRamp::new(40.0, 1.0),
            pre: Default::default(),
            diff: Diffuser::default(),
            bloom: Diffuser::default(),
            bloom_fb: [0.0; 8],
            fdn: Fdn::default(),
            plate: Plate::default(),
            plate_in: 0.0,
            regen: [[sh(0x51), sh(0x52)], [sh(0x53), sh(0x54)]],
            input_sh: [[sh(0x61), sh(0x62)], [sh(0x63), sh(0x64)]],
            loop_hp: Default::default(),
            loop_lp: Default::default(),
            loop_dc: [(0.0, 0.0); 2],
            fb: [0.0; 2],
            tone_lo: Default::default(),
            tone_hi: Default::default(),
            duck_env: PeakFollower::new(5.0, 400.0),
            duck_gr: 1.0,
            swell_g: 1.0,
            env_fast: 0.0,
            env_slow: 0.0,
            mix: ExpSmoother::new(0.0, 100.0, v[p::MIX]),
            decay: ExpSmoother::new(0.5, 20.0, v[p::DECAY]),
            shimmer: ExpSmoother::new(0.0, 100.0, v[p::SHIMMER]),
            hicut: ExpSmoother::new(1000.0, 16000.0, v[p::HICUT]),
            duck: ExpSmoother::new(0.0, 100.0, v[p::DUCK]),
            pitch: OnePole::new(50.0, v[p::PITCH], 1e-5),
            pitch2: OnePole::new(50.0, v[p::PITCH2], 1e-5),
            voice2: OnePole::new(30.0, v[p::VOICE2], 1e-4),
            size: OnePole::new(400.0, v[p::SIZE], 1e-4),
            predelay: OnePole::new(100.0, v[p::PREDELAY], 1e-3),
            width: OnePole::new(30.0, v[p::WIDTH], 1e-4),
            dry_g: OnePole::new(5.0, 1.0, 1e-6),
            freeze_in: OnePole::new(20.0, 1.0, 1e-6),
            regen_amt: OnePole::new(20.0, 1.0, 1e-6),
            input_amt: OnePole::new(20.0, 0.0, 1e-6),
            in_g: LinearRamp::new(5.0, 1.0),
            wet_g: LinearRamp::new(20.0, 1.0),
            was_on: true,
            tail_in: LinearRamp::new(20.0, 1.0),
            tone: OnePole::new(30.0, 50.0, 1e-3),
            bloom_s: OnePole::new(50.0, 70.0, 1e-4),
            wet_level: OnePole::new(20.0, 0.0, 1e-4),
            pending_space: false,
            key: [f64::NAN; 11],
            v2_idle: [true; 2],
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
        for (i, e) in [(p::MIX, &mut self.mix), (p::DECAY, &mut self.decay), (p::SHIMMER, &mut self.shimmer), (p::HICUT, &mut self.hicut), (p::DUCK, &mut self.duck)] {
            if v[i] != old[i] {
                e.set_target(v[i].clamp(PARAMS[i].min, PARAMS[i].max), self.clock);
            }
        }
        self.pitch.set_target(v[p::PITCH]);
        self.pitch2.set_target(v[p::PITCH2]);
        self.voice2.set_target(v[p::VOICE2]);
        self.size.set_target(v[p::SIZE]);
        self.predelay.set_target(v[p::PREDELAY]);
        self.width.set_target(v[p::WIDTH]);
        self.dry_g.set_target(if v[p::KILL_DRY] >= 0.5 { 0.0 } else { 1.0 });
        let freeze = idx(v[p::FREEZE], 3);
        self.freeze_in.set_target(if freeze == 1 { 0.0 } else { 1.0 });
        // placement: Regen (the loop climbs), Input (one shifted copy in, no climb), Both;
        // freeze takes the shifters out of the loop, so a held pad never climbs
        let pl = idx(v[p::PLACEMENT], 3);
        self.regen_amt.set_target(if pl != 1 && freeze == 0 { 1.0 } else { 0.0 });
        self.input_amt.set_target(if pl != 0 { 1.0 } else { 0.0 });
        if idx(v[p::SPACE], 4) != self.space {
            self.pending_space = true;
        }
        if idx(v[p::SPACE], 4) != self.space || idx(v[p::CHARACTER], 2) != self.character {
            self.pending = true;
            self.gate.set_target(0.0);
        }
        self.tone.set_target(v[p::TONE]);
        self.bloom_s.set_target(v[p::BLOOM]);
        self.wet_level.set_target(v[p::WET_LEVEL]);
        self.s = s;
    }

    fn control(&mut self) {
        let sr = self.sr;
        let v = self.s.v;
        let size = self.size.next() as f64 * 0.01;
        self.tone.next();
        let k = (0.4 + 0.6 * size) * SPACE_SCALE[self.space.min(2)];
        let rt = self.decay.value() as f64;
        let hicut = self.hicut.value() as f64;
        let bm = v[p::BASS_MULT] as f64;
        self.fdn.frozen = idx(v[p::FREEZE], 3) != 0;
        self.plate.frozen = self.fdn.frozen;
        // coefficients only when what they depend on moves (dozens of powers otherwise, every 16 samples)
        let key = [size, rt, hicut, bm, v[p::MODDEPTH] as f64, v[p::MOD_RATE] as f64, self.bloom_s.value() as f64, self.space as f64, self.pitch.value().max(self.pitch2.value()) as f64, self.tone.value() as f64, v[p::LOWCUT] as f64];
        if key == self.key {
            return;
        }
        self.key = key;
        self.fdn.set_scale(k);
        self.fdn.set_decay(rt * bm, rt, rt * 0.6, 300.0, hicut);
        let md = v[p::MODDEPTH] as f64 / 25.0;
        let rate = 0.4 * 2f64.powf((v[p::MOD_RATE] as f64 - 50.0) / 25.0);
        self.fdn.set_mod(0.5 * md, rate);
        let bloom = self.bloom_s.value() as f64 * 0.01;
        self.diff.set_scale((0.3 + 0.7 * bloom) * k.min(2.5));
        self.bloom.set_scale(k.min(2.5));
        self.plate.set_scale(0.6 + 0.8 * size);
        let half = self.plate.half_loop_s();
        self.plate.decay = 10f64.powf(-1.5 * half / rt.max(0.05)) as f32;
        self.plate.kd = (-std::f32::consts::TAU * (hicut.min(0.45 * sr as f64) as f32) / sr).exp();
        self.plate.kb = 1.0 - (-std::f32::consts::TAU * 300.0 / sr).exp();
        self.plate.bass_g = (10f64.powf(-1.5 * half / (rt * bm)) / self.plate.decay as f64) as f32;
        self.plate.excursion = 16.0 * md;
        self.plate.rate = rate * 2.0;
        self.plate.density = (0.4 + 0.6 * bloom) as f32;
        // loop filters: the high cut also anti-aliases the shifters (≤ 0.45 · sr / ratio)
        let ratio = (self.pitch.value().max(self.pitch2.value()).max(0.0) as f64 / 12.0).exp2();
        let lp = (hicut.min(0.45 * sr as f64 / ratio)) as f32;
        let tone = (self.tone.value() - 50.0) / 50.0;
        for c in 0..2 {
            self.loop_hp[c].set(SvfMode::HighPass, v[p::LOWCUT], std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            self.loop_lp[c].set(SvfMode::LowPass, lp, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            // tone: a tilt around 1 kHz (±6 dB at the ends)
            self.tone_lo[c].set(SvfMode::LowShelf, 300.0, 0.7, -6.0 * tone, sr);
            self.tone_hi[c].set(SvfMode::HighShelf, 3000.0, 0.7, 6.0 * tone, sr);
        }
        self.duck_env.set_times(5.0, 400.0, sr);
    }

    fn clear_tails(&mut self) {
        self.tail_in.set_immediate(0.0);
        self.tail_in.set_target(1.0);
        self.bloom.reset();
        self.bloom_fb = [0.0; 8];
        self.fdn.reset();
        self.plate.reset();
        self.plate_in = 0.0;
        for c in 0..2 {
            for v in 0..2 {
                self.regen[c][v].reset();
                self.input_sh[c][v].reset();
            }
            self.loop_hp[c].reset();
            self.loop_lp[c].reset();
            self.loop_dc[c] = (0.0, 0.0);
        }
        self.fb = [0.0; 2];
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(2);
        let n = buffers.first().map_or(0, |b| b.len());
        let sr = self.sr;
        let v = self.s.v;
        let on = v[p::BYPASS] < 0.5;
        let trails = v[p::TRAILS] >= 0.5;
        if on != self.was_on {
            self.in_g.set_target(if on { 1.0 } else { 0.0 });
            self.wet_g.set_target(if on || trails { 1.0 } else { 0.0 });
            self.was_on = on;
        }
        let swell_ms = v[p::SWELL];
        let detune = v[p::DETUNE] / 100.0;
        let pan = v[p::VOICE_PAN] * 0.01;
        // the pitch glide: `glide` ms to reach a new interval
        let gk = if v[p::GLIDE] < 1.0 { 0.0 } else { (-1.0 / (v[p::GLIDE] * 0.001 * sr)).exp() };
        let reverse = self.character == 1;
        for i in 0..n {
            self.clock += 1;
            if self.ctrl == 0 {
                self.control();
            }
            self.ctrl = (self.ctrl + 1) % GRID;
            let g = self.gate.next();
            if g == 0.0 && self.gate.is_settled() {
                if self.pending {
                    self.space = idx(self.s.v[p::SPACE], 4);
                    self.character = idx(self.s.v[p::CHARACTER], 2);
                    self.pending = false;
                    if self.pending_space {
                        self.pending_space = false;
                        self.clear_tails();
                        self.key = [f64::NAN; 11];
                        self.control();
                        self.diff.settle();
                        self.bloom.settle();
                        self.fdn.settle();
                    } else {
                        for c in 0..2 {
                            for k in 0..2 {
                                self.regen[c][k].reset();
                                self.input_sh[c][k].reset();
                            }
                        }
                    }
                }
                self.gate.set_target(1.0);
            }
            let gs = 0.5 - 0.5 * (std::f32::consts::PI * g).cos();
            let mix = self.mix.next() * 0.01;
            self.decay.next();
            self.hicut.next();
            let shimmer = self.shimmer.next() * 0.01;
            let duck = self.duck.next() * 0.01;
            // pitch glide in semitones (one-pole per sample)
            let tgt1 = self.pitch.target();
            let tgt2 = self.pitch2.target();
            let p1 = tgt1 + (self.pitch.value() - tgt1) * gk;
            let p2 = tgt2 + (self.pitch2.value() - tgt2) * gk;
            self.pitch.set_immediate(p1);
            self.pitch.set_target(tgt1);
            self.pitch2.set_immediate(p2);
            self.pitch2.set_target(tgt2);
            let v2 = self.voice2.next() * 0.01;
            let pre_ms = self.predelay.next();
            let width = self.width.next() * 0.01;
            let dry_g = self.dry_g.next();
            let in_g = self.in_g.next() * self.freeze_in.next();
            let wet_g = self.wet_g.next();
            let regen = self.regen_amt.next();
            let input_amt = self.input_amt.next();
            let ti = self.tail_in.next();
            let bloom_v = self.bloom_s.next();
            if wet_g == 0.0 && self.wet_g.is_settled() && !on && !trails {
                self.clear_tails();
            }
            let x0 = buffers[0][i];
            let x1 = if nch > 1 { buffers[1][i] } else { x0 };
            // ducking: 0 … 100 % → 0 … 18 dB under −30 dBFS of playing (idle when off)
            let dt = if duck > 0.0 {
                let e = self.duck_env.next(x0.abs().max(x1.abs()));
                let edb = 20.0 * e.max(1e-6).log10();
                let over = ((edb + 30.0) / 6.0).clamp(0.0, 1.0);
                10f32.powf(-18.0 * duck * over / 20.0)
            } else {
                1.0
            };
            self.duck_gr = dt + (self.duck_gr - dt) * 0.999;
            let mut sw = 1.0;
            if swell_ms > 0.0 {
                let a = 0.5 * (x0.abs() + x1.abs());
                self.env_fast += (a - self.env_fast) * 0.05;
                self.env_slow += (a - self.env_slow) * 0.0005;
                if self.env_fast > 2.0 * self.env_slow + 1e-4 {
                    self.swell_g = 0.0;
                }
                self.swell_g = (self.swell_g + 1000.0 / (swell_ms * sr)).min(1.0);
                sw = self.swell_g * self.swell_g;
            }
            // input, predelay, input-placement voices
            let mut xin = [0.0f32; 2];
            for (c, xv) in [x0, x1].into_iter().enumerate() {
                self.pre[c].push(xv * in_g * sw);
                xin[c] = self.pre[c].read((pre_ms as f64 * 0.001 * sr as f64).max(0.0), Interp::Linear);
            }
            let det = [-0.5 * detune, 0.5 * detune];
            if input_amt > 0.0 {
                for c in 0..2 {
                    let s1 = &mut self.input_sh[c][0];
                    s1.set_reverse(reverse);
                    s1.set_semitones((p1 + det[c]) as f64);
                    let a = s1.process(xin[c]);
                    let s2 = &mut self.input_sh[c][1];
                    s2.set_reverse(reverse);
                    s2.set_semitones((p2 - det[c]) as f64);
                    let b = s2.process(xin[c]);
                    xin[c] += input_amt * shimmer * (a + v2 * b);
                }
            }
            // the tank: the input plus the regenerated, shifted tail
            let feed = [(xin[0] + self.fb[0]) * ti, (xin[1] + self.fb[1]) * ti];
            let (tl, tr) = if self.space == 3 {
                self.plate.process(0.5 * (feed[0] + feed[1]), &mut self.plate_in)
            } else {
                let mut d8 = [feed[0], feed[1], -feed[0], -feed[1], feed[0], -feed[1], feed[1], -feed[0]];
                self.diff.process(&mut d8);
                // bloom above 50 %: the diffuser feeds itself, the pad fades in. It always runs (a bloom
                // switched on would otherwise start from empty lines); only its share glides.
                let bl = ((bloom_v * 0.01 - 0.5) * 2.0).max(0.0);
                let gb = 0.85 * bl;
                let mut b = [0.0f32; 8];
                for c in 0..8 {
                    b[c] = d8[c] + gb * self.bloom_fb[c];
                }
                self.bloom.process(&mut b);
                self.bloom_fb = b;
                for c in 0..8 {
                    d8[c] = (1.0 - bl) * d8[c] + bl * b[c] * (1.0 - gb);
                }
                let mut in16 = [0.0f32; 16];
                for c in 0..16 {
                    in16[c] = if c < 8 { d8[c] } else { -d8[c - 8] } * 0.5;
                }
                self.fdn.process(&in16)
            };
            // regeneration: loop filters, two detuned voices (voice 1 left-leaning, voice 2 right-leaning)
            let tail = [tl, tr];
            for c in 0..2 {
                if regen <= 0.0 || shimmer <= 0.0 {
                    self.fb[c] = 0.0;
                    continue;
                }
                let f = self.loop_lp[c].process(self.loop_hp[c].process(tail[c]));
                let s1 = &mut self.regen[c][0];
                s1.set_reverse(reverse);
                s1.set_semitones((p1 + det[c]) as f64);
                let a = s1.process(f);
                // voice 2 only runs while it is heard; it restarts from silence when it comes back
                let b = if v2 > 1e-4 {
                    let s2 = &mut self.regen[c][1];
                    if self.v2_idle[c] {
                        s2.reset();
                        self.v2_idle[c] = false;
                    }
                    s2.set_reverse(reverse);
                    s2.set_semitones((p2 - det[c]) as f64);
                    s2.process(f)
                } else {
                    self.v2_idle[c] = true;
                    0.0
                };
                let (ga, gb) = if c == 0 { (1.0, 1.0 - pan) } else { (1.0 - pan, 1.0) };
                let y = ga * a + v2 * gb * b;
                // loop gain below 1 by construction: scaled by the tail's length, soft-limited, DC-free
                let amt = 0.95 * shimmer * regen / (1.0 + self.decay.value()).sqrt();
                let z = (amt * y).tanh();
                let (dx, dy) = &mut self.loop_dc[c];
                let o = z - *dx + 0.9995 * *dy;
                *dx = z;
                *dy = o;
                self.fb[c] = o;
            }
            let mut wet = [tl, tr];
            // the tone tilt is skipped while it sits exactly flat (50 %)
            if self.tone.value() != 50.0 {
                for c in 0..2 {
                    wet[c] = self.tone_hi[c].process(self.tone_lo[c].process(wet[c]));
                }
            }
            let mid = 0.5 * (wet[0] + wet[1]);
            let side = 0.5 * (wet[0] - wet[1]) * width;
            // equal-power dry / wet (the catalogue's law)
            let t = mix * std::f32::consts::FRAC_PI_2;
            let wl = self.wet_level.next();
            let wl = if wl == 0.0 { 1.0 } else { 10f32.powf(wl / 20.0) };
            let (dg, wg) = (t.cos() * dry_g, t.sin() * self.duck_gr * wet_g * gs * wl);
            let (ol, or) = (x0 * dg + (mid + side) * wg, x1 * dg + (mid - side) * wg);
            if nch > 1 {
                buffers[0][i] = ol;
                buffers[1][i] = or;
            } else {
                buffers[0][i] = 0.5 * (ol + or);
            }
        }
    }
}

impl Default for Shimmer {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Shimmer {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        for e in [&mut self.mix, &mut self.decay, &mut self.shimmer, &mut self.hicut, &mut self.duck] {
            e.prepare(sample_rate);
        }
        self.size.prepare(sample_rate / GRID as f32);
        self.tone.prepare(sample_rate / GRID as f32);
        for o in [&mut self.pitch, &mut self.pitch2, &mut self.voice2, &mut self.predelay, &mut self.width, &mut self.dry_g, &mut self.freeze_in, &mut self.regen_amt, &mut self.input_amt, &mut self.bloom_s, &mut self.wet_level] {
            o.prepare(sample_rate);
        }
        for r in [&mut self.in_g, &mut self.wet_g, &mut self.gate, &mut self.tail_in] {
            r.prepare(sample_rate);
        }
        self.duck_env.prepare(sample_rate);
        for c in 0..2 {
            self.pre[c].prepare((0.51 * sample_rate as f64) as usize);
            for v in 0..2 {
                self.regen[c][v].prepare(sample_rate, 60.0 + 7.0 * (2 * c + v) as f32);
                self.input_sh[c][v].prepare(sample_rate, 55.0 + 5.0 * (2 * c + v) as f32);
            }
        }
        self.diff.prepare(sample_rate, 4, 45.0, 0x5D1F);
        self.bloom.prepare(sample_rate, 4, 60.0, 0x5B10);
        self.fdn.prepare(sample_rate, SPACE_SCALE[2] + 0.1);
        self.plate.prepare(sample_rate, 1.5);
    }

    fn reset(&mut self) {
        let v = self.s.v;
        for (i, e) in [(p::MIX, &mut self.mix), (p::DECAY, &mut self.decay), (p::SHIMMER, &mut self.shimmer), (p::HICUT, &mut self.hicut), (p::DUCK, &mut self.duck)] {
            e.settle(v[i].clamp(PARAMS[i].min, PARAMS[i].max));
        }
        for o in [&mut self.pitch, &mut self.pitch2, &mut self.voice2, &mut self.size, &mut self.predelay, &mut self.width, &mut self.dry_g, &mut self.freeze_in, &mut self.regen_amt, &mut self.input_amt, &mut self.bloom_s, &mut self.wet_level] {
            o.settle();
        }
        self.space = idx(v[p::SPACE], 4);
        self.character = idx(v[p::CHARACTER], 2);
        self.pending = false;
        self.pending_space = false;
        self.key = [f64::NAN; 11];
        self.tone.set_target(v[p::TONE]);
        self.tone.settle();
        self.gate.set_immediate(1.0);
        let on = v[p::BYPASS] < 0.5;
        self.was_on = on;
        self.in_g.set_immediate(if on { 1.0 } else { 0.0 });
        self.wet_g.set_immediate(if on || v[p::TRAILS] >= 0.5 { 1.0 } else { 0.0 });
        for c in 0..2 {
            self.pre[c].reset();
            self.tone_lo[c].reset();
            self.tone_hi[c].reset();
        }
        self.diff.reset();
        self.clear_tails();
        self.tail_in.set_immediate(1.0);
        self.control();
        self.diff.settle();
        self.bloom.settle();
        self.fdn.settle();
        self.duck_env.reset();
        self.duck_gr = 1.0;
        self.swell_g = 1.0;
        self.env_fast = 0.0;
        self.env_slow = 0.0;
        self.ctrl = 0;
        self.clock = 0;
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
