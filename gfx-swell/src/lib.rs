// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-swell — golem's auto-swell ("slow gear") (design: README.md).
//!
//! - **Detection** (gfx-dsp `onset`), on the undelayed main or key signal, all relative to the
//!   playing's level: the level gate L after a reset, picks (A), shape flux (B) and pitch jumps (C)
//!   under `legato`, one swell per strum.
//! - **Gain** (gfx-dsp `swell`): five curves, rise in ms or note values, bow decay, retrigger dip
//!   (0.5 ms under a pick, 8 ms otherwise), reset to the start level once the note has decayed `reset`
//!   dB under its body: the next pick meets a closed gain.
//! - **Lookahead** Off · 3 · 10 · 25 ms, reported: the audio is delayed and each dip is placed to end
//!   where its onset is. Crossfaded over 20 ms on change.
//!
//! Zero latency by default. Real-time safe, deterministic, block-size independent.

#![allow(clippy::needless_range_loop)]

#[cfg(feature = "plugin")]
mod plugin;

#[cfg(test)]
mod tests;

use gfx_dsp::delay::DelayLine;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::onset::{Flux, Level, LevelGate, Pick, PickSettings, PitchJump, Source, Strum, DB_FLOOR};
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::swell::{Curve, Engine, Shape, State};
use gfx_dsp::transport::{Sync, TransportInfo};
use gfx_dsp::{Effect, ParamInfo};

pub const NP: usize = 17;

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::float("rise", "Rise", 5.0, 4000.0, 300.0).with_exp(),
    ParamInfo::enumeration("sync", "Sync", 17, 0),
    ParamInfo::float("sens", "Sensitivity", 0.0, 100.0, 50.0),
    ParamInfo::float("legato", "Legato", 0.0, 100.0, 50.0),
    ParamInfo::enumeration("curve", "Curve", 5, 0),
    ParamInfo::float("depth", "Depth", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::float("dip", "Retrigger Dip", 0.0, 100.0, 100.0),
    ParamInfo::float("strum", "Strum Window", 20.0, 300.0, 60.0),
    ParamInfo::float("decay", "Decay (Bow)", 0.0, 4000.0, 0.0).with_exp(),
    ParamInfo::float("reset", "Reset", 6.0, 60.0, 30.0),
    ParamInfo::float("release", "Reset Fade", 10.0, 2000.0, 60.0),
    ParamInfo::float("thresh", "Noise Floor", -96.0, -30.0, -66.0),
    ParamInfo::enumeration("source", "Detector Source", 2, 0),
    ParamInfo::enumeration("lookahead", "Lookahead", 4, 0),
    ParamInfo::boolean("fire", "Fire", false),
    ParamInfo::float("level", "Level", -24.0, 12.0, 0.0).with_exp(),
    ParamInfo::boolean("bypass", "Bypass", false),
];

pub mod p {
    pub const RISE: usize = 0;
    pub const SYNC: usize = 1;
    pub const SENS: usize = 2;
    pub const LEGATO: usize = 3;
    pub const CURVE: usize = 4;
    pub const DEPTH: usize = 5;
    pub const DIP: usize = 6;
    pub const STRUM: usize = 7;
    pub const DECAY: usize = 8;
    pub const RESET: usize = 9;
    pub const RELEASE: usize = 10;
    pub const THRESH: usize = 11;
    pub const SOURCE: usize = 12;
    pub const LOOKAHEAD: usize = 13;
    pub const FIRE: usize = 14;
    pub const LEVEL: usize = 15;
    pub const BYPASS: usize = 16;
}

/// Curve option labels (enum order, final: append only).
pub const CURVES: [&str; 5] = ["Square", "Linear", "S-Curve", "dB", "Pedal"];
/// Lookahead options (enum order, final), in ms.
pub const LOOKAHEAD_MS: [f32; 4] = [0.0, 3.0, 10.0, 25.0];

const MAX_CH: usize = 2;
const EVENTS: usize = 256;

/// An onset, as the tests and the debug witness see it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub src: Source,
    /// sample index since reset (the sample where it was detected)
    pub at: u64,
    /// started a swell (else absorbed by the strum window / hold-off)
    pub accepted: bool,
}

#[inline]
fn idx(v: f32, n: usize) -> usize {
    (v.round().max(0.0) as usize).min(n - 1)
}

pub struct Swell {
    v: [f32; NP],
    sr: f32,
    transport: TransportInfo,
    // detection
    level: Level,
    gate: LevelGate,
    pick: Pick,
    flux: Flux,
    pitch: PitchJump,
    strum: Strum,
    legato_on: bool,
    key_silent: u64,
    // gain
    engine: Engine,
    depth: ExpSmoother,
    depth_s: OnePole,
    out_level: ExpSmoother,
    out_level_s: OnePole,
    /// the note body: max of the note level from 20 ms after the onset (dB)
    body_db: f32,
    since_onset: u64,
    // audio path
    delay: [DelayLine; MAX_CH],
    la_cur: usize,
    la_prev: usize,
    la_fade: LinearRamp,
    bypass_g: LinearRamp,
    was_bypassed: bool,
    was_fire: bool,
    clock: u64,
    // meters, witness
    gain: f32,
    gr_db: f32,
    onsets: u64,
    detected: [u64; 5],
    events: [Event; EVENTS],
    ev_n: usize,
}

impl Swell {
    pub fn new() -> Self {
        let v: [f32; NP] = core::array::from_fn(|i| PARAMS[i].default);
        Self {
            v,
            sr: 48000.0,
            transport: TransportInfo::default(),
            level: Level::default(),
            gate: LevelGate::default(),
            pick: Pick::default(),
            flux: Flux::default(),
            pitch: PitchJump::default(),
            strum: Strum::default(),
            legato_on: v[p::LEGATO] > 0.0,
            key_silent: 0,
            engine: Engine::new(),
            depth: ExpSmoother::new(0.0, 100.0, v[p::DEPTH]),
            depth_s: OnePole::new(20.0, v[p::DEPTH], 1e-4),
            out_level: ExpSmoother::new(-24.0, 12.0, v[p::LEVEL]),
            out_level_s: OnePole::new(20.0, v[p::LEVEL], 1e-5),
            body_db: DB_FLOOR,
            since_onset: 0,
            delay: Default::default(),
            la_cur: 0,
            la_prev: 0,
            la_fade: LinearRamp::new(20.0, 1.0),
            bypass_g: LinearRamp::new(5.0, 1.0),
            was_bypassed: false,
            was_fire: false,
            clock: 0,
            gain: 0.0,
            gr_db: 0.0,
            onsets: 0,
            detected: [0; 5],
            events: [Event { src: Source::Level, at: 0, accepted: false }; EVENTS],
            ev_n: 0,
        }
    }

    /// The current gain of the swell (without `level`), linear.
    pub fn gain(&self) -> f32 {
        self.gain
    }

    /// `state` meter: 0 Idle, 1 Rise, 2 Hold, 3 Decay.
    pub fn state(&self) -> State {
        self.engine.state()
    }

    /// `gr` meter: the current attenuation (dB, ≥ 0).
    pub fn gr_db(&self) -> f32 {
        self.gr_db
    }

    /// `onsets_count`: swells started since reset.
    pub fn onsets_count(&self) -> u64 {
        self.onsets
    }

    /// Detections per source (L, A, B, C, manual), absorbed or not.
    pub fn detected(&self) -> [u64; 5] {
        self.detected
    }

    /// Moves the onset log out (oldest first) and clears it. Holds the last 256.
    pub fn take_events(&mut self, out: &mut Vec<Event>) {
        let n = self.ev_n.min(EVENTS);
        let start = self.ev_n - n;
        for k in start..self.ev_n {
            out.push(self.events[k % EVENTS]);
        }
        self.ev_n = 0;
    }

    fn la_samples(&self, i: usize) -> usize {
        (LOOKAHEAD_MS[i] * 0.001 * self.sr).round() as usize
    }

    fn pick_settings(&self) -> PickSettings {
        PickSettings { rise_db: Pick::rise_for(self.v[p::SENS] * 0.01), ..PickSettings::default() }
    }

    /// The rise and decay of a swell starting now (samples).
    fn shape(&self) -> Shape {
        let v = &self.v;
        let ms = 0.001 * self.sr as f64;
        let rise = match Sync::from_index(idx(v[p::SYNC], 17)).beats() {
            Some(b) => self.transport.note_samples(b, self.sr),
            None => v[p::RISE].clamp(5.0, 4000.0) as f64 * ms,
        };
        Shape { curve: Curve::from_index(idx(v[p::CURVE], 5)), rise, decay: v[p::DECAY].clamp(0.0, 4000.0) as f64 * ms }
    }

    fn log(&mut self, src: Source, accepted: bool) {
        self.events[self.ev_n % EVENTS] = Event { src, at: self.clock, accepted };
        self.ev_n += 1;
        if self.ev_n >= 2 * EVENTS {
            self.ev_n -= EVENTS;
        }
    }

    /// An onset from `src`, detected `lag` samples after it happened.
    fn onset(&mut self, src: Source, lag: f64, up: bool) {
        self.detected[src as usize] += 1;
        let accepted = self.strum.decide(src, self.v[p::STRUM], up);
        self.log(src, accepted);
        if !accepted {
            return;
        }
        self.onsets += 1;
        self.since_onset = 0;
        self.body_db = DB_FLOOR;
        let ms = 0.001 * self.sr as f64;
        // samples until the onset reaches the output (negative: it already has)
        let left = self.la_cur as f64 - lag;
        let shape = self.shape();
        let dip = (self.v[p::DIP] * 0.01).clamp(0.0, 1.0) as f64;
        if !self.engine.is_playing() {
            self.engine.trigger(shape, dip, 1.0, left.max(0.0).round() as u32);
            return;
        }
        let len = match src {
            // the pick's own transient masks a fast fade
            Source::Pick => left.min(3.0 * ms).max(0.5 * ms),
            _ => 8.0 * ms,
        };
        self.engine.trigger(shape, dip, len, (left - len).max(0.0).round() as u32);
    }

    pub fn process_with_key(&mut self, buffers: &mut [&mut [f32]], key: Option<&[&[f32]]>) {
        let nch = buffers.len().min(MAX_CH);
        let n = buffers.first().map_or(0, |b| b.len());
        let v = self.v;
        let sr = self.sr;
        // key fallback: a key silent for 1 s is not connected
        let mut key = if idx(v[p::SOURCE], 2) == 1 { key.filter(|k| !k.is_empty()) } else { None };
        if let Some(k) = key {
            if k.iter().any(|c| c.iter().any(|&s| s != 0.0)) {
                self.key_silent = 0;
            } else {
                self.key_silent += n as u64;
            }
            if self.key_silent >= sr as u64 {
                key = None;
            }
        }
        let bypassed = v[p::BYPASS] >= 0.5;
        if bypassed != self.was_bypassed {
            if !bypassed {
                // engage = Closed + L: a ringing chord swells in, silence stays closed
                self.engine.reset();
                self.gate.arm();
                self.strum.reset();
            }
            self.bypass_g.set_target(if bypassed { 0.0 } else { 1.0 });
            self.was_bypassed = bypassed;
        }
        let fire = v[p::FIRE] >= 0.5;
        let fire_edge = fire && !self.was_fire;
        self.was_fire = fire;
        let legato = (v[p::LEGATO] * 0.01).clamp(0.0, 1.0);
        let legato_on = legato > 0.0;
        if legato_on && !self.legato_on {
            self.flux.reset();
            self.pitch.reset();
        }
        self.legato_on = legato_on;
        let sens = (v[p::SENS] * 0.01).clamp(0.0, 1.0);
        let window = 18.0 + 30.0 * sens;
        let thresh = v[p::THRESH];
        let delta = Flux::delta_for(legato);
        let clarity = PitchJump::clarity_for(legato);
        let reset_db = v[p::RESET].clamp(6.0, 60.0);
        let release = (v[p::RELEASE].clamp(10.0, 2000.0) * 0.001 * sr) as f64;
        let body_after = (0.02 * sr) as u64;
        let level_lag = self.gate.lag() as f64 + 0.0002 * sr as f64;
        let pick_lag = (Pick::LAG_MS * 0.001 * sr) as f64;
        let flux_lag = (Flux::LAG_MS * 0.001 * sr) as f64;
        let ksc = key.map_or(1.0, |k| 1.0 / k.len() as f32);
        let msc = 1.0 / nch.max(1) as f32;

        for i in 0..n {
            self.clock += 1;
            self.strum.tick();
            // the detection signal: mean of the key's or the main's channels (undelayed)
            let xd = match key {
                Some(k) => k.iter().map(|c| c.get(i).copied().unwrap_or(0.0)).sum::<f32>() * ksc,
                None => (0..nch).map(|c| buffers[c][i]).sum::<f32>() * msc,
            };
            let e_db = self.level.next(xd, thresh);
            let floor = self.level.floor_db(window, thresh);
            let above = e_db > floor;
            if i == 0 && fire_edge {
                self.onset(Source::Manual, 0.0, true);
            }
            if self.gate.next(e_db, floor, self.engine.is_closed()) {
                self.onset(Source::Level, level_lag, true);
            }
            if self.pick.next(xd, above) {
                self.onset(Source::Pick, pick_lag, true);
            }
            if legato_on {
                let phrase = self.level.phrase_db();
                if self.flux.next(xd, phrase, delta) && above {
                    self.onset(Source::Flux, flux_lag, true);
                }
                if let Some(j) = self.pitch.next(xd, clarity) {
                    if above {
                        self.onset(Source::Pitch, j.lag as f64, j.cents > 0.0);
                    }
                }
            }
            // reset: the note has decayed `reset` dB under its body, or under `thresh`
            self.since_onset += 1;
            if self.since_onset > body_after {
                self.body_db = self.body_db.max(e_db);
            }
            if self.engine.is_playing() || self.engine.is_spent() {
                let decayed = self.since_onset > body_after && e_db < self.body_db - reset_db;
                if decayed || e_db < thresh {
                    self.engine.release(release);
                }
            }

            // audio
            let u = self.engine.next();
            let s0 = 1.0 - (self.depth_s.next_from(self.depth.next()) * 0.01).clamp(0.0, 1.0) as f64;
            let g = (s0 + (1.0 - s0) * u) as f32;
            let out_g = gfx_dsp::util::db_to_gain(self.out_level_s.next_from(self.out_level.next()));
            let xf = self.la_fade.next();
            let byp = self.bypass_g.next();
            for c in 0..nch {
                let d = &mut self.delay[c];
                d.push(buffers[c][i]);
                let y = if xf >= 1.0 { d.tap(self.la_cur) } else { d.tap(self.la_prev) + (d.tap(self.la_cur) - d.tap(self.la_prev)) * xf };
                buffers[c][i] = y + (y * g * out_g - y) * byp;
            }
            self.gain = g;
        }
        self.gr_db = (-20.0 * self.gain.max(1e-9).log10()).clamp(0.0, 90.0);
    }
}

/// `OnePole` fed from an upstream smoother each sample.
trait Chain {
    fn next_from(&mut self, target: f32) -> f32;
}

impl Chain for OnePole {
    #[inline]
    fn next_from(&mut self, target: f32) -> f32 {
        if target != self.target() {
            self.set_target(target);
        }
        self.next()
    }
}

impl Default for Swell {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Swell {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        self.level.prepare(sample_rate);
        self.gate.prepare(sample_rate);
        self.pick.prepare(sample_rate);
        self.pick.set(self.pick_settings());
        self.flux.prepare(sample_rate);
        self.pitch.prepare(sample_rate);
        self.strum.prepare(sample_rate);
        self.depth.prepare(sample_rate);
        self.depth_s.prepare(sample_rate);
        self.out_level.prepare(sample_rate);
        self.out_level_s.prepare(sample_rate);
        let max = (LOOKAHEAD_MS[3] * 0.001 * sample_rate).ceil() as usize + 2;
        for d in self.delay.iter_mut() {
            d.prepare(max);
        }
        self.la_fade.prepare(sample_rate);
        self.bypass_g.prepare(sample_rate);
        self.la_cur = self.la_samples(idx(self.v[p::LOOKAHEAD], 4));
        self.la_prev = self.la_cur;
    }

    fn reset(&mut self) {
        let v = self.v;
        self.level.reset();
        self.gate.reset();
        self.pick.reset();
        self.flux.reset();
        self.pitch.reset();
        self.strum.reset();
        self.legato_on = v[p::LEGATO] > 0.0;
        self.key_silent = 0;
        self.engine.reset();
        self.depth.settle(v[p::DEPTH].clamp(0.0, 100.0));
        self.depth_s.set_immediate(v[p::DEPTH].clamp(0.0, 100.0));
        self.out_level.settle(v[p::LEVEL].clamp(-24.0, 12.0));
        self.out_level_s.set_immediate(v[p::LEVEL].clamp(-24.0, 12.0));
        self.body_db = DB_FLOOR;
        self.since_onset = 0;
        for d in self.delay.iter_mut() {
            d.reset();
        }
        self.la_cur = self.la_samples(idx(v[p::LOOKAHEAD], 4));
        self.la_prev = self.la_cur;
        self.la_fade.set_immediate(1.0);
        self.was_bypassed = v[p::BYPASS] >= 0.5;
        self.bypass_g.set_immediate(if self.was_bypassed { 0.0 } else { 1.0 });
        self.was_fire = v[p::FIRE] >= 0.5;
        self.clock = 0;
        self.gain = 0.0;
        self.gr_db = 0.0;
        self.onsets = 0;
        self.detected = [0; 5];
        self.ev_n = 0;
    }

    fn set_param(&mut self, id: &str, value: f32) {
        let Some(i) = PARAMS.iter().position(|pi| pi.id == id) else { return };
        let old = self.v[i];
        self.v[i] = value;
        match i {
            p::DEPTH if value != old => self.depth.set_target(value.clamp(0.0, 100.0), self.clock),
            p::LEVEL if value != old => self.out_level.set_target(value.clamp(-24.0, 12.0), self.clock),
            p::SENS => self.pick.set(self.pick_settings()),
            p::LOOKAHEAD => {
                let la = self.la_samples(idx(value, 4));
                if la != self.la_cur {
                    // a setup change: the host re-aligns; the audio crossfades over 20 ms
                    self.la_prev = if self.la_fade.is_settled() { self.la_cur } else if self.la_fade.value() < 0.5 { self.la_prev } else { self.la_cur };
                    self.la_cur = la;
                    self.la_fade.set_immediate(0.0);
                    self.la_fade.set_target(1.0);
                }
            }
            _ => {}
        }
    }

    fn set_transport(&mut self, transport: TransportInfo) {
        self.transport = transport;
    }

    fn latency(&self) -> usize {
        self.la_cur
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_with_key(buffers, None);
    }
}
