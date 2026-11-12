// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-pitch — golem's pitch shifter (design: README.md §2).
//!
//! Stands for golem's `pitch` block (ids `shift` in semitones, `mix` in % kept), replacing ReaPitch,
//! which golem quarantined because it froze the engine. Two engines on gfx-dsp's centred splice-search
//! shifter:
//! `shift` glides (one-pole of `glide`, at least 10 ms) and is slew-limited to 1 st / ms.
//!
//! - **Whammy** (single notes): one shifter centred on 12 ms, splice search over half a low-E period,
//!   per-sample ratio (EXP sweeps glide).
//! - **Poly** (chords): nine Linkwitz-Riley bands (half-octave where chords sit), each its own shifter
//!   with grains and search sized for the band. All bands share one centre delay: different delays would comb where the bands overlap.
//!
//! A good shifter needs latency. Each engine reports its fixed centre at initialisation or on an engine /
//! tracking change, never when `shift` moves; the dry is delayed by it, so the mix stays coherent.
//! Everything is allocated in `prepare`: nothing allocates or blocks on enable (the ReaPitch failure).

#![allow(clippy::needless_range_loop)]

use gfx_dsp::delay::DelayLine;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::pitch::CenteredShifter;
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
mod plugin;

const MAX_CH: usize = 2;
/// Poly crossovers (Hz): half-octave bands where chords sit, so each note of a low chord gets its own
/// shifter (two notes in one band defeat the splice search).
const XO: [f32; 8] = [130.0, 185.0, 260.0, 370.0, 520.0, 800.0, 1500.0, 3000.0];
const NB: usize = XO.len() + 1;

/// A band's (search ms, longest grain ms) at Fast, from its lowest frequency: the search covers half a
/// period and a little more; grains hold several periods. Stable lengthens both by 1.5.
fn band_shape(k: usize) -> (f32, f32) {
    let f = if k == 0 { 82.0 } else { XO[k - 1] };
    let period_ms = 1000.0 / f;
    ((0.6 * period_ms).clamp(0.5, 6.5), (6.0 * period_ms).clamp(8.0, 80.0))
}

pub static PARAMS: [ParamInfo; 8] = [
    ParamInfo::float("shift", "Shift", -24.0, 24.0, 12.0).with_exp(),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 50.0).with_exp(),
    ParamInfo::enumeration("engine", "Engine", 2, 0),
    ParamInfo::enumeration("tracking", "Tracking", 2, 0),
    ParamInfo::float("glide", "Glide", 0.0, 1000.0, 0.0),
    ParamInfo::float("shift2", "Shift 2", -24.0, 24.0, 0.0).with_exp(),
    ParamInfo::float("voice2", "Voice 2 Level", -60.0, 0.0, -60.0),
    ParamInfo::float("fine", "Fine", -50.0, 50.0, 0.0),
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
    pub const N: usize = 8;
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

/// The centre (ms) of an engine: Whammy 12; Poly Fast 14, Stable 22.
pub fn centre_ms(engine: usize, tracking: usize) -> f32 {
    match (engine, tracking) {
        (0, _) => 12.0,
        (_, 0) => 14.0,
        _ => 22.0,
    }
}

/// One voice: the whammy shifter, or four band shifters.
#[derive(Clone, Default)]
struct Voice {
    whammy: CenteredShifter,
    bands: [CenteredShifter; NB],
    lo: [[Svf; 2]; NB - 1],
    hi: [[Svf; 2]; NB - 1],
}

impl Voice {
    /// Allocates for the largest engine (prepare only).
    fn allocate(&mut self, sr: f32) {
        self.whammy.allocate(sr, 12.0, 6.0);
        for b in self.bands.iter_mut() {
            b.allocate(sr, 22.0, 10.0);
        }
    }

    /// Shapes the shifters for an engine (no allocation: safe on the audio thread).
    fn configure(&mut self, sr: f32, engine: usize, tracking: usize) {
        let c = centre_ms(engine, tracking);
        self.whammy.configure(sr, 12.0, 6.0, 60.0);
        let k = if tracking == 1 { 1.5 } else { 1.0 };
        for (j, b) in self.bands.iter_mut().enumerate() {
            let (search, life) = band_shape(j);
            b.configure(sr, c, (search * k).min(0.4 * c), life * k);
        }
        for (j, f) in XO.iter().enumerate() {
            for s in self.lo[j].iter_mut() {
                s.set(SvfMode::LowPass, *f, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            }
            for s in self.hi[j].iter_mut() {
                s.set(SvfMode::HighPass, *f, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            }
        }
    }

    fn reset(&mut self) {
        self.whammy.reset();
        for b in self.bands.iter_mut() {
            b.reset();
        }
        for s in self.lo.iter_mut().chain(self.hi.iter_mut()) {
            for f in s.iter_mut() {
                f.reset();
            }
        }
    }

    #[inline]
    fn process(&mut self, x: f32, st: f64, poly: bool) -> f32 {
        if !poly {
            self.whammy.set_semitones(st);
            return self.whammy.process(x);
        }
        // Linkwitz-Riley 4 tree: low | (rest → lowmid | (rest → mid | high))
        let mut rest = x;
        let mut y = 0.0;
        for j in 0..NB - 1 {
            let a = self.lo[j][0].process(rest);
            let low = self.lo[j][1].process(a);
            let h = self.hi[j][0].process(rest);
            rest = self.hi[j][1].process(h);
            self.bands[j].set_semitones(st);
            y += self.bands[j].process(low);
        }
        self.bands[NB - 1].set_semitones(st);
        y + self.bands[NB - 1].process(rest)
    }
}

pub struct Pitch {
    s: Settings,
    sr: f32,
    engine: usize,
    tracking: usize,
    pending: bool,
    gate: LinearRamp,
    v1: [Voice; MAX_CH],
    v2: [Voice; MAX_CH],
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
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    /// Reported latency (samples): the engine's fixed centre. Follows a pending engine change.
    pub fn latency_samples(&self) -> usize {
        let (e, t) = if self.pending { (idx(self.s.v[p::ENGINE], 2), idx(self.s.v[p::TRACKING], 2)) } else { (self.engine, self.tracking) };
        (centre_ms(e, t) as f64 * 0.001 * self.sr as f64).round() as usize
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
        let (sr, e, t) = (self.sr, self.engine, self.tracking);
        for v in self.v1.iter_mut().chain(self.v2.iter_mut()) {
            v.configure(sr, e, t);
            v.reset();
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
            let t1 = (self.shift.next() + self.fine.next() * 0.01) as f64;
            let t2 = self.shift2.next() as f64;
            self.st_out[0] += (t1 - self.st_out[0]).clamp(-lim, lim);
            self.st_out[1] += (t2 - self.st_out[1]).clamp(-lim, lim);
            let (st1, st2) = (self.st_out[0], self.st_out[1]);
            let v2db = self.voice2.next();
            let v2g = if v2db <= -60.0 { 0.0 } else { 10f32.powf(v2db / 20.0) };
            let mix = self.mix.next() * 0.01 * std::f32::consts::FRAC_PI_2;
            let (dg, wg) = (mix.cos() * gs, mix.sin() * gs);
            let lat = self.lat_used;
            let poly = self.engine == 1;
            for c in 0..nch {
                let x = buffers[c][i];
                // the dry, delayed by the reported latency: the mix stays coherent
                self.dry[c].push(x);
                let d = self.dry[c].tap(lat);
                let mut w = self.v1[c].process(x, st1, poly);
                if v2g > 0.0 {
                    w += v2g * self.v2[c].process(x, st2, poly);
                }
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
            v.allocate(sample_rate);
        }
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
