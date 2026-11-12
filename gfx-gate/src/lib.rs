// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-gate — golem's noise gate (design: README.md).
//!
//! Stands for golem's `gate` block (ids `thresh`, `attack`, `hold`, `release`, `hpf`, `lpf`, `hyst` kept).
//! What makes it tight without chattering:
//! - it **opens** on the instantaneous peak of the filtered sidechain (as fast as the pick) and
//!   **closes** on a sliding maximum over one period of the lowest content (ripple-free on a low B);
//! - attack is a raised-cosine S-curve in gain, release linear in dB, both resuming from the current
//!   gain (nothing ever jumps);
//! - optional expander curve (ratio, knee), range, follow-decay, tail damp, hum notch, key input.
//!
//! Zero latency. Control-rate work (filter coefficients, thresholds) runs on a fixed 32-sample grid
//! counted from reset, so the output does not depend on the host's block size.

use gfx_dsp::envelope::SlidingMax;
use gfx_dsp::smooth::OnePole;
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::util::db_to_gain;
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
mod plugin;

const MAX_CH: usize = 2;
const CTL: u64 = 32;
const Q_BW: f32 = std::f32::consts::FRAC_1_SQRT_2;

pub static PARAMS: [ParamInfo; 15] = [
    ParamInfo::float("thresh", "Threshold", -90.0, 0.0, -50.0),
    ParamInfo::float("attack", "Attack", 0.0, 100.0, 3.0),
    ParamInfo::float("hold", "Hold", 0.0, 1000.0, 20.0),
    ParamInfo::float("release", "Release", 5.0, 2000.0, 150.0),
    ParamInfo::float("hpf", "Sidechain Low Cut", 0.0, 2000.0, 0.0),
    ParamInfo::float("lpf", "Sidechain High Cut", 500.0, 22000.0, 20000.0),
    ParamInfo::float("hyst", "Hysteresis", -24.0, 0.0, 0.0),
    ParamInfo::float("range", "Range", -90.0, 0.0, -90.0),
    ParamInfo::float("ratio", "Ratio", 1.0, 100.0, 100.0),
    ParamInfo::float("knee", "Knee", 0.0, 24.0, 0.0),
    ParamInfo::boolean("follow", "Follow Decay", false),
    ParamInfo::float("damp", "Tail Damp", 0.0, 100.0, 0.0),
    ParamInfo::enumeration("hum", "Hum Filter", 3, 0),
    ParamInfo::enumeration("source", "Detector Source", 2, 0),
    ParamInfo::boolean("listen", "Listen", false),
];

/// Parameter values as plain numbers (the wrapper and the bench write these).
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub thresh: f32,
    pub attack: f32,
    pub hold: f32,
    pub release: f32,
    pub hpf: f32,
    pub lpf: f32,
    pub hyst: f32,
    pub range: f32,
    pub ratio: f32,
    pub knee: f32,
    pub follow: bool,
    pub damp: f32,
    /// 0 off, 1 = 50 Hz, 2 = 60 Hz
    pub hum: u32,
    /// 0 main, 1 key
    pub source: u32,
    pub listen: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            thresh: -50.0,
            attack: 3.0,
            hold: 20.0,
            release: 150.0,
            hpf: 0.0,
            lpf: 20000.0,
            hyst: 0.0,
            range: -90.0,
            ratio: 100.0,
            knee: 0.0,
            follow: false,
            damp: 0.0,
            hum: 0,
            source: 0,
            listen: false,
        }
    }
}

/// Per-channel sidechain filters.
#[derive(Clone, Default)]
struct Chain {
    hum: [Svf; 4],
    hp: Svf,
    lp: Svf,
    damp_lp: f64,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    Idle,
    Attack,
    Release,
}

pub struct Gate {
    s: Settings,
    sr: f32,
    chains: [Chain; MAX_CH],
    // smoothed control values
    thresh: OnePole,
    hyst: OnePole,
    range: OnePole,
    ratio: OnePole,
    knee: OnePole,
    damp: OnePole,
    hpf: OnePole,
    lpf: OnePole,
    // control-rate state
    open_lin: f32,
    close_lin: f32,
    hum_on: bool,
    hp_on: bool,
    lp_on: bool,
    damp_db: f32,
    // detector
    close_max: SlidingMax,
    opened: bool,
    hold_left: u32,
    // ballistics (linear gain)
    gain: f32,
    phase: Phase,
    att_from: f32,
    att_to: f32,
    att_pos: u32,
    att_len: u32,
    rel_mult: f32,
    // follow decay: close-detector level every ms over the last 50 ms
    hist: [f32; 51],
    hist_pos: usize,
    hist_tick: u32,
    // key fallback
    key_silent: u64,
    clock: u64,
}

impl Gate {
    pub fn new() -> Self {
        let d = Settings::default();
        Self {
            s: d,
            sr: 48000.0,
            chains: Default::default(),
            thresh: OnePole::new(20.0, d.thresh, 1e-4),
            hyst: OnePole::new(20.0, d.hyst, 1e-4),
            range: OnePole::new(20.0, d.range, 1e-4),
            ratio: OnePole::new(20.0, d.ratio, 1e-4),
            knee: OnePole::new(20.0, d.knee, 1e-4),
            damp: OnePole::new(20.0, d.damp, 1e-4),
            hpf: OnePole::new(20.0, d.hpf, 1e-3),
            lpf: OnePole::new(20.0, d.lpf, 1e-2),
            open_lin: 0.0,
            close_lin: 0.0,
            hum_on: false,
            hp_on: false,
            lp_on: false,
            damp_db: 0.0,
            close_max: SlidingMax::new(),
            opened: false,
            hold_left: 0,
            gain: 0.0,
            phase: Phase::Idle,
            att_from: 0.0,
            att_to: 0.0,
            att_pos: 0,
            att_len: 1,
            rel_mult: 1.0,
            hist: [-200.0; 51],
            hist_pos: 0,
            hist_tick: 0,
            key_silent: 0,
            clock: 0,
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn set(&mut self, s: Settings) {
        self.s = s;
        self.thresh.set_target(s.thresh);
        self.hyst.set_target(s.hyst.min(0.0));
        self.range.set_target(s.range.min(0.0));
        self.ratio.set_target(s.ratio.max(1.0));
        self.knee.set_target(s.knee.max(0.0));
        self.damp.set_target(s.damp.clamp(0.0, 100.0));
        self.hpf.set_target(s.hpf.max(0.0));
        self.lpf.set_target(s.lpf);
    }

    /// Gain reduction in dB (for the read-only meter).
    pub fn gain_reduction_db(&self) -> f32 {
        if self.gain <= 0.0 {
            -200.0
        } else {
            20.0 * self.gain.log10()
        }
    }

    pub fn is_open(&self) -> bool {
        self.opened
    }

    fn floor_lin(&self) -> f32 {
        let r = self.range.value();
        if r <= -89.99 {
            0.0
        } else {
            db_to_gain(r)
        }
    }

    /// Control-rate update (every CTL samples on the fixed grid).
    fn control(&mut self) {
        let sr = self.sr;
        self.open_lin = db_to_gain(self.thresh.value());
        self.close_lin = db_to_gain(self.thresh.value() + self.hyst.value());
        let hpf = self.hpf.value();
        let lpf = self.lpf.value();
        self.hp_on = hpf >= 10.0;
        self.lp_on = lpf < 20000.0;
        let hum_f = match self.s.hum {
            1 => 50.0,
            2 => 60.0,
            _ => 0.0,
        };
        self.hum_on = hum_f > 0.0;
        for c in self.chains.iter_mut() {
            if self.hp_on {
                c.hp.set(SvfMode::HighPass, hpf, Q_BW, 0.0, sr);
            }
            if self.lp_on {
                c.lp.set(SvfMode::LowPass, lpf, Q_BW, 0.0, sr);
            }
            if self.hum_on {
                for (k, n) in c.hum.iter_mut().enumerate() {
                    n.set(SvfMode::Notch, hum_f * (k + 1) as f32, 6.0, 0.0, sr);
                }
            }
        }
        // close window: one period of the lowest content the detector sees
        let lowest = hpf.max(30.0);
        let win_s = (1.2 / lowest).clamp(0.008, 0.040);
        self.close_max.set_window((win_s * sr) as usize);
        self.damp_db = self.damp.value() * 0.12;
    }

    #[inline]
    fn sidechain(&mut self, ch: usize, x: f32) -> f32 {
        let c = &mut self.chains[ch];
        let mut v = x;
        if self.hum_on {
            for n in c.hum.iter_mut() {
                v = n.process(v);
            }
        }
        if self.hp_on {
            v = c.hp.process(v);
        }
        if self.lp_on {
            v = c.lp.process(v);
        }
        v
    }

    /// Static curve below the threshold, in dB (≤ 0), floored at range.
    fn curve_db(&self, level_db: f32) -> f32 {
        let t = self.thresh.value();
        let r = self.ratio.value();
        let w = self.knee.value();
        let x = level_db - t;
        let g = if x >= w * 0.5 {
            0.0
        } else if w > 0.0 && x > -w * 0.5 {
            -(r - 1.0) * (x - w * 0.5).powi(2) / (2.0 * w)
        } else {
            (r - 1.0) * x
        };
        g.max(self.range.value())
    }

    fn start_release(&mut self, target: f32) {
        let mut rel_ms = self.s.release.max(1.0);
        if self.s.follow {
            // slope of the close detector over the last 50 ms, dB / s
            let now = self.hist[self.hist_pos];
            let then = self.hist[(self.hist_pos + 1) % self.hist.len()];
            let slope = -((now - then) / 0.05).min(0.0);
            let s = slope.clamp(20.0, 300.0);
            let t = (300f32.ln() - s.ln()) / (300f32.ln() - 20f32.ln());
            let factor = (0.25f32.ln() + t * (4f32.ln() - 0.25f32.ln())).exp();
            rel_ms *= factor;
        }
        // linear in dB from 0 dB down to range (silent range counts as −90 dB)
        let span = self.range.value().min(-1.0);
        let samples = (rel_ms * 0.001 * self.sr).max(1.0);
        self.rel_mult = 10f32.powf(span / (20.0 * samples));
        self.phase = Phase::Release;
        self.att_to = target;
    }

    fn start_attack(&mut self, target: f32) {
        let len = (self.s.attack.max(0.05) * 0.001 * self.sr).round().max(1.0) as u32;
        self.att_from = self.gain;
        self.att_to = target;
        self.att_pos = 0;
        self.att_len = len;
        self.phase = Phase::Attack;
    }

    /// Processes with an optional key (sidechain) input.
    pub fn process_with_key(&mut self, main: &mut [&mut [f32]], key: Option<&[&[f32]]>) {
        let nch = main.len().min(MAX_CH);
        let n = main.first().map_or(0, |b| b.len());
        // key fallback: a key that stays silent for 1 s is not connected
        let mut use_key = false;
        if self.s.source == 1 {
            if let Some(k) = key {
                if k.iter().any(|c| c.iter().any(|&v| v != 0.0)) {
                    self.key_silent = 0;
                } else {
                    self.key_silent += n as u64;
                }
                use_key = self.key_silent < self.sr as u64;
            }
        }
        let hold_samples = (self.s.hold.max(0.0) * 0.001 * self.sr) as u32;
        let ms = (self.sr * 0.001).max(1.0) as u32;

        for i in 0..n {
            if self.clock.is_multiple_of(CTL) {
                for _ in 0..CTL {
                    // the smoothers advance at audio rate in CTL steps
                    self.thresh.next();
                    self.hyst.next();
                    self.range.next();
                    self.ratio.next();
                    self.knee.next();
                    self.damp.next();
                    self.hpf.next();
                    self.lpf.next();
                }
                self.control();
            }
            self.clock += 1;

            // --- detection (linked: max over channels)
            let mut level = 0.0f32;
            let mut listen = [0.0f32; MAX_CH];
            for ch in 0..nch {
                let src = if use_key {
                    let k = key.unwrap();
                    k[ch.min(k.len() - 1)][i]
                } else {
                    main[ch][i]
                };
                let v = self.sidechain(ch, src);
                listen[ch] = v;
                level = level.max(v.abs());
            }
            let close = self.close_max.next(level);

            // follow-decay history, one point per ms
            self.hist_tick += 1;
            if self.hist_tick >= ms {
                self.hist_tick = 0;
                self.hist_pos = (self.hist_pos + 1) % self.hist.len();
                self.hist[self.hist_pos] = if close > 0.0 { 20.0 * close.log10() } else { -200.0 };
            }

            // --- states
            if !self.opened {
                if level > self.open_lin {
                    self.opened = true;
                    self.hold_left = hold_samples;
                }
            } else if close > self.close_lin {
                self.hold_left = hold_samples;
            } else if self.hold_left > 0 {
                self.hold_left -= 1;
            } else {
                self.opened = false;
            }

            // --- target gain
            let floor = self.floor_lin();
            let target = if self.opened {
                1.0
            } else if self.ratio.value() >= 99.5 && self.knee.value() == 0.0 {
                floor
            } else {
                let l = if close > 0.0 { 20.0 * close.log10() } else { -200.0 };
                let g = self.curve_db(l);
                if g <= -89.99 && floor == 0.0 {
                    0.0
                } else {
                    db_to_gain(g).max(floor)
                }
            };

            // --- ballistics
            if target > self.gain {
                if self.phase != Phase::Attack || target != self.att_to {
                    self.start_attack(target);
                }
            } else if target < self.gain && self.phase != Phase::Release {
                self.start_release(target);
            } else if target < self.gain && self.att_to != target {
                self.att_to = target;
            }
            match self.phase {
                Phase::Attack => {
                    self.att_pos += 1;
                    if self.att_pos >= self.att_len {
                        self.gain = self.att_to;
                        self.phase = Phase::Idle;
                    } else {
                        let t = self.att_pos as f32 / self.att_len as f32;
                        let w = 0.5 - 0.5 * (std::f32::consts::PI * t).cos();
                        self.gain = self.att_from + (self.att_to - self.att_from) * w;
                    }
                }
                Phase::Release => {
                    let g = self.gain * self.rel_mult;
                    let lowest = if self.att_to == 0.0 { 3.2e-5 } else { self.att_to };
                    if g <= lowest {
                        self.gain = self.att_to;
                        self.phase = Phase::Idle;
                    } else {
                        self.gain = g;
                    }
                }
                Phase::Idle => {}
            }

            // --- output
            let damp_db = self.damp_db;
            let gain = self.gain;
            let reduction_db = if gain >= 1.0 {
                0.0
            } else if gain <= 0.0 {
                200.0
            } else {
                -20.0 * gain.log10()
            };
            // tail damp: the first `damp_db` of reduction come from the highs
            let (broadband, lp_coef) = if damp_db > 0.0 && reduction_db > 0.0 {
                let r = reduction_db.min(damp_db);
                let fc = 20000.0 * (1500.0f32 / 20000.0).powf(r / damp_db);
                let a = (-2.0 * std::f32::consts::PI * fc / self.sr).exp() as f64;
                let bb = if gain <= 0.0 { 0.0 } else { db_to_gain((-reduction_db + damp_db).min(0.0)) };
                (bb, Some(a))
            } else {
                (gain, None)
            };
            for ch in 0..nch {
                let x = main[ch][i];
                let c = &mut self.chains[ch];
                // the damping low-pass always runs so it never switches in from a stale state
                let a = lp_coef.unwrap_or(0.0);
                c.damp_lp = x as f64 + (c.damp_lp - x as f64) * a;
                let src = if lp_coef.is_some() { c.damp_lp as f32 } else { x };
                main[ch][i] = if self.s.listen { listen[ch] } else if broadband == 1.0 { src } else { src * broadband };
            }
        }
    }
}

impl Default for Gate {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Gate {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        for s in [
            &mut self.thresh,
            &mut self.hyst,
            &mut self.range,
            &mut self.ratio,
            &mut self.knee,
            &mut self.damp,
            &mut self.hpf,
            &mut self.lpf,
        ] {
            s.prepare(sample_rate);
        }
        self.close_max.prepare((0.041 * sample_rate) as usize + 1);
    }

    fn reset(&mut self) {
        for s in [
            &mut self.thresh,
            &mut self.hyst,
            &mut self.range,
            &mut self.ratio,
            &mut self.knee,
            &mut self.damp,
            &mut self.hpf,
            &mut self.lpf,
        ] {
            s.settle();
        }
        for c in self.chains.iter_mut() {
            c.hum.iter_mut().for_each(Svf::reset);
            c.hp.reset();
            c.lp.reset();
            c.damp_lp = 0.0;
        }
        self.close_max.reset();
        self.opened = false;
        self.hold_left = 0;
        self.gain = self.floor_lin();
        self.phase = Phase::Idle;
        self.hist = [-200.0; 51];
        self.hist_pos = 0;
        self.hist_tick = 0;
        self.key_silent = 0;
        self.clock = 0;
        self.control();
    }

    fn set_param(&mut self, id: &str, v: f32) {
        let mut s = self.s;
        match id {
            "thresh" => s.thresh = v,
            "attack" => s.attack = v,
            "hold" => s.hold = v,
            "release" => s.release = v,
            "hpf" => s.hpf = v,
            "lpf" => s.lpf = v,
            "hyst" => s.hyst = v,
            "range" => s.range = v,
            "ratio" => s.ratio = v,
            "knee" => s.knee = v,
            "follow" => s.follow = v >= 0.5,
            "damp" => s.damp = v,
            "hum" => s.hum = v.round().clamp(0.0, 2.0) as u32,
            "source" => s.source = v.round().clamp(0.0, 1.0) as u32,
            "listen" => s.listen = v >= 0.5,
            _ => return,
        }
        self.set(s);
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_with_key(buffers, None);
    }
}

#[cfg(test)]
mod tests;
