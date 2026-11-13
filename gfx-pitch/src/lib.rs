// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-pitch — golem's pitch shifter (design: README.md §2).
//!
//! Stands for golem's `pitch` block (ids `shift` in semitones, `mix` in % kept), replacing ReaPitch,
//! which golem quarantined because it froze the engine. Two engines:
//! `shift` glides (one-pole of `glide`, at least 10 ms) and is slew-limited to 1 st / ms.
//!
//! - **Whammy** (single notes): gfx-dsp's centred splice-search shifter centred on 12 ms, splice search
//!   over half a low-E period, per-sample ratio (EXP sweeps glide).
//! - **Poly** (chords): gfx-dsp's low-latency phase vocoder (`vocoder`): peak-locked shifting on a long
//!   asymmetric analysis window (8192 at 48 kHz) and a short synthesis window: Fast 6.6 ms, Stable 10 ms.
//!   Both voices share one analysis.
//!
//! A good shifter needs latency. Each engine reports its fixed centre at initialisation or on an engine /
//! tracking change, never when `shift` moves; the dry is delayed by it, so the mix stays coherent.
//! Everything is allocated in `prepare`: nothing allocates or blocks on enable (the ReaPitch failure).

#![allow(clippy::needless_range_loop)]

use gfx_dsp::delay::DelayLine;
use gfx_dsp::detect::{harmony_interval, NoteTracker, PitchDetector, SCALES};
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::pitch::CenteredShifter;
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::vocoder::PvShifter;
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
mod plugin;

const MAX_CH: usize = 2;
pub static PARAMS: [ParamInfo; 11] = [
    ParamInfo::float("shift", "Shift", -24.0, 24.0, 12.0).with_exp(),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 50.0).with_exp(),
    ParamInfo::enumeration("engine", "Engine", 2, 0),
    ParamInfo::enumeration("tracking", "Tracking", 2, 0),
    ParamInfo::float("glide", "Glide", 0.0, 1000.0, 0.0),
    ParamInfo::float("shift2", "Shift 2", -24.0, 24.0, 0.0).with_exp(),
    ParamInfo::float("voice2", "Voice 2 Level", -60.0, 0.0, -60.0),
    ParamInfo::float("fine", "Fine", -50.0, 50.0, 0.0),
    // phase 2: intelligent harmony (appended)
    ParamInfo::enumeration("harmony", "Harmony", 2, 0),
    ParamInfo::enumeration("key", "Key", 12, 0),
    ParamInfo::enumeration("scale", "Scale", SCALES.len() as u32, 0),
];

pub mod p {
    pub const SHIFT: usize = 0;
    pub const MIX: usize = 1;
    pub const ENGINE: usize = 2;
    pub const TRACKING: usize = 3;
    pub const GLIDE: usize = 4;
    pub const SHIFT2: usize = 5;
    pub const VOICE2: usize = 6;
    pub const FINE: usize = 7;
    pub const HARMONY: usize = 8;
    pub const KEY: usize = 9;
    pub const SCALE: usize = 10;
    pub const N: usize = 11;
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

/// The Whammy's centre (ms).
pub const WHAMMY_MS: f32 = 12.0;

/// Reported latency (samples) of an engine at `sr`: the Whammy's centre, or the vocoder's synthesis
/// window (at 48 kHz: Fast 319 samples, 6.6 ms; Stable 479, 10 ms).
pub fn latency_of(engine: usize, tracking: usize, sr: f32) -> usize {
    if engine == 0 {
        (WHAMMY_MS as f64 * 0.001 * sr as f64).round() as usize
    } else {
        2 * PvShifter::modes(sr)[tracking.min(1)].1 - 1
    }
}

pub struct Pitch {
    s: Settings,
    sr: f32,
    engine: usize,
    tracking: usize,
    pending: bool,
    gate: LinearRamp,
    v1: [CenteredShifter; MAX_CH],
    v2: [CenteredShifter; MAX_CH],
    pv: [PvShifter; MAX_CH],
    dry: [DelayLine; MAX_CH],
    shift: OnePole,
    shift2: OnePole,
    fine: OnePole,
    voice2: OnePole,
    mix: ExpSmoother,
    /// the dry's delay in use: it moves to the new engine's latency at the bottom of the dip (the whole
    /// output dips when the latency changes, since the dry moves)
    lat_used: usize,
    /// after an engine change the output stays down until the new shifters' lines hold audio
    hold: usize,
    /// the shifts after the slew limit (1 st / ms: a −24 → +24 jump takes 48 ms)
    st_out: [f64; 2],
    clock: u64,
    /// harmony: the played note (detected on the undelayed input, so ~12 ms ahead of the wet)
    det: PitchDetector,
    notes: NoteTracker,
}

impl Pitch {
    pub fn new() -> Self {
        let s = Settings::default();
        let v = s.v;
        Self {
            s,
            sr: 48000.0,
            engine: 0,
            tracking: 0,
            pending: false,
            gate: LinearRamp::new(30.0, 1.0),
            v1: Default::default(),
            v2: Default::default(),
            pv: Default::default(),
            dry: Default::default(),
            shift: OnePole::new(10.0, v[p::SHIFT], 1e-5),
            shift2: OnePole::new(10.0, v[p::SHIFT2], 1e-5),
            fine: OnePole::new(20.0, v[p::FINE], 1e-4),
            voice2: OnePole::new(20.0, v[p::VOICE2], 1e-4),
            mix: ExpSmoother::new(0.0, 100.0, v[p::MIX]),
            lat_used: 0,
            hold: 0,
            st_out: [0.0; 2],
            clock: 0,
            det: PitchDetector::default(),
            notes: NoteTracker::default(),
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    /// Reported latency (samples). Follows a pending engine change.
    pub fn latency_samples(&self) -> usize {
        let (e, t) = if self.pending { (idx(self.s.v[p::ENGINE], 2), idx(self.s.v[p::TRACKING], 2)) } else { (self.engine, self.tracking) };
        latency_of(e, t, self.sr)
    }

    pub fn set(&mut self, s: Settings) {
        let v = &s.v;
        if v[p::MIX] != self.s.v[p::MIX] {
            self.mix.set_target(v[p::MIX].clamp(0.0, 100.0), self.clock);
        }
        self.shift.set_target(v[p::SHIFT]);
        self.shift2.set_target(v[p::SHIFT2]);
        self.fine.set_target(v[p::FINE]);
        self.voice2.set_target(v[p::VOICE2]);
        // glide: the time `shift` takes to reach a new value (at least 10 ms: no steps)
        let g = v[p::GLIDE].max(10.0);
        self.shift.set_time(g, self.sr);
        self.shift2.set_time(g, self.sr);
        if idx(v[p::ENGINE], 2) != self.engine || idx(v[p::TRACKING], 2) != self.tracking {
            self.pending = true;
            self.gate.set_target(0.0);
        }
        self.s = s;
    }

    fn apply_engine(&mut self) {
        self.engine = idx(self.s.v[p::ENGINE], 2);
        self.tracking = idx(self.s.v[p::TRACKING], 2);
        let sr = self.sr;
        for v in self.v1.iter_mut().chain(self.v2.iter_mut()) {
            v.configure(sr, WHAMMY_MS, 6.0, 60.0);
            v.reset();
        }
        for pv in self.pv.iter_mut() {
            pv.configure(self.tracking);
        }
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(MAX_CH);
        let n = buffers.first().map_or(0, |b| b.len());
        for i in 0..n {
            self.clock += 1;
            let g = self.gate.next();
            if g == 0.0 && self.gate.is_settled() {
                if self.pending {
                    self.pending = false;
                    self.apply_engine();
                    self.lat_used = self.latency_samples();
                    self.hold = 2 * self.lat_used;
                }
                if self.hold > 0 {
                    self.hold -= 1;
                } else {
                    self.gate.set_target(1.0);
                }
            }
            let gs = 0.5 - 0.5 * (std::f32::consts::PI * g).cos();
            let lim = 1000.0 / self.sr as f64;
            let (sh1, sh2, fine) = (self.shift.next(), self.shift2.next(), self.fine.next());
            let v = &self.s.v;
            let (t1, t2) = if v[p::HARMONY] >= 0.5 {
                // the shifts become scale steps above the played note; the slew limit below makes
                // each interval change a ~1 ms bend
                let xm = if nch > 1 { 0.5 * (buffers[0][i] + buffers[1][i]) } else { buffers[0][i] };
                if self.det.push(xm) {
                    self.notes.update(self.det.hz());
                }
                let key = idx(v[p::KEY], 12) as u8;
                let scale = idx(v[p::SCALE], SCALES.len());
                let pc = self.notes.pitch_class().unwrap_or(key);
                (
                    harmony_interval(pc, key, scale, v[p::SHIFT]) as f64 + (fine * 0.01) as f64,
                    harmony_interval(pc, key, scale, v[p::SHIFT2]) as f64,
                )
            } else {
                ((sh1 + fine * 0.01) as f64, sh2 as f64)
            };
            self.st_out[0] += (t1 - self.st_out[0]).clamp(-lim, lim);
            self.st_out[1] += (t2 - self.st_out[1]).clamp(-lim, lim);
            let (st1, st2) = (self.st_out[0], self.st_out[1]);
            let v2db = self.voice2.next();
            let v2g = if v2db <= -60.0 { 0.0 } else { 10f32.powf(v2db / 20.0) };
            let mix = self.mix.next() * 0.01 * std::f32::consts::FRAC_PI_2;
            let (dg, wg) = (mix.cos() * gs, mix.sin() * gs);
            let lat = self.lat_used;
            let poly = self.engine == 1;
            let (r1, r2) = ((st1 / 12.0).exp2() as f32, (st2 / 12.0).exp2() as f32);
            for c in 0..nch {
                let x = buffers[c][i];
                // the dry, delayed by the reported latency: the mix stays coherent
                self.dry[c].push(x);
                let d = self.dry[c].tap(lat);
                let w = if poly {
                    let pv = &mut self.pv[c];
                    pv.set_ratio(0, r1);
                    pv.set_ratio(1, r2);
                    let y = pv.process(x, if v2g > 0.0 { 2 } else { 1 });
                    y[0] + v2g * y[1]
                } else {
                    self.v1[c].set_semitones(st1);
                    let mut w = self.v1[c].process(x);
                    if v2g > 0.0 {
                        self.v2[c].set_semitones(st2);
                        w += v2g * self.v2[c].process(x);
                    }
                    w
                };
                buffers[c][i] = dg * d + wg * w;
            }
        }
    }
}

impl Default for Pitch {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Pitch {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        for o in [&mut self.shift, &mut self.shift2, &mut self.fine, &mut self.voice2] {
            o.prepare(sample_rate);
        }
        self.mix.prepare(sample_rate);
        self.gate.prepare(sample_rate);
        for d in self.dry.iter_mut() {
            d.prepare((0.03 * sample_rate as f64) as usize);
        }
        for v in self.v1.iter_mut().chain(self.v2.iter_mut()) {
            v.allocate(sample_rate, WHAMMY_MS, 6.0);
        }
        for pv in self.pv.iter_mut() {
            pv.prepare(sample_rate);
        }
        self.det.prepare(sample_rate);
        self.apply_engine();
    }

    fn reset(&mut self) {
        let v = self.s.v;
        for o in [&mut self.shift, &mut self.shift2, &mut self.fine, &mut self.voice2] {
            o.settle();
        }
        self.mix.settle(v[p::MIX].clamp(0.0, 100.0));
        self.pending = false;
        self.gate.set_immediate(1.0);
        self.apply_engine();
        for d in self.dry.iter_mut() {
            d.reset();
        }
        self.lat_used = self.latency_samples();
        self.st_out = [(v[p::SHIFT] + v[p::FINE] * 0.01) as f64, v[p::SHIFT2] as f64];
        self.clock = 0;
        self.det.reset();
        self.notes.reset();
    }

    fn set_param(&mut self, id: &str, value: f32) {
        if let Some(i) = PARAMS.iter().position(|pi| pi.id == id) {
            let s = self.s.with(i, value);
            self.set(s);
        }
    }

    fn latency(&self) -> usize {
        self.latency_samples()
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_block(buffers);
    }
}

#[cfg(test)]
mod tests;
