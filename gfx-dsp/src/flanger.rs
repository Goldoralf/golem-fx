// SPDX-License-Identifier: GPL-3.0-or-later
//! The flanger engine of gfx-flanger and gfx-flanger-calf (design: gfx-flanger/README.md).
//!
//! Stands for golem's `flanger-js` block (ids `length`, `feedback` in dB, `wet`, `dry`, `rate` kept).
//! - Fractional delay, swept **exponentially** from `manual` to `length`, so the notches move evenly in
//!   pitch.
//! - Feedback is a magnitude in dB with a separate polarity. A soft limiter, drive and filters sit in
//!   the loop.
//! - Modes:
//!   - Mistress, Grey and A/DA on the BBD engine;
//!   - Tape TZ± (true through-zero: the dry is delayed 1 ms, reported as latency);
//!   - Pseudo TZ (the dry goes through an all-pass, zero latency);
//!   - Barberpole ↑ / ↓ (Esqueda's synchronised dual flanger);
//!   - Pedal (the EXP pedal sweeps `manual`).
//!
//! The LFO runs freely: it starts at phase 0 on reset and no parameter change ever moves it.

use crate::bbd::Bbd;
use crate::delay::{DelayLine, Interp};
use crate::envelope::PeakFollower;
use crate::exp_smooth::ExpSmoother;
use crate::lfo::Rng;
use crate::smooth::{LinearRamp, OnePole};
use crate::svf::{Svf, SvfMode};
use crate::transport::{Sync, TransportInfo};
use crate::{Effect, ParamInfo};

const MAX_DELAY_MS: f64 = 32.0;
/// Tape through-zero: the dry's fixed delay.
const TZ_MS: f64 = 1.0;
const CTRL: u32 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Mistress,
    Grey,
    Ada,
    TapeTzPlus,
    TapeTzMinus,
    PseudoTz,
    BarberUp,
    BarberDown,
    Pedal,
}

impl Mode {
    pub const ALL: [Mode; 9] = [Mode::Mistress, Mode::Grey, Mode::Ada, Mode::TapeTzPlus, Mode::TapeTzMinus, Mode::PseudoTz, Mode::BarberUp, Mode::BarberDown, Mode::Pedal];

    fn bbd(self) -> bool {
        matches!(self, Mode::Mistress | Mode::Grey | Mode::Ada)
    }

    fn tape_tz(self) -> bool {
        matches!(self, Mode::TapeTzPlus | Mode::TapeTzMinus)
    }

    fn barber(self) -> bool {
        matches!(self, Mode::BarberUp | Mode::BarberDown)
    }

    /// The mode's own LFO shape (index into the `shape` enum after "Mode").
    fn shape(self) -> u32 {
        match self {
            Mode::Mistress => SH_ROUNDED,
            Mode::Grey => SH_LOG,
            Mode::Ada => SH_TRIANGLE,
            _ => SH_SINE,
        }
    }
}

/// `shape` enum: 0 Mode, then these.
pub const SH_SINE: u32 = 1;
pub const SH_TRIANGLE: u32 = 2;
pub const SH_LOG: u32 = 3;
pub const SH_EXP: u32 = 4;
pub const SH_RAMP_UP: u32 = 5;
pub const SH_RAMP_DOWN: u32 = 6;
pub const SH_RANDOM: u32 = 7;
/// internal: a smoothed triangle (Electric Mistress)
const SH_ROUNDED: u32 = 100;

pub static PARAMS: [ParamInfo; 20] = [
    ParamInfo::float("length", "Length", 0.0, 30.0, 6.0),
    ParamInfo::float("feedback", "Feedback", -60.0, -1.0, -24.0).with_exp(),
    ParamInfo::float("wet", "Wet", -60.0, 0.0, -10.0),
    ParamInfo::float("dry", "Dry", -60.0, 0.0, 0.0),
    ParamInfo::float("rate", "Rate", 0.05, 5.0, 0.3).with_exp(),
    ParamInfo::enumeration("mode", "Mode", 9, 0),
    ParamInfo::float("manual", "Manual", 0.05, 30.0, 0.1).with_exp(),
    ParamInfo::float("depth", "Depth", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::enumeration("fb_polarity", "Feedback Polarity", 2, 0),
    ParamInfo::enumeration("shape", "LFO Shape", 8, 0),
    ParamInfo::enumeration("sweep_src", "Sweep Source", 4, 0),
    ParamInfo::float("env_sens", "Envelope Sensitivity", -40.0, 0.0, -20.0),
    ParamInfo::float("env_att", "Envelope Attack", 1.0, 100.0, 10.0),
    ParamInfo::float("env_rel", "Envelope Release", 20.0, 2000.0, 300.0),
    ParamInfo::float("stereo_phase", "Stereo Phase", 0.0, 180.0, 90.0),
    ParamInfo::enumeration("sync", "Sync", 17, 0),
    ParamInfo::float("drive", "Loop Drive", 0.0, 100.0, 0.0),
    ParamInfo::float("fb_hpf", "Loop Low Cut", 20.0, 500.0, 20.0),
    ParamInfo::float("fb_lpf", "Loop High Cut", 1000.0, 20000.0, 20000.0),
    ParamInfo::float("bass_keep", "Bass Keep", 0.0, 300.0, 0.0),
];

pub mod p {
    pub const LENGTH: usize = 0;
    pub const FEEDBACK: usize = 1;
    pub const WET: usize = 2;
    pub const DRY: usize = 3;
    pub const RATE: usize = 4;
    pub const MODE: usize = 5;
    pub const MANUAL: usize = 6;
    pub const DEPTH: usize = 7;
    pub const FB_POLARITY: usize = 8;
    pub const SHAPE: usize = 9;
    pub const SWEEP_SRC: usize = 10;
    pub const ENV_SENS: usize = 11;
    pub const ENV_ATT: usize = 12;
    pub const ENV_REL: usize = 13;
    pub const STEREO_PHASE: usize = 14;
    pub const SYNC: usize = 15;
    pub const DRIVE: usize = 16;
    pub const FB_HPF: usize = 17;
    pub const FB_LPF: usize = 18;
    pub const BASS_KEEP: usize = 19;
    pub const N: usize = 20;
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
    fn mode(&self) -> Mode {
        Mode::ALL[idx(self.v[p::MODE], 9)]
    }
}

#[inline]
fn idx(v: f32, n: usize) -> usize {
    (v.round().max(0.0) as usize).min(n - 1)
}

#[inline]
fn db(v: f32) -> f32 {
    if v <= -60.0 {
        0.0
    } else {
        10f32.powf(v / 20.0)
    }
}

/// What a structural change switches.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Structure {
    mode: Mode,
    bass_keep: bool,
}

#[derive(Clone)]
struct Channel {
    line: DelayLine,
    bbd: Bbd,
    dry_line: DelayLine,
    dry_ap: [(f32, f32); 4],
    fb: f32,
    fb_hp: Svf,
    fb_lp: Svf,
    lo: [Svf; 2],
    u_smooth: f64,
    rng: Rng,
    rnd_from: f64,
    rnd_to: f64,
    last_phase: f64,
}

impl Channel {
    fn new(seed: u64) -> Self {
        Self {
            line: DelayLine::new(),
            bbd: Bbd::new(512),
            dry_line: DelayLine::new(),
            dry_ap: [(0.0, 0.0); 4],
            fb: 0.0,
            fb_hp: Svf::new(),
            fb_lp: Svf::new(),
            lo: Default::default(),
            u_smooth: 0.0,
            rng: Rng::new(seed),
            rnd_from: 0.0,
            rnd_to: 0.0,
            last_phase: 0.0,
        }
    }

    fn reset(&mut self) {
        self.line.reset();
        self.bbd.reset();
        self.dry_line.reset();
        self.dry_ap = [(0.0, 0.0); 4];
        self.fb = 0.0;
        self.fb_hp.reset();
        self.fb_lp.reset();
        for f in self.lo.iter_mut() {
            f.reset();
        }
        self.u_smooth = 0.0;
        self.rng.reset();
        self.rnd_from = 0.0;
        self.rnd_to = self.rng.next_f64();
        self.last_phase = 0.0;
    }
}

/// Unipolar LFO (0 … 1) at phase p: every shape starts at 0 (the `manual` end) on phase 0.
#[inline]
fn lfo(p: f64, shape: u32, ch: &Channel) -> f64 {
    // a triangle 0 → 1 at p = 0.5 → 0
    let tri = 1.0 - (1.0 - 2.0 * p).abs();
    let a = 3.0;
    match shape {
        SH_TRIANGLE => tri,
        SH_ROUNDED => 0.5 - 0.5 * (0.97 * (std::f64::consts::TAU * p).cos()).asin() / 0.97f64.asin(),
        // log lingers at the top, exp at the bottom
        SH_LOG => 1.0 - ((a * (1.0 - tri)).exp() - 1.0) / (a.exp() - 1.0),
        SH_EXP => ((a * tri).exp() - 1.0) / (a.exp() - 1.0),
        SH_RAMP_UP => p,
        SH_RAMP_DOWN => 1.0 - p,
        SH_RANDOM => ch.rnd_from + (ch.rnd_to - ch.rnd_from) * 0.5 * (1.0 - (std::f64::consts::PI * p).cos()),
        _ => 0.5 - 0.5 * (std::f64::consts::TAU * p).cos(),
    }
}

pub struct Flanger {
    s: Settings,
    sr: f32,
    st: Structure,
    pending: Option<Structure>,
    /// wet dip for structural changes; the whole output dips when the latency changes
    gate: LinearRamp,
    full_dip: bool,
    ch: [Channel; 2],
    env: PeakFollower,
    phase: f64,
    transport: TransportInfo,
    rate: ExpSmoother,
    feedback: ExpSmoother,
    manual: ExpSmoother,
    depth: ExpSmoother,
    length: OnePole,
    wet: OnePole,
    dry: OnePole,
    drive: OnePole,
    sphase: OnePole,
    fb_hpf: OnePole,
    fb_lpf: OnePole,
    bass: OnePole,
    bk_amt: OnePole,
    /// feedback polarity, glided (a sign flip at high regen would click)
    pol: OnePole,
    ctrl: u32,
    clock: u64,
}

impl Flanger {
    pub fn new() -> Self {
        let s = Settings::default();
        let v = s.v;
        Self {
            s,
            sr: 48000.0,
            st: Structure { mode: s.mode(), bass_keep: false },
            pending: None,
            gate: LinearRamp::new(10.0, 1.0),
            full_dip: false,
            ch: [Channel::new(0xF1), Channel::new(0xF2)],
            env: PeakFollower::new(10.0, 300.0),
            phase: 0.0,
            transport: TransportInfo::default(),
            rate: ExpSmoother::new(0.05, 5.0, v[p::RATE]),
            feedback: ExpSmoother::new(-60.0, -1.0, v[p::FEEDBACK]),
            manual: ExpSmoother::new(0.05, 30.0, v[p::MANUAL]),
            depth: ExpSmoother::new(0.0, 100.0, v[p::DEPTH]),
            length: OnePole::new(50.0, v[p::LENGTH], 1e-5),
            wet: OnePole::new(20.0, v[p::WET], 1e-4),
            dry: OnePole::new(20.0, v[p::DRY], 1e-4),
            drive: OnePole::new(20.0, v[p::DRIVE], 1e-4),
            sphase: OnePole::new(50.0, v[p::STEREO_PHASE], 1e-4),
            fb_hpf: OnePole::new(30.0, v[p::FB_HPF], 1e-3),
            fb_lpf: OnePole::new(30.0, v[p::FB_LPF], 1e-2),
            bass: OnePole::new(30.0, v[p::BASS_KEEP].max(20.0), 1e-3),
            bk_amt: OnePole::new(20.0, 0.0, 1e-6),
            pol: OnePole::new(15.0, 1.0, 1e-6),
            ctrl: 0,
            clock: 0,
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn phase(&self) -> f64 {
        self.phase
    }

    /// Reported latency (samples): 1 ms in Tape TZ, else 0.
    pub fn latency_samples(&self) -> usize {
        let st = self.pending.unwrap_or(self.st);
        if st.mode.tape_tz() {
            (TZ_MS * 0.001 * self.sr as f64).round() as usize
        } else {
            0
        }
    }

    pub fn set(&mut self, s: Settings) {
        let v = &s.v;
        let old = self.s.v;
        if v[p::RATE] != old[p::RATE] {
            self.rate.set_target(v[p::RATE].clamp(0.05, 5.0), self.clock);
        }
        if v[p::FEEDBACK] != old[p::FEEDBACK] {
            self.feedback.set_target(v[p::FEEDBACK].clamp(-60.0, -1.0), self.clock);
        }
        if v[p::MANUAL] != old[p::MANUAL] {
            self.manual.set_target(v[p::MANUAL].clamp(0.05, 30.0), self.clock);
        }
        if v[p::DEPTH] != old[p::DEPTH] {
            self.depth.set_target(v[p::DEPTH].clamp(0.0, 100.0), self.clock);
        }
        self.length.set_target(v[p::LENGTH].clamp(0.0, 30.0));
        self.wet.set_target(v[p::WET]);
        self.dry.set_target(v[p::DRY]);
        self.drive.set_target(v[p::DRIVE] * 0.01);
        self.sphase.set_target(v[p::STEREO_PHASE]);
        self.fb_hpf.set_target(v[p::FB_HPF]);
        self.fb_lpf.set_target(v[p::FB_LPF]);
        if v[p::BASS_KEEP] >= 20.0 {
            self.bass.set_target(v[p::BASS_KEEP]);
        }
        self.bk_amt.set_target(if v[p::BASS_KEEP] >= 20.0 { 1.0 } else { 0.0 });
        self.pol.set_target(if idx(v[p::FB_POLARITY], 2) == 1 { -1.0 } else { 1.0 });
        let st = Structure { mode: s.mode(), bass_keep: v[p::BASS_KEEP] >= 20.0 };
        let target = self.pending.unwrap_or(self.st);
        if st != target {
            if st == self.st {
                self.pending = None;
                self.gate.set_target(1.0);
            } else {
                self.full_dip = st.mode.tape_tz() != self.st.mode.tape_tz();
                self.pending = Some(st);
                self.gate.set_target(0.0);
            }
        }
        self.s = s;
    }

    fn control(&mut self) {
        let sr = self.sr;
        let (hp, lp) = (self.fb_hpf.next(), self.fb_lpf.next().min(0.45 * sr));
        let bk = self.bass.next();
        for c in self.ch.iter_mut() {
            c.fb_hp.set(SvfMode::HighPass, hp, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            c.fb_lp.set(SvfMode::LowPass, lp, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            for f in c.lo.iter_mut() {
                f.set(SvfMode::LowPass, bk, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            }
        }
        let v = &self.s.v;
        self.env.set_times(v[p::ENV_ATT].max(1.0), v[p::ENV_REL].max(20.0), sr);
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(2);
        let n = buffers.first().map_or(0, |b| b.len());
        let sr = self.sr as f64;
        let beats = Sync::from_index(idx(self.s.v[p::SYNC], 17)).beats();
        let src = idx(self.s.v[p::SWEEP_SRC], 4);
        let sens = self.s.v[p::ENV_SENS];
        let shape_sel = idx(self.s.v[p::SHAPE], 8) as u32;
        let tz_n = (TZ_MS * 0.001 * sr).round();
        // the sweep position glides (~15 ms): source changes and ramp wraps sweep instead of jumping
        let u_k = (-1.0 / (0.015 * sr)).exp();
        for i in 0..n {
            self.clock += 1;
            if self.ctrl == 0 {
                self.control();
            }
            self.ctrl = (self.ctrl + 1) % CTRL;
            let g = self.gate.next();
            if g == 0.0 && self.gate.is_settled() {
                if let Some(st) = self.pending.take() {
                    self.st = st;
                    for c in self.ch.iter_mut() {
                        c.reset();
                    }
                }
                self.gate.set_target(1.0);
            }
            let gs = 0.5 - 0.5 * (std::f32::consts::PI * g).cos();
            let (g_wet, g_all) = if self.full_dip { (1.0, gs) } else { (gs, 1.0) };

            let rate = self.rate.next() as f64;
            let fb_db = self.feedback.next();
            let manual = self.manual.next() as f64;
            let depth = self.depth.next() as f64 * 0.01;
            let length = self.length.next() as f64;
            let wet_g = db(self.wet.next());
            let dry_g = db(self.dry.next());
            let drive = self.drive.next();
            let sph = self.sphase.next() as f64 / 360.0;
            let bk = self.bk_amt.next();
            let fbg = self.pol.next() * db(fb_db);
            let hz = match beats {
                Some(b) => self.transport.bpm() / 60.0 / b,
                None => rate,
            };
            match beats.and_then(|b| self.transport.phase_at(i, b, 0.0, self.sr)) {
                Some(ph) => self.phase = ph,
                None => self.phase = (self.phase + hz / sr).fract(),
            }
            let mode = self.st.mode;
            let shape = if shape_sel == 0 { mode.shape() } else { shape_sel };
            // envelope position
            let mut lvl = 0.0f32;
            for c in 0..nch {
                lvl = lvl.max(buffers[c][i].abs());
            }
            let e = self.env.next(lvl);
            let u_env = (((20.0 * e.max(1e-6).log10()) - sens + 30.0) / 30.0).clamp(0.0, 1.0) as f64;
            let kd = 1.0 + 3.0 * drive;
            for c in 0..nch {
                let x = buffers[c][i];
                let chs = &mut self.ch[c];
                let ph = if c == 0 { self.phase } else { (self.phase + sph).fract() };
                if ph < chs.last_phase && chs.last_phase - ph > 0.5 {
                    chs.rnd_from = chs.rnd_to;
                    chs.rnd_to = chs.rng.next_f64();
                }
                chs.last_phase = ph;
                // bass keep: Linkwitz-Riley split; the lows bypass everything
                // complementary split (lows + highs = the input exactly), so switching it never colours
                let lo = chs.lo[0].process(x);
                let lo = chs.lo[1].process(lo);
                // (not in Tape TZ: its dry is delayed, the lows would no longer line up)
                let bk = if mode.tape_tz() { 0.0 } else { bk };
                let xin = x - bk * lo;
                let low_part = bk * lo;
                // sweep position
                let u_lfo = lfo(ph, shape, chs);
                let u = match (mode, src) {
                    (Mode::Pedal, _) | (_, 2) => 0.0,
                    (_, 1) => u_env,
                    (_, 3) => (u_lfo + u_env).min(1.0),
                    _ => u_lfo,
                };
                chs.u_smooth = u + (chs.u_smooth - u) * u_k;
                let u = chs.u_smooth;
                let lo_ms = manual.max(0.05);
                let hi_ms = length.max(lo_ms * 1.001);
                let fb_in = chs.fb;
                let line_in = xin + fbg * fb_in;
                let wet = if mode.barber() {
                    // two reads on sawtooth ramps half a cycle apart, each faded by sin²
                    chs.line.push(line_in);
                    let mut y = 0.0f32;
                    for k in 0..2 {
                        let r = (ph + 0.5 * k as f64).fract();
                        let ramp = if mode == Mode::BarberUp { 1.0 - r } else { r };
                        let d = lo_ms * (hi_ms / lo_ms).powf(ramp * depth);
                        let w = (std::f64::consts::PI * r).sin().powi(2) as f32;
                        y += w * chs.line.read((d * 0.001 * sr).max(3.0), Interp::Lagrange5);
                    }
                    y
                } else if mode.tape_tz() {
                    // through zero: the wet sweeps around the dry's fixed 1 ms, `manual` past zero
                    let tau = -lo_ms.min(TZ_MS * 0.95) + (hi_ms + lo_ms.min(TZ_MS * 0.95)) * u * depth;
                    chs.line.push(line_in);
                    let d = ((TZ_MS + tau) * 0.001 * sr).max(3.0);
                    chs.line.read(d, Interp::Lagrange5)
                } else {
                    let d_ms = lo_ms * (hi_ms / lo_ms).powf(u * depth);
                    if mode.bbd() {
                        chs.bbd.set_delay(d_ms * 0.001);
                        chs.bbd.process(line_in)
                    } else {
                        chs.line.push(line_in);
                        chs.line.read((d_ms * 0.001 * sr).max(3.0), Interp::Lagrange5)
                    }
                };
                // the loop: filters, drive, soft limiter (the comb's peaks reach +19 dB in there)
                let f = chs.fb_lp.process(chs.fb_hp.process(wet));
                chs.fb = (f * kd).tanh() / kd;
                let wet = if mode == Mode::TapeTzMinus { -wet } else { wet };
                // the dry path: delayed in Tape TZ, all-passed in Pseudo TZ (≈ 0.25 ms of group delay)
                chs.dry_line.push(xin);
                let dry = if mode.tape_tz() {
                    chs.dry_line.tap(tz_n as usize)
                } else if mode == Mode::PseudoTz {
                    let t = (std::f64::consts::PI * 5100.0 / sr).tan();
                    let a = ((t - 1.0) / (t + 1.0)) as f32;
                    let mut v = xin;
                    for s in chs.dry_ap.iter_mut() {
                        let y = a * v + s.0 - a * s.1;
                        s.0 = v;
                        s.1 = y;
                        v = y;
                    }
                    v
                } else {
                    xin
                };
                buffers[c][i] = (low_part + dry_g * dry + wet_g * g_wet * wet) * g_all;
            }
        }
    }
}

impl Default for Flanger {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Flanger {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        for e in [&mut self.rate, &mut self.feedback, &mut self.manual, &mut self.depth] {
            e.prepare(sample_rate);
        }
        for o in [&mut self.length, &mut self.wet, &mut self.dry, &mut self.drive, &mut self.sphase, &mut self.bk_amt, &mut self.pol] {
            o.prepare(sample_rate);
        }
        for o in [&mut self.fb_hpf, &mut self.fb_lpf, &mut self.bass] {
            o.prepare(sample_rate / CTRL as f32);
        }
        self.gate.prepare(sample_rate);
        self.env.prepare(sample_rate);
        let len = (MAX_DELAY_MS * 0.001 * sample_rate as f64 * 1.1) as usize + 16;
        for c in self.ch.iter_mut() {
            c.line.prepare(len);
            c.dry_line.prepare((TZ_MS * 0.001 * sample_rate as f64) as usize + 8);
            c.bbd.prepare(sample_rate);
            c.bbd.set_filters(0.45, 12000.0);
        }
        self.control();
    }

    fn reset(&mut self) {
        if let Some(st) = self.pending.take() {
            self.st = st;
        }
        let v = self.s.v;
        self.rate.settle(v[p::RATE]);
        self.feedback.settle(v[p::FEEDBACK].clamp(-60.0, -1.0));
        self.manual.settle(v[p::MANUAL].clamp(0.05, 30.0));
        self.depth.settle(v[p::DEPTH]);
        for o in [&mut self.length, &mut self.wet, &mut self.dry, &mut self.drive, &mut self.sphase, &mut self.bk_amt, &mut self.pol, &mut self.fb_hpf, &mut self.fb_lpf, &mut self.bass] {
            o.settle();
        }
        self.gate.set_immediate(1.0);
        self.full_dip = false;
        for c in self.ch.iter_mut() {
            c.reset();
        }
        self.env.reset();
        self.phase = 0.0;
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

    fn latency(&self) -> usize {
        self.latency_samples()
    }

    fn set_transport(&mut self, transport: TransportInfo) {
        self.transport = transport;
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_block(buffers);
    }
}

