// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-pitch-classic — golem's classic delay-line shifter (design: gfx-pitch/README.md §3).
//!
//! Stands for golem's `pitch-shift-js` block (REAPER's JS superpitch). It keeps the ids `shift`,
//! `window`, `overlap`, `wet` and `dry` (dB), used literally:
//! - a read head sweeps the line at the ratio;
//! - every `window` a new head starts at the other end;
//! - the two cross over `overlap`, linearly, as in the JS.
//!
//! `search` adds a WSOLA-style splice search (the H3000's "deglitch"): each new head starts where its
//! waveform best continues the old one, which removes most of the warble.
//!
//! Reported latency 0, as the JS. The mean delay (about window / 2) is inherent and documented.

#![allow(clippy::needless_range_loop)]

use gfx_dsp::delay::{DelayLine, Interp};
use gfx_dsp::smooth::OnePole;
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
mod plugin;

const MAX_CH: usize = 2;

pub static PARAMS: [ParamInfo; 6] = [
    ParamInfo::float("shift", "Shift", -24.0, 24.0, 0.0).with_exp(),
    ParamInfo::float("window", "Window", 10.0, 100.0, 50.0),
    ParamInfo::float("overlap", "Overlap", 1.0, 50.0, 20.0),
    ParamInfo::float("wet", "Wet", -120.0, 6.0, 0.0),
    ParamInfo::float("dry", "Dry", -120.0, 6.0, -120.0),
    ParamInfo::boolean("search", "Splice Search", false),
];

pub mod p {
    pub const SHIFT: usize = 0;
    pub const WINDOW: usize = 1;
    pub const OVERLAP: usize = 2;
    pub const WET: usize = 3;
    pub const DRY: usize = 4;
    pub const SEARCH: usize = 5;
    pub const N: usize = 6;
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
fn db(v: f32) -> f32 {
    if v <= -120.0 {
        0.0
    } else {
        10f32.powf(v / 20.0)
    }
}

/// The two-head "rotating tape".
#[derive(Clone, Default)]
struct Tape {
    line: DelayLine,
    /// active head and the one fading out
    d: [f64; 2],
    age: f64,
    fading: bool,
    fade_age: f64,
}

impl Tape {
    fn reset(&mut self, d0: f64) {
        self.line.reset();
        self.d = [d0, d0];
        self.age = 0.0;
        self.fading = false;
        self.fade_age = 0.0;
    }

    fn corr(&self, a: usize, b: usize, n: usize) -> f64 {
        let (mut c, mut e) = (0.0f64, 1e-12f64);
        let mut k = 0;
        while k < n {
            let x = self.line.tap(a + k) as f64;
            c += x * self.line.tap(b + k) as f64;
            e += x * x;
            k += 2;
        }
        c / e.sqrt()
    }

    /// One sample. `w`, `o` window and overlap (samples); `search` range (samples, 0 = off).
    #[inline]
    fn process(&mut self, x: f32, ratio: f64, w: f64, o: f64, search: usize) -> f32 {
        self.line.push(x);
        let travel = w * (ratio - 1.0);
        let start = o + travel.max(0.0) + search as f64;
        // a ratio change can push the head out of the line: splice early enough that the head fading out
        // still has room for the whole overlap (it keeps moving by 1 − ratio per sample)
        let max = self.line.max_delay() as f64 - 4.0;
        let need = o * (ratio - 1.0).abs();
        let out = self.d[0] < 2.0 + need || self.d[0] > max - need;
        if (self.age >= w - o || out) && !self.fading {
            // splice: the old head fades out over `overlap`, a new one starts at the other end
            self.d[1] = self.d[0];
            let mut ns = start;
            if search > 0 {
                let other = self.d[1].round().max(2.0) as usize;
                let n = (o as usize).clamp(32, 480);
                let lo = (start - search as f64).max(2.0) as usize;
                let hi = start as usize + search;
                let (mut best, mut bv) = (start, f64::MIN);
                for d in lo..=hi {
                    let v = self.corr(d, other, n);
                    if v > bv {
                        bv = v;
                        best = d as f64 + (self.d[1] - other as f64);
                    }
                }
                ns = best;
            }
            self.d[0] = ns;
            self.fading = true;
            self.fade_age = 0.0;
            self.age = 0.0;
        }
        let a = self.line.read(self.d[0].clamp(2.0, max), Interp::Lagrange3);
        let y = if self.fading {
            let t = (self.fade_age / o.max(1.0)).min(1.0) as f32;
            let b = self.line.read(self.d[1].clamp(2.0, max), Interp::Lagrange3);
            self.d[1] += 1.0 - ratio;
            self.fade_age += 1.0;
            if self.fade_age >= o {
                self.fading = false;
            }
            t * a + (1.0 - t) * b
        } else {
            a
        };
        self.d[0] += 1.0 - ratio;
        self.age += 1.0;
        y
    }
}

pub struct PitchClassic {
    s: Settings,
    sr: f32,
    tape: [Tape; MAX_CH],
    shift: OnePole,
    /// the shift after a 0.25 st / ms slew limit
    st: f64,
    wet: OnePole,
    dry: OnePole,
}

impl PitchClassic {
    pub fn new() -> Self {
        let s = Settings::default();
        let v = s.v;
        Self { s, sr: 48000.0, tape: Default::default(), shift: OnePole::new(10.0, v[p::SHIFT], 1e-5), st: v[p::SHIFT] as f64, wet: OnePole::new(20.0, db(v[p::WET]), 1e-6), dry: OnePole::new(20.0, db(v[p::DRY]), 1e-6) }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn set(&mut self, s: Settings) {
        self.shift.set_target(s.v[p::SHIFT]);
        self.wet.set_target(db(s.v[p::WET]));
        self.dry.set_target(db(s.v[p::DRY]));
        self.s = s;
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(MAX_CH);
        let n = buffers.first().map_or(0, |b| b.len());
        let ms = self.sr as f64 * 0.001;
        let w = self.s.v[p::WINDOW].clamp(10.0, 100.0) as f64 * ms;
        let o = (self.s.v[p::OVERLAP].clamp(1.0, 50.0) as f64 * ms).min(0.5 * w);
        let search = if self.s.v[p::SEARCH] >= 0.5 { (2.0 * ms) as usize } else { 0 };
        for i in 0..n {
            let t = self.shift.next() as f64;
            // 0.25 st / ms: a heel-to-toe sweep (0 → +12 in 300 ms is 0.04 st / ms) is never limited, a
            // −24 → +24 jump takes 192 ms instead of forcing a splice storm
            let lim = 250.0 / self.sr as f64;
            self.st += (t - self.st).clamp(-lim, lim);
            let ratio = (self.st / 12.0).exp2();
            let (wg, dg) = (self.wet.next(), self.dry.next());
            for c in 0..nch {
                let x = buffers[c][i];
                let y = self.tape[c].process(x, ratio, w, o, search);
                buffers[c][i] = dg * x + wg * y;
            }
        }
    }
}

impl Default for PitchClassic {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for PitchClassic {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        for o in [&mut self.shift, &mut self.wet, &mut self.dry] {
            o.prepare(sample_rate);
        }
        // the longest travel: a 100 ms window at +24 st (ratio 4), plus overlap and search
        for t in self.tape.iter_mut() {
            t.line.prepare((0.5 * sample_rate as f64) as usize);
        }
    }

    fn reset(&mut self) {
        for o in [&mut self.shift, &mut self.wet, &mut self.dry] {
            o.settle();
        }
        self.st = self.s.v[p::SHIFT] as f64;
        let d0 = self.s.v[p::OVERLAP] as f64 * self.sr as f64 * 0.001;
        for t in self.tape.iter_mut() {
            t.reset(d0);
        }
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
mod tests {
    use super::*;

    fn freq_of(y: &[f32]) -> f64 {
        let mut t = Vec::new();
        for i in 1..y.len() {
            if y[i - 1] < 0.0 && y[i] >= 0.0 {
                t.push((i - 1) as f64 + (-y[i - 1] / (y[i] - y[i - 1])) as f64);
            }
        }
        (t.len() - 1) as f64 / (t[t.len() - 1] - t[0]) * 48000.0
    }

    #[test]
    fn shifts_on_average_and_search_tightens_it() {
        for search in [0.0, 1.0] {
            let mut q = PitchClassic::new();
            q.prepare(48000.0, 4800, 1);
            q.set(Settings::default().with(p::SHIFT, 7.0).with(p::SEARCH, search));
            q.reset();
            let mut y: Vec<f32> = (0..96000).map(|i| 0.3 * (std::f32::consts::TAU * 220.0 * i as f32 / 48000.0).sin()).collect();
            for c in y.chunks_mut(256) {
                q.process(&mut [c]);
            }
            let cents = 1200.0 * (freq_of(&y[12000..]) / 220.0).log2();
            // without search the crossfades between heads of unrelated phase bias a zero-crossing count (the
            // JS's warble); each grain still plays at the ratio
            let tol = if search > 0.0 { 3.0 } else { 60.0 };
            assert!((cents - 700.0).abs() < tol, "search {search}: {cents:.1} cents");
        }
    }

    #[test]
    fn dry_and_wet_are_independent_db() {
        let mut q = PitchClassic::new();
        q.prepare(48000.0, 4800, 1);
        q.set(Settings::default().with(p::SHIFT, 0.0).with(p::WET, -120.0).with(p::DRY, -6.0));
        q.reset();
        let mut y = vec![0.5f32; 4800];
        q.process(&mut [&mut y[..]]);
        assert!((y[4000] - 0.5 * 10f32.powf(-6.0 / 20.0)).abs() < 1e-3, "{}", y[4000]);
    }
}
