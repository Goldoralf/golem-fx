// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-radio — golem's small-speaker and transmission pedal (design: README.md).
//!
//! A simulated device chain, not a band-pass: microphone → transmitter and channel → receiver amplifier
//! → loudspeaker → room. Each stage is physical and has zero latency:
//!
//! - the mic is a sandwich (pre-filter, a memoryless curve at 2×, post-filter); the carbon button's
//!   rational curve gives its 2nd harmonic, its frying noise follows the speech;
//! - AM and narrowband FM run in complex baseband at the host rate (gfx-dsp `baseband`): detuning
//!   distortion, the birdie, selective fading, overmodulation, FM threshold clicks and the squelch tail
//!   come out of the simulation; the cell phone is a backward-adaptive LPC coder on the 8 kHz virtual
//!   samples of gfx-dsp's `converter` (gfx-dsp `codec`), with bursty losses concealed from the decoder's
//!   own output;
//! - the amplifier and the speaker's Klippel motor share one oversampled island (4× for the hard-rail
//!   amps, 2× for the others); horns add the throat's air nonlinearity; the body is matched sections;
//! - the room is shared by the chains, so a model change keeps its tail.
//!
//! Keying: a 20 ms sliding maximum against `squelch` (4 dB hysteresis, 300 ms × `squelch_tail` hang); in
//! silence each channel closes its own way and its noises stop. Reported latency 0. Real-time safe,
//! deterministic (every source seeded, reset at `reset`).

#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

pub mod chain;
pub mod channel;
pub mod room;
pub mod voicing;

#[cfg(feature = "plugin")]
mod plugin;

#[cfg(test)]
mod tests;

use chain::{Cfg, Chain};
use channel::db;
use gfx_dsp::blep::Kernel;
use gfx_dsp::codec::{GilbertElliott, FRAME};
use gfx_dsp::converter::Clock;
use gfx_dsp::envelope::SlidingMax;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::smooth::OnePole;
use gfx_dsp::{Effect, ParamInfo};
use room::RoomSlot;
use voicing::{Chan, ChanCfg, Mic, Room, MODELS, PHONE_RECEIVER};

pub const NP: usize = 23;

/// The reference programme level: −18 dBFS RMS.
pub const REF: f64 = 0.125_892_541_179_416_72;

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::enumeration("model", "Model", 10, 0),
    ParamInfo::float("drive", "Drive", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("noise", "Noise", 0.0, 100.0, 50.0).with_exp(),
    ParamInfo::float("distance", "Distance", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("tune", "Tune", -100.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("squelch", "Squelch", 0.0, 100.0, 25.0),
    ParamInfo::enumeration("mic", "Mic", 6, 0),
    ParamInfo::enumeration("channel", "Channel", 7, 0),
    ParamInfo::enumeration("speaker", "Speaker", 11, 0),
    ParamInfo::enumeration("room", "Room", 5, 0),
    ParamInfo::float("band", "Bandwidth", -100.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("resonance", "Resonance", 0.0, 200.0, 100.0),
    ParamInfo::float("codec", "Codec", 0.0, 100.0, 50.0),
    ParamInfo::float("dropouts", "Dropouts", 0.0, 100.0, 0.0),
    ParamInfo::float("whistle", "Whistle", -80.0, -10.0, -80.0),
    ParamInfo::float("squelch_tail", "Tail", 0.0, 200.0, 100.0),
    ParamInfo::boolean("roger", "Roger", false),
    ParamInfo::float("battery", "Battery", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::float("rattle", "Rattle", 0.0, 100.0, 0.0),
    ParamInfo::enumeration("mains", "Mains", 2, 0),
    ParamInfo::float("level", "Level", -24.0, 12.0, 0.0).with_exp(),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::boolean("bypass", "Bypass", false),
];

pub mod p {
    pub const MODEL: usize = 0;
    pub const DRIVE: usize = 1;
    pub const NOISE: usize = 2;
    pub const DISTANCE: usize = 3;
    pub const TUNE: usize = 4;
    pub const SQUELCH: usize = 5;
    pub const MIC: usize = 6;
    pub const CHANNEL: usize = 7;
    pub const SPEAKER: usize = 8;
    pub const ROOM: usize = 9;
    pub const BAND: usize = 10;
    pub const RESONANCE: usize = 11;
    pub const CODEC: usize = 12;
    pub const DROPOUTS: usize = 13;
    pub const WHISTLE: usize = 14;
    pub const TAIL: usize = 15;
    pub const ROGER: usize = 16;
    pub const BATTERY: usize = 17;
    pub const RATTLE: usize = 18;
    pub const MAINS: usize = 19;
    pub const LEVEL: usize = 20;
    pub const MIX: usize = 21;
    pub const BYPASS: usize = 22;
}

/// Option labels (enum order, final: append only).
pub const MODELS_L: [&str; 10] = ["Telephone", "Cell Phone", "Megaphone", "Walkie-Talkie", "AM Radio", "PA Horn", "Intercom", "Pocket Radio", "Gramophone", "Laptop"];
pub const MICS: [&str; 6] = ["Model", "Clean", "Carbon", "Dynamic", "Crystal", "Electret"];
pub const CHANNELS: [&str; 7] = ["Model", "None", "Phone Line", "Digital Line", "Cell Codec", "AM", "NBFM"];
pub const SPEAKERS_L: [&str; 11] = ["Model", "None", "Handset", "Megaphone Horn", "Walkie Speaker", "Table Radio", "PA Horn", "Intercom", "Pocket Radio", "Gramophone Horn", "Laptop"];
pub const ROOMS: [&str; 5] = ["Model", "None", "Small Room", "Hall", "Street"];
pub const MAINS: [&str; 2] = ["50 Hz", "60 Hz"];

const MAX_CH: usize = 2;
/// Control grid (samples): filter redesigns and structure checks, counted from reset.
const CTL: u32 = 16;
/// A chain switch: the new chain runs unheard, then crossfades (s).
const WARM_S: f64 = 0.010;
const XFADE_S: f64 = 0.030;
/// The old room rings out at most this long (s).
const ROOM_OUT_S: f64 = 3.0;
/// Keying: hysteresis (dB), hang (s at `squelch_tail` 100 %).
const KEY_HYST: f64 = 0.630_957_344_480_193_2; // −4 dB
const KEY_HANG_S: f64 = 0.300;

/// The virtual 8 kHz instants of one host sample, with their frame positions and loss flags.
#[derive(Clone, Copy, Debug, Default)]
pub struct VInst {
    pub n: usize,
    pub d: [f64; 2],
    pub pos: [usize; 2],
    pub lost: [bool; 2],
}

/// What every chain reads at one host sample.
#[derive(Clone, Copy, Debug)]
pub struct Ctx {
    /// `drive` 0 – 1 and its gain into the amp (0 … +30 dB)
    pub drive: f64,
    pub dg: f64,
    /// `noise` in %
    pub noise: f64,
    pub distance: f64,
    /// `tune` −1 … 1
    pub tune: f64,
    /// `band` in octaves (−1 … 1) and `resonance` as a factor (0 – 2)
    pub band: f64,
    pub resonance: f64,
    /// the codec law: LAR step, excitation mix, deepest cut (dB)
    pub codec: (f64, f64, f64),
    pub dropouts: f64,
    /// the spur's amplitude re the carrier (0 = off)
    pub whistle: f64,
    pub tail: f64,
    pub roger: bool,
    pub battery: f64,
    pub rattle: f64,
    pub mains: f64,
    pub keyed: bool,
    /// `squelch` > 0 (the NBFM receiver's squelch may mute)
    pub squelch_on: bool,
    pub room: Room,
    pub ins: VInst,
    /// derived from `noise` (once per change): the noise gain 10^(0.6 (n − 50) / 20) with its fade to 0
    /// under 10 % (`nf`) and without it (`ng`); the noise variance factor 10^(0.6 (n − 50) / 10)
    pub nf: f64,
    pub ng: f64,
    pub vk: f64,
    /// derived from `drive`: the AM modulation index, the mic gains (+12 dB / +6 dB at 100 %)
    pub am_m: f64,
    pub mic_g12: f64,
    pub mic_g6: f64,
}

/// A chain switch in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Switch {
    None,
    Warm(u32),
    Fade(u32),
}

pub struct Radio {
    v: [f32; NP],
    sr: f64,
    nch: usize,
    kernel: &'static Kernel,
    chains: Vec<[Chain; 2]>,
    cur: usize,
    switch: Switch,
    rooms: [RoomSlot; 2],
    room_cur: usize,
    /// whether the other room is still ringing out
    room_old: bool,
    clock: Clock,
    vcount: u64,
    ge: GilbertElliott,
    lost: bool,
    key_det: SlidingMax,
    keyed: bool,
    /// (`squelch`, its threshold as a level)
    key_thr: (f64, f64),
    hang: u64,
    now: u64,
    ctl: u32,
    // EXP-capable
    drive: ExpSmoother,
    noise: ExpSmoother,
    distance: ExpSmoother,
    tune: ExpSmoother,
    band: ExpSmoother,
    battery: ExpSmoother,
    level: ExpSmoother,
    mix: ExpSmoother,
    // the others
    squelch: OnePole,
    resonance: OnePole,
    dropouts: OnePole,
    whistle: OnePole,
    rattle: OnePole,
    bypass: f64,
    // derived
    dg: f64,
    out_g: f64,
    nf: f64,
    ng: f64,
    vk: f64,
    am_m: f64,
    mic_g: (f64, f64),
    designed: (f64, f64, f64),
}

impl Default for Radio {
    fn default() -> Self {
        Self::new()
    }
}

impl Radio {
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
            kernel: Kernel::get(),
            chains: Vec::new(),
            cur: 0,
            switch: Switch::None,
            rooms: [RoomSlot::new(), RoomSlot::new()],
            room_cur: 0,
            room_old: false,
            clock: Clock::default(),
            vcount: 0,
            ge: GilbertElliott::default(),
            lost: false,
            key_det: SlidingMax::new(),
            keyed: true,
            key_thr: (f64::NAN, 0.0),
            hang: 0,
            now: 0,
            ctl: 0,
            drive: exp(p::DRIVE),
            noise: exp(p::NOISE),
            distance: exp(p::DISTANCE),
            tune: exp(p::TUNE),
            band: exp(p::BAND),
            battery: exp(p::BATTERY),
            level: exp(p::LEVEL),
            mix: exp(p::MIX),
            squelch: OnePole::new(30.0, 25.0, 1e-3),
            resonance: OnePole::new(50.0, 100.0, 1e-3),
            dropouts: OnePole::new(50.0, 0.0, 1e-4),
            whistle: OnePole::new(30.0, -80.0, 1e-3),
            rattle: OnePole::new(50.0, 0.0, 1e-4),
            bypass: 0.0,
            dg: 1.0,
            out_g: 1.0,
            nf: 1.0,
            ng: 1.0,
            vk: 1.0,
            am_m: channel::AM_M,
            mic_g: (1.0, 1.0),
            designed: (f64::NAN, f64::NAN, f64::NAN),
        }
    }

    fn exps(&mut self) -> [(&mut ExpSmoother, usize); 8] {
        [
            (&mut self.drive, p::DRIVE),
            (&mut self.noise, p::NOISE),
            (&mut self.distance, p::DISTANCE),
            (&mut self.tune, p::TUNE),
            (&mut self.band, p::BAND),
            (&mut self.battery, p::BATTERY),
            (&mut self.level, p::LEVEL),
            (&mut self.mix, p::MIX),
        ]
    }

    fn poles(&mut self) -> [(&mut OnePole, usize); 5] {
        [(&mut self.squelch, p::SQUELCH), (&mut self.resonance, p::RESONANCE), (&mut self.dropouts, p::DROPOUTS), (&mut self.whistle, p::WHISTLE), (&mut self.rattle, p::RATTLE)]
    }

    fn opt(&self, i: usize) -> usize {
        self.v[i].round().max(0.0) as usize
    }

    /// The chain the parameters ask for (`Model` options resolve to the model's own).
    fn target_cfg(&self) -> Cfg {
        let model = self.opt(p::MODEL).min(9);
        let m = &MODELS[model];
        let mic = match self.opt(p::MIC) {
            0 => m.mic,
            k => Mic::from_option(k),
        };
        let chan = match self.opt(p::CHANNEL) {
            0 => m.chan,
            k => ChanCfg { kind: Chan::from_option(k), short: false },
        };
        let speaker = match self.opt(p::SPEAKER) {
            0 => m.speaker,
            1 => 0,
            k => (k - 1).min(PHONE_RECEIVER - 1),
        };
        Cfg { model, mic, chan, speaker }
    }

    fn target_room(&self) -> Room {
        match self.opt(p::ROOM) {
            0 => MODELS[self.opt(p::MODEL).min(9)].room,
            k => Room::from_option(k),
        }
    }

    /// The keying state (true: keyed / squelch open), as the `state` meter shows it.
    pub fn is_keyed(&self) -> bool {
        self.keyed && self.chains.first().is_none_or(|c| c[self.cur].squelch_open())
    }

    /// The current chain (tests).
    pub fn chain(&self, ch: usize) -> &Chain {
        &self.chains[ch][self.cur]
    }

    /// Tests: the cell codec's predictor and bells removed.
    pub fn set_codec_bypass(&mut self, on: bool) {
        for c in self.chains.iter_mut() {
            for s in c.iter_mut() {
                s.set_codec_bypass(on);
            }
        }
    }

    fn ctx(&self, ins: VInst) -> Ctx {
        let wh = self.whistle.value() as f64;
        let (q, a, cut) = gfx_dsp::codec::law(self.v[p::CODEC] as f64);
        Ctx {
            drive: self.drive.value() as f64 / 100.0,
            dg: self.dg,
            noise: self.noise.value() as f64,
            distance: self.distance.value() as f64 / 100.0,
            tune: self.tune.value() as f64 / 100.0,
            band: self.band.value() as f64 / 100.0,
            resonance: self.resonance.value() as f64 / 100.0,
            codec: (q, a, cut),
            dropouts: self.dropouts.value() as f64 / 100.0,
            whistle: if wh <= -79.95 { 0.0 } else { db(wh) * ((wh + 80.0) / 0.05).min(1.0) },
            tail: self.v[p::TAIL] as f64 / 100.0,
            roger: self.v[p::ROGER] > 0.5,
            battery: self.battery.value() as f64 / 100.0,
            rattle: self.rattle.value() as f64 / 100.0,
            mains: if self.opt(p::MAINS) == 1 { 60.0 } else { 50.0 },
            keyed: self.keyed,
            squelch_on: self.v[p::SQUELCH] > 0.0,
            room: self.rooms[self.room_cur].kind,
            ins,
            nf: self.nf,
            ng: self.ng,
            vk: self.vk,
            am_m: self.am_m,
            mic_g12: self.mic_g.0,
            mic_g6: self.mic_g.1,
        }
    }

    fn update_derived(&mut self) {
        let d = self.drive.value() as f64 / 100.0;
        self.dg = db(30.0 * d);
        self.out_g = db(self.level.value() as f64);
        let n = self.noise.value() as f64;
        self.ng = db(0.6 * (n - 50.0));
        self.nf = if n <= 0.0 { 0.0 } else { self.ng * (n / 10.0).min(1.0) };
        self.vk = self.ng * self.ng;
        self.am_m = channel::AM_M * (2.5 / channel::AM_M).powf(d);
        self.mic_g = (db(12.0 * d), db(6.0 * d));
    }

    fn reset_audio(&mut self) {
        let cfg = self.target_cfg();
        let c = self.ctx(VInst::default());
        for ch in self.chains.iter_mut() {
            for s in ch.iter_mut() {
                s.configure(cfg, &c);
            }
        }
        self.cur = 0;
        self.switch = Switch::None;
        let rk = self.target_room();
        for r in self.rooms.iter_mut() {
            r.configure(rk);
            r.reset();
            r.gin = 0.0;
        }
        self.room_cur = 0;
        self.rooms[0].gin = 1.0;
        self.room_old = false;
        self.clock.reset();
        self.vcount = 0;
        self.ge.reset();
        self.lost = false;
        self.key_det.reset();
        self.keyed = true;
        self.hang = 0;
        self.ctl = 0;
        self.designed = (self.band.value() as f64, self.resonance.value() as f64, self.distance.value() as f64);
    }

    /// Starts a chain switch: the other slot gets the new structure, reset, and runs unheard first.
    fn start_switch(&mut self, cfg: Cfg) {
        let o = 1 - self.cur;
        let c = self.ctx(VInst::default());
        for ch in self.chains.iter_mut() {
            ch[o].configure(cfg, &c);
        }
        self.switch = Switch::Warm(0);
    }

    fn start_room(&mut self, kind: Room) {
        let o = 1 - self.room_cur;
        self.rooms[o].configure(kind);
        self.rooms[o].reset();
        self.rooms[o].gin = 0.0;
        self.rooms[self.room_cur].age = 0;
        self.room_cur = o;
        self.room_old = true;
    }

    /// Keying on the transmitter input's 20 ms sliding maximum.
    #[inline]
    fn key(&mut self, level: f32) {
        let sq = self.squelch.value() as f64;
        let peak = self.key_det.next(level) as f64;
        if sq <= 0.0 {
            self.keyed = true;
            self.hang = 0;
            return;
        }
        if sq != self.key_thr.0 {
            self.key_thr = (sq, db(-80.0 + 0.6 * sq));
        }
        let thr = self.key_thr.1;
        if peak > thr {
            self.keyed = true;
            self.hang = (KEY_HANG_S * (self.v[p::TAIL] as f64 / 100.0) * self.sr) as u64;
        } else if peak < thr * KEY_HYST {
            if self.hang > 0 {
                self.hang -= 1;
            } else {
                self.keyed = false;
            }
        }
    }
}

impl Effect for Radio {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate as f64;
        self.nch = channels.clamp(1, MAX_CH);
        self.kernel = Kernel::get();
        let sr = self.sr;
        self.chains = (0..self.nch)
            .map(|_| {
                let mut a = Chain::new();
                a.prepare(sr);
                let b = a.clone();
                [a, b]
            })
            .collect();
        for r in self.rooms.iter_mut() {
            r.prepare(sr);
        }
        let win = (0.020 * sr).round() as usize;
        self.key_det.prepare(win);
        self.key_det.set_window(win);
        for (s, _) in self.exps() {
            s.prepare(sample_rate);
        }
        for (s, _) in self.poles() {
            s.prepare(sample_rate);
        }
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
        self.bypass = if v[p::BYPASS] > 0.5 { 1.0 } else { 0.0 };
        self.now = 0;
        self.update_derived();
        self.ge.set(0.003 * self.dropouts.value() as f64, 2.0);
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
    }

    fn meter(&self, id: &str) -> Option<f32> {
        (id == "state").then(|| if self.is_keyed() { 1.0 } else { 0.0 })
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(self.chains.len());
        if nch == 0 {
            return;
        }
        let n = buffers[0].len();
        let sr = self.sr;
        let kernel = self.kernel;
        let byp_target = if self.v[p::BYPASS] > 0.5 { 1.0 } else { 0.0 };
        let byp_step = 1.0 / (0.010 * sr);
        let warm_n = (WARM_S * sr) as u32;
        let fade_n = (XFADE_S * sr) as u32;
        let room_in_step = 1.0 / (XFADE_S * sr);
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
            if moving {
                self.update_derived();
            }
            if self.bypass != byp_target {
                self.bypass = if byp_target > self.bypass { (self.bypass + byp_step).min(1.0) } else { (self.bypass - byp_step).max(0.0) };
            }
            let mut lvl = 0.0f32;
            for c in 0..nch {
                lvl = lvl.max(buffers[c][i].abs());
            }
            self.key(lvl);

            // ---- control grid: structure and filter designs
            if self.ctl == 0 {
                let cfg = self.target_cfg();
                if self.switch == Switch::None && cfg != self.chains[0][self.cur].cfg {
                    self.start_switch(cfg);
                }
                let rk = self.target_room();
                if !self.room_old && rk != self.rooms[self.room_cur].kind {
                    self.start_room(rk);
                }
                let d = (self.band.value() as f64, self.resonance.value() as f64, self.distance.value() as f64);
                if d != self.designed {
                    let c = self.ctx(VInst::default());
                    let tone = d.0 != self.designed.0 || d.1 != self.designed.1;
                    for ch in self.chains[..nch].iter_mut() {
                        for s in ch.iter_mut() {
                            if tone {
                                s.design(&c, CTL);
                            }
                            if d.2 != self.designed.2 {
                                s.design_distance(&c);
                            }
                        }
                    }
                    self.designed = d;
                }
                self.ge.set(0.003 * self.dropouts.value() as f64, 2.0);
            }
            self.ctl = (self.ctl + 1) % CTL;

            // ---- the 8 kHz virtual clock, frames and losses (one for every chain)
            let t = self.clock.tick(8000.0 / sr, 0.0);
            let mut ins = VInst { n: t.n, d: t.d, ..VInst::default() };
            for j in 0..t.n {
                let pos = (self.vcount % FRAME as u64) as usize;
                if pos == 0 {
                    self.lost = self.ge.step();
                }
                ins.pos[j] = pos;
                ins.lost[j] = self.lost;
                self.vcount += 1;
            }

            let c = self.ctx(ins);

            let mix = self.mix.value() as f64 / 100.0;
            let b = self.bypass;
            let feed = (1.0 - b) as f32;
            let (sw, cur) = (self.switch, self.cur);
            let a = match sw {
                Switch::Fade(k) => (k as f32 + 0.5) / fade_n.max(1) as f32,
                _ => 0.0,
            };
            let mut direct = [0.0f32; MAX_CH];
            let mut send = [0.0f32; MAX_CH];
            for k in 0..nch {
                let x = buffers[k][i] * feed;
                let (d0, s0) = self.chains[k][cur].process(kernel, x, &c);
                let (d, s) = match sw {
                    Switch::None => (d0, s0),
                    Switch::Warm(_) => {
                        let _ = self.chains[k][1 - cur].process(kernel, x, &c);
                        (d0, s0)
                    }
                    Switch::Fade(_) => {
                        let (d1, s1) = self.chains[k][1 - cur].process(kernel, x, &c);
                        (d0 + (d1 - d0) * a, s0 + (s1 - s0) * a)
                    }
                };
                direct[k] = d;
                send[k] = s;
            }
            self.switch = match sw {
                Switch::None => Switch::None,
                Switch::Warm(k) if k + 1 >= warm_n => Switch::Fade(0),
                Switch::Warm(k) => Switch::Warm(k + 1),
                Switch::Fade(k) if k + 1 >= fade_n => {
                    self.cur = 1 - self.cur;
                    Switch::None
                }
                Switch::Fade(k) => Switch::Fade(k + 1),
            };

            // ---- the room(s): the new one fills while the old one rings out
            let (sl, sr_) = (send[0], if nch > 1 { send[1] } else { send[0] });
            let rc = self.room_cur;
            let (mut rl, mut rr_) = (0.0f32, 0.0f32);
            if self.rooms[rc].kind != Room::None {
                if self.rooms[rc].gin < 1.0 {
                    self.rooms[rc].gin = (self.rooms[rc].gin + room_in_step).min(1.0);
                }
                let (l, r) = self.rooms[rc].process(sl, sr_);
                rl += l;
                rr_ += r;
            }
            if self.room_old {
                let o = &mut self.rooms[1 - rc];
                o.gin = (o.gin - room_in_step).max(0.0);
                if o.kind != Room::None {
                    let (l, r) = o.process(sl, sr_);
                    rl += l;
                    rr_ += r;
                }
                o.age += 1;
                if o.kind == Room::None || (o.gin == 0.0 && o.quiet) || o.age as f64 > ROOM_OUT_S * sr {
                    self.room_old = false;
                    o.reset();
                }
            }

            // ---- output: level, mix (a layering control, README §2.7), bypass with trails
            let og = self.out_g;
            for k in 0..nch {
                let dry = buffers[k][i] as f64;
                let room = if k == 0 { rl } else { rr_ } as f64;
                let wet = (direct[k] as f64 + room) * og;
                buffers[k][i] = (dry * (1.0 - mix * (1.0 - b)) + wet * mix) as f32;
            }
            self.now += 1;
        }
    }
}
