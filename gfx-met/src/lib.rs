// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-met — golem's keyed expander-gate for high gain (design: README.md).
//!
//! A true mean-square detector on the key (the lane's DI), read in dB, drives a hard-knee downward
//! expander (1 : 2.8 by default, the Zuul's slope). The detector's time constant switches from slow to
//! fast when the level falls more than 6.5 dB below a leaky reference: the analog circuit's
//! slow-then-plunge timing. Hold and Release (Zuul+) shape the fall of the gain.

use gfx_dsp::smooth::OnePole;
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
mod plugin;

pub static PARAMS: [ParamInfo; 11] = [
    ParamInfo::float("thresh", "Threshold", -90.0, 0.0, -45.0),
    ParamInfo::float("hold", "Hold", 0.0, 500.0, 0.0),
    ParamInfo::float("release", "Release", 0.0, 1000.0, 0.0),
    ParamInfo::float("ratio", "Ratio", 1.5, 10.0, 2.8),
    ParamInfo::float("range", "Range", -90.0, 0.0, -90.0),
    ParamInfo::float("smooth", "Smoothing", 10.0, 200.0, 58.0),
    ParamInfo::boolean("adaptive", "Adaptive", true),
    ParamInfo::float("hpf", "Key Low Cut", 0.0, 500.0, 0.0),
    ParamInfo::float("lpf", "Key High Cut", 500.0, 22000.0, 22000.0),
    ParamInfo::enumeration("source", "Detector Source", 2, 1),
    ParamInfo::boolean("listen", "Listen", false),
];

/// The analog circuit's constants (README §1): fast τ = slow τ / 13, clamp at 6.5 dB. The reference
/// leaks in 1 s: with it, a fall runs its first 6.5 dB at the slow rate (≈ 85 ms) before it plunges, the
/// design review's reading of the circuit (README §2).
pub const FAST_RATIO: f64 = 13.0;
pub const CLAMP_DB: f64 = 6.5;
pub const REF_MS: f64 = 1000.0;
/// The fixed key coupling high-pass (≈ 32 Hz).
pub const KEY_HPF_HZ: f64 = 32.0;
/// Fall floor with Hold and no Release: the step after the hold never clicks.
pub const MIN_FALL_MS: f64 = 5.0;
/// Gain smoother: an attack (a rise of more than `ATTACK_DB`) opens through two cascaded one-poles of
/// `RISE_MS` in linear gain (an S-shaped onset, within 1 dB after ≈ 1 ms, no click); a smaller rise —
/// the mean-square ripple of a low note — is smoothed over `RIPPLE_MS`; falls take `FALL_MS`, in dB.
/// Without it the ripple (±0.9 dB on a low E in fast mode) becomes gain wobble at the slope.
pub const RISE_MS: f64 = 0.25;
pub const LISTEN_MS: f32 = 5.0;
pub const RIPPLE_MS: f64 = 40.0;
pub const ATTACK_DB: f64 = 3.0;
pub const FALL_MS: f64 = 1.5;
const CTL: u32 = 32;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub thresh: f32,
    pub hold: f32,
    pub release: f32,
    pub ratio: f32,
    pub range: f32,
    pub smooth: f32,
    pub adaptive: bool,
    pub hpf: f32,
    pub lpf: f32,
    pub source_key: bool,
    pub listen: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            thresh: -45.0,
            hold: 0.0,
            release: 0.0,
            ratio: 2.8,
            range: -90.0,
            smooth: 58.0,
            adaptive: true,
            hpf: 0.0,
            lpf: 22000.0,
            source_key: true,
            listen: false,
        }
    }
}

pub struct Met {
    s: Settings,
    sr: f64,
    // smoothed controls
    thresh: OnePole,
    ratio: OnePole,
    range: OnePole,
    hpf: OnePole,
    lpf: OnePole,
    // key path
    dc_x: f64,
    dc_y: f64,
    dc_a: f64,
    hp: Svf,
    lp: Svf,
    hp_on: bool,
    lp_on: bool,
    // detector
    ms: f64,
    reference: f64,
    fast: bool,
    // gain
    g_db: f64,  // after hold / release
    out_db: f64, // after the smoother
    lin1: f64,  // attack smoother stages (linear gain)
    lin2: f64,
    listen: gfx_dsp::smooth::LinearRamp,
    below: u64, // samples since the level went under the threshold
    key_seen: bool,
    clock: u64,
}

impl Met {
    pub fn new() -> Self {
        let s = Settings::default();
        Self {
            s,
            sr: 48000.0,
            thresh: OnePole::new(20.0, s.thresh, 1e-4),
            ratio: OnePole::new(20.0, s.ratio, 1e-5),
            range: OnePole::new(20.0, s.range, 1e-4),
            hpf: OnePole::new(20.0, s.hpf, 1e-3),
            lpf: OnePole::new(20.0, s.lpf, 1e-2),
            dc_x: 0.0,
            dc_y: 0.0,
            dc_a: 0.0,
            hp: Svf::new(),
            lp: Svf::new(),
            hp_on: false,
            lp_on: false,
            ms: 0.0,
            reference: -200.0,
            fast: false,
            g_db: 0.0,
            out_db: 0.0,
            lin1: 1.0,
            lin2: 1.0,
            listen: gfx_dsp::smooth::LinearRamp::new(LISTEN_MS, 0.0),
            below: 0,
            key_seen: false,
            clock: 0,
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn set(&mut self, s: Settings) {
        self.s = s;
        self.thresh.set_target(s.thresh.clamp(-90.0, 0.0));
        self.ratio.set_target(s.ratio.clamp(1.5, 10.0));
        self.range.set_target(s.range.clamp(-90.0, 0.0));
        self.hpf.set_target(s.hpf.clamp(0.0, 500.0));
        self.listen.set_target(if s.listen { 1.0 } else { 0.0 });
        self.lpf.set_target(s.lpf.clamp(500.0, 22000.0));
    }

    /// Current attenuation in dB (≤ 0).
    pub fn gain_reduction_db(&self) -> f32 {
        self.out_db.min(0.0) as f32
    }

    pub fn is_open(&self) -> bool {
        self.out_db > -1.0
    }

    /// The detector's level in dB (RMS of the filtered key).
    pub fn level_db(&self) -> f64 {
        10.0 * (self.ms + 1e-20).log10()
    }

    fn update_filters(&mut self) {
        let sr = self.sr as f32;
        let h = self.hpf.value();
        self.hp_on = h > 1.0;
        if self.hp_on {
            self.hp.set(SvfMode::HighPass, h, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
        }
        let l = self.lpf.value();
        self.lp_on = l < 21999.0;
        if self.lp_on {
            self.lp.set(SvfMode::LowPass, l.min(0.45 * sr), std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
        }
    }

    /// The filtered key sample.
    #[inline]
    fn key_filter(&mut self, x: f64) -> f64 {
        // fixed first-order coupling high-pass
        let y = self.dc_a * (self.dc_y + x - self.dc_x);
        self.dc_x = x;
        self.dc_y = y;
        let mut v = y as f32;
        if self.hp_on {
            v = self.hp.process(v);
        }
        if self.lp_on {
            v = self.lp.process(v);
        }
        v as f64
    }

    /// One detector + gain step; returns the gain in dB after the smoother.
    #[inline]
    fn step(&mut self, k: f64) -> f64 {
        let slow = (self.s.smooth.clamp(10.0, 200.0) as f64) * 0.001 * self.sr;
        let tau = if self.s.adaptive && self.fast { slow / FAST_RATIO } else { slow };
        let a = 1.0 - (-1.0 / tau).exp();
        self.ms += a * (k * k - self.ms);
        let level = 10.0 * (self.ms + 1e-20).log10();
        // the leaky reference and its clamp
        let ar = 1.0 - (-1.0 / (REF_MS * 0.001 * self.sr)).exp();
        self.reference += ar * (level - self.reference);
        if self.reference < level {
            self.reference = level; // the reference never sits under a rising level
        }
        self.fast = level < self.reference - CLAMP_DB;

        let t = self.thresh.next() as f64;
        let ratio = self.ratio.next() as f64;
        let range = self.range.next() as f64;
        let target = if level >= t { 0.0 } else { ((level - t) * (ratio - 1.0)).max(range) };
        if level >= t {
            self.below = 0;
        } else {
            self.below += 1;
        }

        if target >= self.g_db {
            self.g_db = target;
        } else {
            let hold = (self.s.hold.max(0.0) as f64) * 0.001 * self.sr;
            if (self.below as f64) > hold {
                let rel_ms = if self.s.release > 0.0 {
                    self.s.release as f64
                } else if self.s.hold > 0.0 {
                    MIN_FALL_MS
                } else {
                    0.0
                };
                if rel_ms <= 0.0 || range >= 0.0 {
                    self.g_db = target;
                } else {
                    let step = (-range) / (rel_ms * 0.001 * self.sr);
                    self.g_db = (self.g_db - step).max(target);
                }
            }
        }
        let coef = |ms: f64, sr: f64| 1.0 - (-1.0 / (ms * 0.001 * sr)).exp();
        let up = self.g_db - self.out_db;
        if up > ATTACK_DB || (up > 0.0 && self.lin2 < self.lin1) {
            // an attack: two linear-gain one-poles, an S-shaped onset
            let target = 10f64.powf(self.g_db / 20.0);
            let c = coef(RISE_MS, self.sr);
            self.lin1 += c * (target - self.lin1);
            self.lin2 += c * (self.lin1 - self.lin2);
            self.out_db = 20.0 * self.lin2.max(1e-10).log10();
        } else {
            let ms = if up > 0.0 { RIPPLE_MS } else { FALL_MS };
            self.out_db += coef(ms, self.sr) * (self.g_db - self.out_db);
            let l = 10f64.powf(self.out_db / 20.0);
            self.lin1 = l;
            self.lin2 = l;
        }
        self.out_db
    }

    /// Processes with an optional key (sidechain) input. Before any non-zero key sample has been seen,
    /// the detector reads the main input; afterwards it stays on the key (a muted DI keeps the gate
    /// closed rather than opening on the amp's hiss).
    pub fn process_with_key(&mut self, main: &mut [&mut [f32]], key: Option<&[&[f32]]>) {
        let n = main.first().map_or(0, |b| b.len());
        let mut use_key = false;
        if self.s.source_key {
            if let Some(k) = key {
                if !self.key_seen && k.iter().any(|c| c.iter().any(|&v| v != 0.0)) {
                    self.key_seen = true;
                }
                use_key = self.key_seen;
            }
        }
        for i in 0..n {
            if self.clock % CTL as u64 == 0 {
                for _ in 0..CTL {
                    self.hpf.next();
                    self.lpf.next();
                }
                self.update_filters();
            }
            self.clock += 1;
            // the detector's input: the loudest channel of the source
            let mut x = 0.0f64;
            if use_key {
                if let Some(k) = key {
                    for c in k.iter() {
                        let v = c[i] as f64;
                        if v.abs() > x.abs() {
                            x = v;
                        }
                    }
                }
            } else {
                for c in main.iter() {
                    let v = c[i] as f64;
                    if v.abs() > x.abs() {
                        x = v;
                    }
                }
            }
            let kf = self.key_filter(x);
            let g_db = self.step(kf);
            let g = if g_db <= -90.0 { 0.0 } else { 10f64.powf(g_db / 20.0) as f32 };
            // Listen crossfades between the gated main and the filtered key (no click)
            let l = self.listen.next();
            for c in main.iter_mut() {
                c[i] = (1.0 - l) * c[i] * g + l * kf as f32;
            }
        }
    }
}

impl Default for Met {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Met {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate as f64;
        for p in [&mut self.thresh, &mut self.ratio, &mut self.range, &mut self.hpf, &mut self.lpf] {
            p.prepare(sample_rate);
        }
        self.listen.prepare(sample_rate);
        let rc = 1.0 / (2.0 * std::f64::consts::PI * KEY_HPF_HZ);
        self.dc_a = rc / (rc + 1.0 / self.sr);
        self.update_filters();
    }

    fn reset(&mut self) {
        for p in [&mut self.thresh, &mut self.ratio, &mut self.range, &mut self.hpf, &mut self.lpf] {
            p.settle();
        }
        self.hp.reset();
        self.lp.reset();
        self.update_filters();
        self.dc_x = 0.0;
        self.dc_y = 0.0;
        self.ms = 0.0;
        self.reference = -200.0;
        self.fast = false;
        // starts closed: silence before the first note
        let floor = self.s.range.clamp(-90.0, 0.0) as f64;
        self.g_db = floor;
        self.out_db = floor;
        self.lin1 = 10f64.powf(floor / 20.0);
        self.lin2 = self.lin1;
        self.listen.settle();
        self.below = 0;
        self.key_seen = false;
        self.clock = 0;
    }

    fn set_param(&mut self, id: &str, value: f32) {
        let mut s = self.s;
        match id {
            "thresh" => s.thresh = value,
            "hold" => s.hold = value,
            "release" => s.release = value,
            "ratio" => s.ratio = value,
            "range" => s.range = value,
            "smooth" => s.smooth = value,
            "adaptive" => s.adaptive = value >= 0.5,
            "hpf" => s.hpf = value,
            "lpf" => s.lpf = value,
            "source" => s.source_key = value >= 0.5,
            "listen" => s.listen = value >= 0.5,
            _ => return,
        }
        self.set(s);
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_with_key(buffers, None);
    }

    /// The key bus (gfx-bench drives it through the trait): the inherent `process_with_key`.
    fn process_with_key(&mut self, buffers: &mut [&mut [f32]], key: Option<&[&[f32]]>) {
        Met::process_with_key(self, buffers, key);
    }
}

#[cfg(test)]
mod tests;
