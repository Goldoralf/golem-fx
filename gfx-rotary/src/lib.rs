// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-rotary — golem's rotary speaker (design: README.md).
//!
//! Stands for golem's `rotary` block (ids `speed`, `mic`, `spacing`, `depth` kept). For guitar and keys
//! (golem's organ keeps setBfree's own Leslie).
//!
//! - **Motors**: each rotor's speed is a first-order system with its own accelerate and decelerate
//!   constants (setBfree, Herrera et al.). The horn reaches full speed while the drum still lags.
//! - **Paths**: the horn mouth and the drum baffle turn on their radius. Each mic hears the direct path
//!   and the cabinet's reflections. The delay of each path is its real length, so the Doppler is exact.
//!   Each path is shaded by its angle: level and brightness follow where the rotor points.
//! - Tube stage (ADAA), sub-bass kept mono and unmodulated, 800 Hz crossover, cabinet voicing.
//! - Brake: the rotors coast with their decelerate constants; the horn parks at `stop_angle`.
//!
//! Mono in, stereo out (a stereo input is summed). The paths are read relative to their shortest
//! length, so the added delay is the rotation's own swing (~1 ms), not the mic distance. It is not
//! reported. Deterministic: horn at 0, drum at 0.37 turn, fixed seeds.

#![allow(clippy::needless_range_loop)]

use gfx_dsp::adaa::{Adaa1, Shaper};
use gfx_dsp::delay::{DelayLine, Interp};
use gfx_dsp::lfo::Rng;
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::util::one_pole_coef;
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
pub mod plugin;

const C_SOUND: f64 = 343.0;
const TAU: f64 = std::f64::consts::TAU;
/// Reflections: extra path (m), gain, and angle offset (turns: 0.5 = the back wall).
const HORN_REFL: [(f64, f32, f64); 3] = [(0.38, 0.32, 0.5), (0.81, 0.22, 0.25), (1.27, 0.14, 0.75)];
const DRUM_REFL: [(f64, f32, f64); 2] = [(0.45, 0.30, 0.5), (0.95, 0.18, 0.3)];
const MAX_PATH_MS: f64 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cabinet {
    L122,
    L147,
    L145,
    Vibratone,
    L122Rear,
}

impl Cabinet {
    pub const ALL: [Cabinet; 5] = [Cabinet::L122, Cabinet::L147, Cabinet::L145, Cabinet::Vibratone, Cabinet::L122Rear];
}

/// A cabinet's voicing.
struct Voicing {
    horn_r: f64,
    drum_r: f64,
    has_horn: bool,
    lp: f32,
    drive_gain: f32,
    direct: f32,
    refl: f32,
    /// the cabinet's low-pass and shading cost some level on a guitar: made up here
    makeup: f32,
}

fn voicing(c: Cabinet) -> Voicing {
    match c {
        Cabinet::L122 => Voicing { horn_r: 0.192, drum_r: 0.22, has_horn: true, lp: 6000.0, drive_gain: 1.0, direct: 1.0, refl: 1.0, makeup: 1.12 },
        Cabinet::L147 => Voicing { horn_r: 0.192, drum_r: 0.22, has_horn: true, lp: 6500.0, drive_gain: 1.6, direct: 1.0, refl: 1.0, makeup: 1.12 },
        Cabinet::L145 => Voicing { horn_r: 0.15, drum_r: 0.18, has_horn: true, lp: 5500.0, drive_gain: 1.2, direct: 1.0, refl: 0.8, makeup: 1.15 },
        Cabinet::Vibratone => Voicing { horn_r: 0.0, drum_r: 0.15, has_horn: false, lp: 5000.0, drive_gain: 1.3, direct: 1.0, refl: 0.6, makeup: 1.35 },
        Cabinet::L122Rear => Voicing { horn_r: 0.192, drum_r: 0.22, has_horn: true, lp: 4500.0, drive_gain: 1.0, direct: 0.6, refl: 1.6, makeup: 1.25 },
    }
}

pub static PARAMS: [ParamInfo; 23] = [
    ParamInfo::enumeration("speed", "Speed", 3, 1),
    ParamInfo::float("mic", "Mic Distance", 0.0, 100.0, 70.0),
    ParamInfo::float("spacing", "Mic Spacing", 0.0, 100.0, 50.0),
    ParamInfo::float("depth", "Depth", 0.0, 100.0, 60.0),
    ParamInfo::enumeration("cabinet", "Cabinet", 5, 0),
    ParamInfo::float("ramp", "Ramp", 0.0, 400.0, 100.0).with_exp(),
    ParamInfo::float("drive", "Drive", 0.0, 100.0, 20.0).with_exp(),
    ParamInfo::float("balance", "Horn / Drum", 0.0, 100.0, 50.0),
    ParamInfo::float("horn_slow", "Horn Slow", 0.3, 2.0, 0.8),
    ParamInfo::float("horn_fast", "Horn Fast", 3.0, 9.0, 6.7),
    ParamInfo::float("drum_slow", "Drum Slow", 0.3, 2.0, 0.67),
    ParamInfo::float("drum_fast", "Drum Fast", 2.5, 8.0, 5.7),
    ParamInfo::float("horn_accel", "Horn Accel", 0.05, 3.0, 0.16),
    ParamInfo::float("horn_decel", "Horn Decel", 0.05, 3.0, 0.32),
    ParamInfo::float("drum_accel", "Drum Accel", 0.2, 12.0, 4.1),
    ParamInfo::float("drum_decel", "Drum Decel", 0.2, 12.0, 1.4),
    ParamInfo::float("brake", "Brake", 0.0, 100.0, 50.0),
    ParamInfo::float("stop_angle", "Stop Angle", 0.0, 360.0, 0.0),
    ParamInfo::boolean("counter", "Counter-rotate", true),
    ParamInfo::float("wobble", "Wobble", 0.0, 100.0, 30.0),
    ParamInfo::float("bass_keep", "Bass Keep", 0.0, 200.0, 150.0),
    ParamInfo::float("level", "Level", -24.0, 12.0, 0.0),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 100.0).with_exp(),
];

pub mod p {
    pub const SPEED: usize = 0;
    pub const MIC: usize = 1;
    pub const SPACING: usize = 2;
    pub const DEPTH: usize = 3;
    pub const CABINET: usize = 4;
    pub const RAMP: usize = 5;
    pub const DRIVE: usize = 6;
    pub const BALANCE: usize = 7;
    pub const HORN_SLOW: usize = 8;
    pub const HORN_FAST: usize = 9;
    pub const DRUM_SLOW: usize = 10;
    pub const DRUM_FAST: usize = 11;
    pub const HORN_ACCEL: usize = 12;
    pub const HORN_DECEL: usize = 13;
    pub const DRUM_ACCEL: usize = 14;
    pub const DRUM_DECEL: usize = 15;
    pub const BRAKE: usize = 16;
    pub const STOP_ANGLE: usize = 17;
    pub const COUNTER: usize = 18;
    pub const WOBBLE: usize = 19;
    pub const BASS_KEEP: usize = 20;
    pub const LEVEL: usize = 21;
    pub const MIX: usize = 22;
    pub const N: usize = 23;
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

/// A rotor: speed (Hz) on its own inertia, angle in turns.
#[derive(Clone, Copy, Debug, Default)]
pub struct Rotor {
    pub hz: f64,
    pub angle: f64,
    tau: f64,
    k: f64,
}

impl Rotor {
    #[inline]
    fn step(&mut self, target: f64, tau_up: f64, tau_down: f64, dir: f64, sr: f64) {
        let tau = if target > self.hz { tau_up } else { tau_down };
        // the coefficient only changes with the time constant: an exp per change, not per sample
        if tau != self.tau {
            self.tau = tau;
            self.k = if tau <= 1e-4 { 0.0 } else { (-1.0 / (tau * sr)).exp() };
        }
        let k = self.k;
        self.hz = target + (self.hz - target) * k;
        self.angle = (self.angle + dir * self.hz / sr).rem_euclid(1.0);
    }
}

/// A path's delay, gain and shading coefficient, interpolated linearly across a 4-sample segment (the
/// geometry moves at most ~7 turns per second: computing it every sample was most of the cost).
#[derive(Clone, Copy, Default)]
struct PathLerp {
    d: f64,
    dd: f64,
    g: f32,
    dg: f32,
    k: f32,
    dk: f32,
}

impl PathLerp {
    /// Aims at the end values over `seg` samples (or jumps there on the first segment).
    #[inline]
    fn aim(&mut self, d: f64, g: f32, k: f32, seg: f64, jump: bool) {
        if jump {
            *self = PathLerp { d, g, k, ..Default::default() };
        } else {
            self.dd = (d - self.d) / seg;
            self.dg = (g - self.g) / seg as f32;
            self.dk = (k - self.k) / seg as f32;
        }
    }

    #[inline]
    fn advance(&mut self) {
        self.d += self.dd;
        self.g += self.dg;
        self.k += self.dk;
    }
}

const SEG: u32 = 4;

/// One path: shading filter (one-pole low-pass whose cut-off follows the angle).
#[derive(Clone, Copy, Default)]
struct Shade {
    z: f32,
}

impl Shade {
    #[inline]
    fn process(&mut self, x: f32, k: f32) -> f32 {
        self.z += k * (x - self.z);
        self.z
    }
}

pub struct Rotary {
    s: Settings,
    sr: f32,
    horn: Rotor,
    drum: Rotor,
    horn_line: DelayLine,
    drum_line: DelayLine,
    /// [mic][path]
    horn_shade: [[Shade; 4]; 2],
    drum_shade: [Shade; 3],
    adaa: Adaa1,
    sub: [Svf; 2],
    xo: [Svf; 2],
    cab_lp: [Svf; 2],
    rng: Rng,
    drift: [f64; 2],
    drift_t: [f64; 2],
    drift_ph: f64,
    parked: bool,
    mic: OnePole,
    spacing: OnePole,
    depth: OnePole,
    drive: OnePole,
    balance: OnePole,
    level: OnePole,
    mix: OnePole,
    cab_fade: LinearRamp,
    cab: Cabinet,
    pending_cab: Option<Cabinet>,
    ramp: OnePole,
    sub_amt: OnePole,
    /// shading one-pole coefficients at the dark / bright ends (horn, drum, Vibratone drum)
    k_dark: f32,
    k_bright: f32,
    k_drum_lo: f32,
    k_drum_hi: f32,
    k_vib_lo: f32,
    k_vib_hi: f32,
    horn_trig: [[(f64, f64); 4]; 2],
    drum_trig: [(f64, f64); 3],
    ctrl: u32,
    horn_p: [[PathLerp; 4]; 2],
    drum_p: [PathLerp; 3],
    seg: u32,
    paths_init: bool,
}

impl Rotary {
    pub fn new() -> Self {
        let s = Settings::default();
        let v = s.v;
        Self {
            s,
            sr: 48000.0,
            horn: Rotor::default(),
            drum: Rotor::default(),
            horn_line: DelayLine::new(),
            drum_line: DelayLine::new(),
            horn_shade: Default::default(),
            drum_shade: Default::default(),
            adaa: Adaa1::new(),
            sub: Default::default(),
            xo: Default::default(),
            cab_lp: Default::default(),
            rng: Rng::new(0x1E5),
            drift: [0.0; 2],
            drift_t: [0.0; 2],
            drift_ph: 0.0,
            parked: false,
            mic: OnePole::new(50.0, v[p::MIC], 1e-4),
            spacing: OnePole::new(50.0, v[p::SPACING], 1e-4),
            depth: OnePole::new(50.0, v[p::DEPTH], 1e-4),
            drive: OnePole::new(30.0, v[p::DRIVE], 1e-4),
            balance: OnePole::new(30.0, v[p::BALANCE], 1e-4),
            level: OnePole::new(20.0, v[p::LEVEL], 1e-4),
            mix: OnePole::new(20.0, v[p::MIX], 1e-4),
            cab_fade: LinearRamp::new(15.0, 1.0),
            cab: Cabinet::L122,
            pending_cab: None,
            ramp: OnePole::new(30.0, v[p::RAMP], 1e-4),
            sub_amt: OnePole::new(20.0, 1.0, 1e-6),
            k_dark: 0.0,
            k_bright: 0.0,
            k_drum_lo: 0.0,
            k_drum_hi: 0.0,
            k_vib_lo: 0.0,
            k_vib_hi: 0.0,
            horn_trig: [[(0.0, 1.0); 4]; 2],
            drum_trig: [(0.0, 1.0); 3],
            ctrl: 0,
            horn_p: Default::default(),
            drum_p: Default::default(),
            seg: 0,
            paths_init: false,
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn horn(&self) -> Rotor {
        self.horn
    }

    pub fn drum(&self) -> Rotor {
        self.drum
    }

    pub fn set(&mut self, s: Settings) {
        let v = &s.v;
        self.mic.set_target(v[p::MIC]);
        self.spacing.set_target(v[p::SPACING]);
        self.depth.set_target(v[p::DEPTH]);
        self.drive.set_target(v[p::DRIVE]);
        self.balance.set_target(v[p::BALANCE]);
        self.level.set_target(v[p::LEVEL]);
        self.mix.set_target(v[p::MIX]);
        self.ramp.set_target(v[p::RAMP]);
        self.sub_amt.set_target(if v[p::BASS_KEEP] >= 20.0 { 1.0 } else { 0.0 });
        let cab = Cabinet::ALL[idx(v[p::CABINET], 5)];
        let target = self.pending_cab.unwrap_or(self.cab);
        if cab != target {
            if cab == self.cab {
                self.pending_cab = None;
                self.cab_fade.set_target(1.0);
            } else {
                self.pending_cab = Some(cab);
                self.cab_fade.set_target(0.0);
            }
        }
        if v[p::BASS_KEEP] != self.s.v[p::BASS_KEEP] {
            self.s = s;
            self.set_filters();
        }
        self.s = s;
    }

    fn set_filters(&mut self) {
        let sr = self.sr;
        let bk = self.s.v[p::BASS_KEEP].max(20.0);
        for f in self.sub.iter_mut() {
            f.set(SvfMode::LowPass, bk, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
        }
        for f in self.xo.iter_mut() {
            f.set(SvfMode::LowPass, 800.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
        }
        let k = |fc: f32| 1.0 - (-std::f32::consts::TAU * fc / sr).exp();
        self.k_dark = k(1500.0);
        self.k_bright = k(10500.0f32.min(0.45 * sr));
        self.k_drum_lo = k(900.0);
        self.k_drum_hi = k(1500.0);
        self.k_vib_lo = k(2000.0);
        self.k_vib_hi = k(8000.0);
        let lp = voicing(self.cab).lp.min(0.45 * sr);
        for f in self.cab_lp.iter_mut() {
            f.set(SvfMode::LowPass, lp, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
        }
    }


    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(2);
        let n = buffers.first().map_or(0, |b| b.len());
        let sr = self.sr as f64;
        let v = self.s.v;
        let speed = idx(v[p::SPEED], 3);
        let dir_d = if v[p::COUNTER] >= 0.5 { -1.0 } else { 1.0 };
        let brake = v[p::BRAKE] as f64 * 0.01;
        let stop_angle = v[p::STOP_ANGLE] as f64 / 360.0;
        let wobble = v[p::WOBBLE] as f64 * 0.01;
        let drift_k = one_pole_coef(sr / (TAU * 0.4));
        for i in 0..n {
            // cabinet changes dip the rotary path 15 ms
            let g = self.cab_fade.next();
            if g == 0.0 && self.cab_fade.is_settled() {
                if let Some(c) = self.pending_cab.take() {
                    self.cab = c;
                    self.set_filters();
                }
                self.cab_fade.set_target(1.0);
            }
            let gs = 0.5 - 0.5 * (std::f32::consts::PI * g).cos();
            let voi = voicing(self.cab);
            // motors
            let ramp = (self.ramp.next() as f64 * 0.01).max(0.0);
            let (ht, dt) = match speed {
                0 => (0.0, 0.0),
                2 => (v[p::HORN_FAST] as f64, v[p::DRUM_FAST] as f64),
                _ => (v[p::HORN_SLOW] as f64, v[p::DRUM_SLOW] as f64),
            };
            // stop coasts on the decelerate constants scaled by the brake (0 = abrupt)
            let bscale = if speed == 0 { (2.0 * brake).max(0.02) } else { 1.0 };
            // wobble: slow deterministic drift of each motor (±2 % at 100 %)
            self.drift_ph += 0.4 / sr;
            if self.drift_ph >= 1.0 {
                self.drift_ph -= 1.0;
                self.drift_t = [self.rng.bipolar() as f64, self.rng.bipolar() as f64];
            }
            for k in 0..2 {
                self.drift[k] = self.drift_t[k] + (self.drift[k] - self.drift_t[k]) * drift_k;
            }
            let hw = 1.0 + 0.02 * wobble * self.drift[0];
            let dw = 1.0 + 0.02 * wobble * self.drift[1];
            if speed == 0 && self.horn.hz < 0.1 {
                // park the horn at the stop angle (shortest way, a gentle servo)
                let mut diff = (stop_angle - self.horn.angle).rem_euclid(1.0);
                if diff > 0.5 {
                    diff -= 1.0;
                }
                self.parked = diff.abs() < 1e-3;
                self.horn.hz = (diff * 2.0).clamp(-0.1, 0.1);
                self.horn.angle = (self.horn.angle + self.horn.hz / sr).rem_euclid(1.0);
            } else {
                self.parked = false;
                self.horn.step(ht * hw, v[p::HORN_ACCEL] as f64 * ramp, v[p::HORN_DECEL] as f64 * ramp * bscale, 1.0, sr);
            }
            self.drum.step(dt * dw, v[p::DRUM_ACCEL] as f64 * ramp, v[p::DRUM_DECEL] as f64 * ramp * bscale, dir_d, sr);

            // input: mono sum, tube stage, sub-bass split, crossover
            let x = if nch > 1 { 0.5 * (buffers[0][i] + buffers[1][i]) } else { buffers[0][i] };
            let drive = self.drive.next() * 0.01 * voi.drive_gain;
            let kd = 1.0 + 6.0 * drive;
            // the tube stage only runs while it is heard (at 0 its weight is 0: exactly clean)
            let xt = if drive > 0.0 {
                let sat = self.adaa.process(Shaper::Asym(0.15), x * kd) / kd;
                x + (sat - x) * (5.0 * drive).min(1.0)
            } else {
                x
            };
            let s0 = self.sub[0].process(xt);
            let sub = self.sub[1].process(s0) * self.sub_amt.next();
            let rest = xt - sub;
            let d0 = self.xo[0].process(rest);
            let low = self.xo[1].process(d0);
            let (drum_in, horn_in) = if voi.has_horn { (low, rest - low) } else { (rest, 0.0) };
            self.horn_line.push(horn_in);
            self.drum_line.push(drum_in);

            // geometry
            let mic = self.mic.next() as f64 * 0.01;
            let spacing = self.spacing.next() as f64 * 0.01;
            let depth = self.depth.next() as f64 * 0.01;
            let dist = 0.25 + 0.95 * mic;
            // 100 % ≈ 1.5 × the real radius
            let geo = 1.5 * depth;
            let am = (depth * (1.0 - 0.5 * mic)) as f32;
            let refl_g = ((0.3 + 0.7 * mic) * voi.refl as f64) as f32;
            let half_sp = 0.25 * spacing; // ± 90° at 100 %
            // the mics' and reflections' angles, at control rate (spacing glides slowly)
            if self.ctrl == 0 {
                for m in 0..2 {
                    let mang = if m == 0 { -half_sp } else { half_sp };
                    for k in 0..4 {
                        let aoff = if k == 0 { 0.0 } else { HORN_REFL[k - 1].2 };
                        self.horn_trig[m][k] = (TAU * (aoff - mang)).sin_cos();
                    }
                }
                for k in 0..3 {
                    let aoff = if k == 0 { 0.0 } else { DRUM_REFL[k - 1].2 };
                    self.drum_trig[k] = (TAU * aoff).sin_cos();
                }
            }
            self.ctrl = (self.ctrl + 1) % 32;
            // every 4 samples: each path's delay, gain and shading at the segment's end (angles 4 samples
            // ahead), then a linear glide to them
            if self.seg == 0 {
                let ahead = SEG as f64 / sr;
                let jump = !self.paths_init;
                self.paths_init = true;
                if voi.has_horn {
                    let r = voi.horn_r * geo;
                    let base = dist - r;
                    let (sh, ch) = (TAU * (self.horn.angle + self.horn.hz * ahead)).sin_cos();
                    for m in 0..2 {
                        for k in 0..4 {
                            let (extra, gain) = if k == 0 { (0.0, voi.direct) } else { (HORN_REFL[k - 1].0, HORN_REFL[k - 1].1 * refl_g) };
                            let (sp, cp) = self.horn_trig[m][k];
                            let cosv = ch * cp - sh * sp;
                            let len = (dist * dist + r * r - 2.0 * dist * r * cosv).sqrt() - base + extra;
                            let facing = (0.5 + 0.5 * cosv) as f32;
                            let g = gain * (1.0 - am * (1.0 - facing));
                            let kf = self.k_dark + (self.k_bright - self.k_dark) * facing;
                            self.horn_p[m][k].aim((len / C_SOUND * sr).max(2.0), g, kf, SEG as f64, jump);
                        }
                    }
                }
                let r = voi.drum_r * geo * 0.6;
                let base = dist - r;
                let (sd, cd) = (TAU * (self.drum.angle + dir_d * self.drum.hz * ahead)).sin_cos();
                let (kd0, kd1) = if voi.has_horn { (self.k_drum_lo, self.k_drum_hi) } else { (self.k_vib_lo, self.k_vib_hi) };
                for k in 0..3 {
                    let (extra, gain) = if k == 0 { (0.0, voi.direct) } else { (DRUM_REFL[k - 1].0, DRUM_REFL[k - 1].1 * refl_g) };
                    let (sp, cp) = self.drum_trig[k];
                    let cosv = cd * cp - sd * sp;
                    let len = (dist * dist + r * r - 2.0 * dist * r * cosv).sqrt() - base + extra;
                    let facing = (0.5 + 0.5 * cosv) as f32;
                    let g = gain * (1.0 - 1.3 * am.min(0.75) * (1.0 - facing * facing));
                    let kf = kd0 + (kd1 - kd0) * facing;
                    self.drum_p[k].aim((len / C_SOUND * sr).max(2.0), g, kf, SEG as f64, jump);
                }
            }
            self.seg = (self.seg + 1) % SEG;
            let mut horn_out = [0.0f32; 2];
            if voi.has_horn {
                for m in 0..2 {
                    let mut acc = 0.0f32;
                    for k in 0..4 {
                        let pth = &mut self.horn_p[m][k];
                        // the direct path cubic; the reflections (already shaded dark) linear
                        let y = self.horn_line.read(pth.d, if k == 0 { Interp::Lagrange3 } else { Interp::Linear });
                        acc += pth.g * self.horn_shade[m][k].process(y, pth.k);
                        pth.advance();
                    }
                    horn_out[m] = acc;
                }
            }
            let mut drum_out = 0.0f32;
            for k in 0..3 {
                let pth = &mut self.drum_p[k];
                let y = self.drum_line.read(pth.d, Interp::Lagrange3);
                drum_out += pth.g * self.drum_shade[k].process(y, pth.k);
                pth.advance();
            }
            let bal = self.balance.next() * 0.01;
            let (gh, gd) = ((2.0 * bal).min(1.0), (2.0 * (1.0 - bal)).min(1.0));
            // loudness-neutral: divide by each rotor's RMS gain over a turn. With f = facing:
            // horn g = 1 − am + am·f, E[g²] = 1 − am + 3/8·am²;
            // drum g = 1 − c + c·f², E[g²] = (1 − c)² + 3/4·c(1 − c) + 35/128·c²
            let c = 1.3 * am.min(0.75);
            let rms_h = (1.0 - am + 0.375 * am * am).sqrt();
            let rms_d = ((1.0 - c) * (1.0 - c) + 0.75 * c * (1.0 - c) + 0.2734 * c * c).sqrt();
            let norm_h = voi.makeup / ((voi.direct + 0.68 * refl_g) * rms_h);
            let norm_d = voi.makeup / ((voi.direct + 0.48 * refl_g) * rms_d);
            let mix = self.mix.next() * 0.01;
            let lvl = self.level.next();
            let og = if lvl == 0.0 { 1.0 } else { 10f32.powf(lvl / 20.0) };
            let mut out = [0.0f32; 2];
            for m in 0..2 {
                let rot = gh * horn_out[m] * norm_h + gd * drum_out * norm_d;
                let rot = self.cab_lp[m].process(rot) * gs;
                let wet = sub + rot;
                out[m] = (x + (wet - x) * mix) * og;
            }
            if nch > 1 {
                buffers[0][i] = out[0];
                buffers[1][i] = out[1];
            } else {
                buffers[0][i] = 0.5 * (out[0] + out[1]);
            }
        }
    }
}

impl Default for Rotary {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Rotary {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        for o in [&mut self.mic, &mut self.spacing, &mut self.depth, &mut self.drive, &mut self.balance, &mut self.level, &mut self.mix, &mut self.ramp, &mut self.sub_amt] {
            o.prepare(sample_rate);
        }
        self.cab_fade.prepare(sample_rate);
        let len = (MAX_PATH_MS * 0.001 * sample_rate as f64) as usize + 16;
        self.horn_line.prepare(len);
        self.drum_line.prepare(len);
        self.set_filters();
    }

    fn reset(&mut self) {
        if let Some(c) = self.pending_cab.take() {
            self.cab = c;
        }
        self.cab = Cabinet::ALL[idx(self.s.v[p::CABINET], 5)];
        for o in [&mut self.mic, &mut self.spacing, &mut self.depth, &mut self.drive, &mut self.balance, &mut self.level, &mut self.mix, &mut self.ramp, &mut self.sub_amt] {
            o.settle();
        }
        self.cab_fade.set_immediate(1.0);
        let v = self.s.v;
        let speed = idx(v[p::SPEED], 3);
        let (h, d) = match speed {
            0 => (0.0, 0.0),
            2 => (v[p::HORN_FAST] as f64, v[p::DRUM_FAST] as f64),
            _ => (v[p::HORN_SLOW] as f64, v[p::DRUM_SLOW] as f64),
        };
        self.horn = Rotor { hz: h, angle: 0.0, tau: -1.0, k: 0.0 };
        self.drum = Rotor { hz: d, angle: 0.37, tau: -1.0, k: 0.0 };
        self.horn_line.reset();
        self.drum_line.reset();
        self.horn_shade = Default::default();
        self.drum_shade = Default::default();
        self.adaa.reset(Shaper::Asym(0.15));
        for f in self.sub.iter_mut().chain(self.xo.iter_mut()).chain(self.cab_lp.iter_mut()) {
            f.reset();
        }
        self.rng.reset();
        self.drift = [0.0; 2];
        self.drift_t = [self.rng.bipolar() as f64, self.rng.bipolar() as f64];
        self.drift_ph = 0.0;
        self.parked = false;
        self.ctrl = 0;
        self.seg = 0;
        self.paths_init = false;
        self.set_filters();
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
