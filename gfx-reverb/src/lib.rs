// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-reverb — golem's reverb (design: README.md).
//!
//! One plugin for golem's `reverb`, `reverb-plate` and `reverb-room` blocks. They share the ids `mix`,
//! `decay`, `size`, `predelay` and `hicut`; golem pins `algo` per block at insert.
//!
//! Algorithms, on gfx-dsp's reverb engine:
//! - FDN-16 tails: Hall, Room, Chamber, Cathedral, Cloud;
//! - Dattorro tank: Plate;
//! - dispersive springs: Spring;
//! - early reflections around a short tail: Ambience;
//! - a self-fed diffuser before the FDN: Bloom;
//! - tap bursts: Gated, Reverse.
//!
//! Around the algorithms:
//! - shimmer insert (a pitch shifter feeding the tail back into itself), freeze / infinite;
//! - ducking, swell, era, trails through the VST3 bypass parameter.
//!
//! Deterministic: fixed seeds, control values on a 16-sample grid counted from reset. Zero latency.

#![allow(clippy::needless_range_loop)]

use gfx_dsp::adaa::{Adaa1, Shaper};
use gfx_dsp::delay::{DelayLine, Interp};
use gfx_dsp::envelope::PeakFollower;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::lfo::Rng;
use gfx_dsp::pitch::GrainShifter;
use gfx_dsp::reverb::{Diffuser, Fdn, Plate, Spring, Taps};
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::transport::{Sync, TransportInfo};
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
mod plugin;

const GRID: u32 = 16;
const N_ER: usize = 24;
const N_BURST: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algo {
    Hall,
    Plate,
    Room,
    Chamber,
    Cathedral,
    Ambience,
    Spring,
    Bloom,
    Cloud,
    Gated,
    Reverse,
}

impl Algo {
    pub const ALL: [Algo; 11] = [
        Algo::Hall,
        Algo::Plate,
        Algo::Room,
        Algo::Chamber,
        Algo::Cathedral,
        Algo::Ambience,
        Algo::Spring,
        Algo::Bloom,
        Algo::Cloud,
        Algo::Gated,
        Algo::Reverse,
    ];

    /// FDN delay scale at size 24 m.
    fn scale(self) -> f64 {
        match self {
            Algo::Room => 0.45,
            Algo::Chamber => 0.7,
            Algo::Cathedral => 1.8,
            Algo::Cloud => 3.0,
            Algo::Ambience => 0.3,
            _ => 1.0,
        }
    }

    /// Early reflections: (first, last) in ms at size 24 m, and their level at `early` 50 %.
    fn early(self) -> (f64, f64, f32) {
        match self {
            Algo::Hall => (12.0, 80.0, 0.5),
            Algo::Room => (2.0, 25.0, 0.8),
            Algo::Chamber => (4.0, 40.0, 0.7),
            Algo::Cathedral => (25.0, 150.0, 0.4),
            Algo::Ambience => (1.0, 30.0, 1.0),
            _ => (0.0, 0.0, 0.0),
        }
    }

    /// How many early reflections per side: sparse for big spaces, dense where they are the sound.
    fn er_taps(self) -> usize {
        match self {
            Algo::Hall | Algo::Cathedral => 12,
            Algo::Chamber => 18,
            _ => N_ER,
        }
    }

    /// Modulation at `mod` 50 %: depth (ms) and rate (Hz).
    fn modulation(self) -> (f64, f64) {
        match self {
            Algo::Room | Algo::Chamber => (0.15, 0.4),
            Algo::Cloud => (1.2, 0.3),
            Algo::Plate => (0.5, 0.9),
            _ => (0.5, 0.5),
        }
    }
}

pub static PARAMS: [ParamInfo; 32] = [
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 30.0).with_exp(),
    ParamInfo::float("decay", "Decay", 0.1, 60.0, 1.3).with_exp(),
    ParamInfo::float("size", "Size", 5.0, 60.0, 24.0),
    ParamInfo::float("predelay", "Predelay", 0.0, 250.0, 0.0),
    ParamInfo::float("hicut", "High Cut", 1000.0, 20000.0, 7000.0).with_exp(),
    ParamInfo::enumeration("algo", "Algorithm", 11, 0),
    ParamInfo::float("lowcut", "Low Cut", 20.0, 500.0, 80.0),
    ParamInfo::float("bass_mult", "Bass Decay", 0.5, 2.0, 1.0),
    ParamInfo::float("bass_xover", "Bass Crossover", 100.0, 1000.0, 300.0),
    ParamInfo::float("mod", "Modulation", 0.0, 100.0, 50.0),
    ParamInfo::float("diffusion", "Diffusion", 0.0, 100.0, 80.0),
    ParamInfo::float("early", "Early Reflections", 0.0, 100.0, 50.0),
    ParamInfo::float("width", "Width", 0.0, 150.0, 100.0),
    ParamInfo::enumeration("era", "Era", 3, 0),
    ParamInfo::float("mod_rate", "Modulation Rate", 0.0, 100.0, 50.0),
    ParamInfo::float("high_mult", "High Decay", 0.1, 1.0, 1.0),
    ParamInfo::float("wet_low", "Wet Low Shelf", 0.0, 100.0, 50.0),
    ParamInfo::float("wet_high", "Wet High Shelf", 0.0, 100.0, 50.0),
    ParamInfo::float("swell", "Swell", 0.0, 2000.0, 0.0),
    ParamInfo::float("drive", "Drive", 0.0, 100.0, 0.0),
    ParamInfo::enumeration("springs", "Springs", 3, 1),
    ParamInfo::enumeration("sync", "Sync", 17, 0),
    ParamInfo::float("shimmer", "Shimmer", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("shimmer_pitch", "Shimmer Pitch", -12.0, 24.0, 12.0),
    ParamInfo::float("duck", "Ducking", 0.0, 24.0, 0.0).with_exp(),
    ParamInfo::float("duck_sens", "Duck Sensitivity", 0.0, 100.0, 50.0),
    ParamInfo::float("duck_release", "Duck Release", 50.0, 2000.0, 400.0),
    ParamInfo::enumeration("freeze", "Freeze", 3, 0),
    ParamInfo::boolean("kill_dry", "Kill Dry", false),
    ParamInfo::boolean("bypass", "Bypass", false),
    ParamInfo::boolean("trails", "Trails", true),
    ParamInfo::float("dry", "Dry", -60.0, 0.0, 0.0),
];

pub mod p {
    pub const MIX: usize = 0;
    pub const DECAY: usize = 1;
    pub const SIZE: usize = 2;
    pub const PREDELAY: usize = 3;
    pub const HICUT: usize = 4;
    pub const ALGO: usize = 5;
    pub const LOWCUT: usize = 6;
    pub const BASS_MULT: usize = 7;
    pub const BASS_XOVER: usize = 8;
    pub const MOD: usize = 9;
    pub const DIFFUSION: usize = 10;
    pub const EARLY: usize = 11;
    pub const WIDTH: usize = 12;
    pub const ERA: usize = 13;
    pub const MOD_RATE: usize = 14;
    pub const HIGH_MULT: usize = 15;
    pub const WET_LOW: usize = 16;
    pub const WET_HIGH: usize = 17;
    pub const SWELL: usize = 18;
    pub const DRIVE: usize = 19;
    pub const SPRINGS: usize = 20;
    pub const SYNC: usize = 21;
    pub const SHIMMER: usize = 22;
    pub const SHIMMER_PITCH: usize = 23;
    pub const DUCK: usize = 24;
    pub const DUCK_SENS: usize = 25;
    pub const DUCK_RELEASE: usize = 26;
    pub const FREEZE: usize = 27;
    pub const KILL_DRY: usize = 28;
    pub const BYPASS: usize = 29;
    pub const TRAILS: usize = 30;
    pub const DRY: usize = 31;
    pub const N: usize = 32;
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

pub struct Reverb {
    s: Settings,
    sr: f32,
    algo: Algo,
    pending: bool,
    gate: LinearRamp,
    // input side
    lowcut: [Svf; 2],
    adaa: [Adaa1; 2],
    pre: [DelayLine; 2],
    er: [Taps; 2],
    burst: [Taps; 2],
    swell_g: f32,
    env_fast: f32,
    env_slow: f32,
    // tails
    diff: Diffuser,
    bloom: Diffuser,
    bloom_fb: [f32; 8],
    fdn: Fdn,
    plate: Plate,
    plate_in: f32,
    springs: [Spring; 3],
    // shimmer insert
    shifter: GrainShifter,
    shim_lp: Svf,
    shim_fb: f32,
    shim_amt: OnePole,
    // output side
    out_lp: [Svf; 2],
    era_lp: [Svf; 2],
    low_shelf: [Svf; 2],
    high_shelf: [Svf; 2],
    duck_env: PeakFollower,
    duck_gr: f32,
    // smoothing
    mix: ExpSmoother,
    decay: ExpSmoother,
    hicut: ExpSmoother,
    shimmer: ExpSmoother,
    duck: ExpSmoother,
    size: OnePole,
    predelay: OnePole,
    width: OnePole,
    early: OnePole,
    drive: OnePole,
    dry_g: OnePole,
    in_g: LinearRamp,
    wet_g: LinearRamp,
    freeze_in: OnePole,
    was_on: bool,
    transport: TransportInfo,
    era_rng: Rng,
    /// the tap tables are rebuilt only when what they depend on changes
    taps_key: (u32, i64, i64),
    era: usize,
    pending_era: bool,
    wet_low: OnePole,
    wet_high: OnePole,
    tail_key: [f64; 11],
    filt_key: [f64; 7],
    /// after the tails are cleared their input fades in, or every line would start mid-waveform
    tail_in: LinearRamp,
    ctrl: u32,
    clock: u64,
}

impl Reverb {
    pub fn new() -> Self {
        let s = Settings::default();
        let v = s.v;
        Self {
            s,
            sr: 48000.0,
            algo: Algo::Hall,
            pending: false,
            gate: LinearRamp::new(40.0, 1.0),
            lowcut: Default::default(),
            adaa: Default::default(),
            pre: Default::default(),
            er: Default::default(),
            burst: Default::default(),
            swell_g: 1.0,
            env_fast: 0.0,
            env_slow: 0.0,
            diff: Diffuser::default(),
            bloom: Diffuser::default(),
            bloom_fb: [0.0; 8],
            fdn: Fdn::default(),
            plate: Plate::default(),
            plate_in: 0.0,
            springs: Default::default(),
            shifter: GrainShifter::new(),
            shim_lp: Svf::new(),
            shim_fb: 0.0,
            shim_amt: OnePole::new(20.0, 0.0, 1e-6),
            out_lp: Default::default(),
            era_lp: Default::default(),
            low_shelf: Default::default(),
            high_shelf: Default::default(),
            duck_env: PeakFollower::new(5.0, 400.0),
            duck_gr: 1.0,
            mix: ExpSmoother::new(0.0, 100.0, v[p::MIX]),
            decay: ExpSmoother::new(0.1, 60.0, v[p::DECAY]),
            hicut: ExpSmoother::new(1000.0, 20000.0, v[p::HICUT]),
            shimmer: ExpSmoother::new(0.0, 100.0, v[p::SHIMMER]),
            duck: ExpSmoother::new(0.0, 24.0, v[p::DUCK]),
            size: OnePole::new(400.0, v[p::SIZE], 1e-4),
            predelay: OnePole::new(100.0, v[p::PREDELAY], 1e-3),
            width: OnePole::new(30.0, v[p::WIDTH], 1e-4),
            early: OnePole::new(30.0, v[p::EARLY], 1e-4),
            drive: OnePole::new(30.0, v[p::DRIVE], 1e-4),
            dry_g: OnePole::new(5.0, 1.0, 1e-6),
            in_g: LinearRamp::new(5.0, 1.0),
            wet_g: LinearRamp::new(20.0, 1.0),
            freeze_in: OnePole::new(20.0, 1.0, 1e-6),
            was_on: true,
            transport: TransportInfo::default(),
            era_rng: Rng::new(0xE7A),
            taps_key: (u32::MAX, 0, 0),
            era: 0,
            pending_era: false,
            wet_low: OnePole::new(30.0, 50.0, 1e-3),
            wet_high: OnePole::new(30.0, 50.0, 1e-3),
            tail_key: [f64::NAN; 11],
            filt_key: [f64::NAN; 7],
            tail_in: LinearRamp::new(20.0, 1.0),
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
        for (i, e) in [(p::MIX, &mut self.mix), (p::DECAY, &mut self.decay), (p::HICUT, &mut self.hicut), (p::SHIMMER, &mut self.shimmer), (p::DUCK, &mut self.duck)] {
            if v[i] != old[i] {
                e.set_target(v[i].clamp(PARAMS[i].min, PARAMS[i].max), self.clock);
            }
        }
        self.size.set_target(v[p::SIZE]);
        self.predelay.set_target(v[p::PREDELAY]);
        self.width.set_target(v[p::WIDTH]);
        self.early.set_target(v[p::EARLY]);
        self.drive.set_target(v[p::DRIVE]);
        // the dry level (dB; −60 = silent), killed for a parallel lane
        let dry = if v[p::DRY] <= -60.0 { 0.0 } else { 10f32.powf(v[p::DRY] / 20.0) };
        self.dry_g.set_target(if v[p::KILL_DRY] >= 0.5 { 0.0 } else { dry });
        // freeze: new notes stay out (Freeze) or still enter (Infinite)
        self.freeze_in.set_target(if idx(v[p::FREEZE], 3) == 1 { 0.0 } else { 1.0 });
        if Algo::ALL[idx(v[p::ALGO], 11)] != self.algo {
            self.pending = true;
            self.gate.set_target(0.0);
        }
        if idx(v[p::ERA], 3) != self.era {
            self.pending_era = true;
            self.gate.set_target(0.0);
        }
        self.wet_low.set_target(v[p::WET_LOW]);
        self.wet_high.set_target(v[p::WET_HIGH]);
        self.s = s;
    }

    /// Predelay or burst length as a note value when `sync` is on (samples), else `None`.
    fn synced(&self) -> Option<f64> {
        Sync::from_index(idx(self.s.v[p::SYNC], 17)).beats().map(|b| self.transport.note_samples(b, self.sr))
    }

    fn control(&mut self) {
        let sr = self.sr;
        let v = self.s.v;
        let algo = self.algo;
        let size = self.size.next() as f64;
        let k = size / 24.0;
        let rt = self.decay.value() as f64;
        let rt = if algo == Algo::Ambience { rt.clamp(0.1, 0.6) } else { rt };
        let hicut = self.hicut.value() as f64;
        let frozen = idx(v[p::FREEZE], 3) != 0;
        self.fdn.frozen = frozen;
        self.plate.frozen = frozen;
        // the tails' coefficients cost dozens of powers: recomputed only when what they depend on moves
        let tail_key = [size, rt, hicut, v[p::BASS_MULT] as f64, v[p::HIGH_MULT] as f64, v[p::BASS_XOVER] as f64, v[p::MOD] as f64, v[p::MOD_RATE] as f64, v[p::DIFFUSION] as f64, algo as u8 as f64, self.era as f64];
        if tail_key != self.tail_key {
        self.tail_key = tail_key;
        self.fdn.set_scale(k * algo.scale());
        self.fdn.set_decay(rt * v[p::BASS_MULT] as f64, rt, rt * 0.5 * v[p::HIGH_MULT] as f64, v[p::BASS_XOVER] as f64, hicut);
        self.fdn.frozen = frozen;
        let (md, mr) = algo.modulation();
        let m = v[p::MOD] as f64 / 50.0;
        let era = self.era;
        let era_mod = if era == 2 { 1.5 } else { 1.0 };
        let rate = mr * 2f64.powf((v[p::MOD_RATE] as f64 - 50.0) / 25.0);
        self.fdn.set_mod(md * m * era_mod, rate);
        let diff = v[p::DIFFUSION] as f64 * 0.01;
        self.diff.set_scale(k.clamp(0.3, 2.5) * (if algo == Algo::Cloud { 2.0 } else { 1.0 }) * (0.3 + 0.7 * diff));
        self.bloom.set_scale(k.clamp(0.3, 2.5));
        self.plate.set_scale((size / 15.0).clamp(0.4, 2.0));
        let half = self.plate.half_loop_s();
        self.plate.decay = 10f64.powf(-1.5 * half / rt.max(0.05)) as f32;
        self.plate.kd = (-TAU_F * (hicut.min(0.45 * sr as f64) as f32) / sr).exp();
        self.plate.kb = 1.0 - (-TAU_F * v[p::BASS_XOVER] / sr).exp();
        self.plate.bass_g = (10f64.powf(-1.5 * half / (rt * v[p::BASS_MULT] as f64)) / self.plate.decay as f64) as f32;
        self.plate.excursion = 16.0 * m;
        self.plate.rate = rate * 1.8;
        self.plate.density = (0.4 + 0.6 * diff) as f32;
        }
        let spring_len = (0.02 + 0.0012 * size) * sr as f64;
        for (j, sp) in self.springs.iter_mut().enumerate() {
            sp.set_len(spring_len * (1.0 + 0.13 * j as f64));
        }
        // early reflections and bursts: rebuilt only when the algorithm, the size or the burst length moves
        let len = self.synced().unwrap_or((0.05 + 0.55 * ((size - 5.0) / 55.0)) * sr as f64).min(2.0 * sr as f64 - 8.0);
        let key = (algo as u32, (size * 100.0) as i64, len as i64);
        let rebuild = key != self.taps_key;
        for t in self.er.iter_mut().chain(self.burst.iter_mut()) {
            t.tick(0.02);
        }
        self.taps_key = key;
        let (t0, t1, _) = algo.early();
        let mut rng = Rng::new(0xEA51 + algo as u64);
        if rebuild {
        let n_er = algo.er_taps();
        // fewer taps, the same energy
        let norm = (N_ER as f32 / n_er as f32).sqrt();
        for c in 0..2 {
            for i in 0..N_ER {
                if i >= n_er {
                    self.er[c].set(i, 1.0, 0.0);
                    continue;
                }
                let t = t0 + (t1 - t0) * ((i as f64 + rng.next_f64()) / n_er as f64).powf(1.3);
                let g = (-(t - t0) / (t1 - t0 + 1e-3) * 2.5).exp() as f32 * if rng.next_u64() & 1 == 1 { 1.0 } else { -1.0 };
                self.er[c].set(i, t * k * 0.001 * sr as f64, g * 0.35 * norm);
            }
        }
        // bursts: flat (Gated) or rising (Reverse), over the size or the synced note
        for c in 0..2 {
            for i in 0..N_BURST {
                let t = (i as f64 + 0.5 * c as f64 + 0.5 * rng.next_f64()) / N_BURST as f64;
                // Gated: flat, then cut on its length with a 3 ms fade (crisp, click-free)
                let fade = (0.003 * sr as f64 / len).min(0.5);
                let g = match algo {
                    Algo::Reverse => t as f32,
                    _ => ((1.0 - t) / fade).clamp(0.0, 1.0) as f32,
                };
                self.burst[c].set(i, t * len, g * 0.25);
            }
        }
        }
        // output filters: also only when their inputs move
        let (wl, wh) = (self.wet_low.next(), self.wet_high.next());
        let era = self.era;
        let filt_key = [hicut, era as f64, wl as f64, wh as f64, v[p::LOWCUT] as f64, v[p::SHIMMER_PITCH] as f64, v[p::DUCK_RELEASE] as f64];
        if filt_key == self.filt_key {
            return;
        }
        self.filt_key = filt_key;
        let lp = (1.5 * hicut).min(0.45 * sr as f64) as f32;
        let era_f = match era {
            1 => 12000.0f32,
            2 => 7000.0,
            _ => 0.45 * sr,
        };
        for c in 0..2 {
            self.out_lp[c].set(SvfMode::LowPass, lp, 0.5, 0.0, sr);
            self.era_lp[c].set(SvfMode::LowPass, era_f.min(0.45 * sr), std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            self.low_shelf[c].set(SvfMode::LowShelf, 200.0, 0.7, 24.0 * (wl - 50.0) / 50.0 * 0.5, sr);
            self.high_shelf[c].set(SvfMode::HighShelf, 4000.0, 0.7, 24.0 * (wh - 50.0) / 50.0 * 0.5, sr);
            self.lowcut[c].set(SvfMode::HighPass, v[p::LOWCUT], std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
        }
        let ratio = (v[p::SHIMMER_PITCH] as f64 / 12.0).exp2();
        self.shim_lp.set(SvfMode::LowPass, ((0.45 * sr as f64 / ratio.max(1.0)) as f32).min(12000.0), std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
        self.shifter.set_ratio(ratio);
        self.duck_env.set_times(5.0, v[p::DUCK_RELEASE].clamp(50.0, 2000.0), sr);
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
        let freeze = idx(v[p::FREEZE], 3);
        let swell_ms = v[p::SWELL];
        let duck_th = -60.0 + 60.0 * (1.0 - v[p::DUCK_SENS] * 0.01);
        let n_springs = idx(v[p::SPRINGS], 3) + 1;
        for i in 0..n {
            self.clock += 1;
            if self.ctrl == 0 {
                self.control();
            }
            self.ctrl = (self.ctrl + 1) % GRID;
            let g = self.gate.next();
            if g == 0.0 && self.gate.is_settled() {
                if self.pending {
                    self.algo = Algo::ALL[idx(self.s.v[p::ALGO], 11)];
                    self.pending = false;
                    self.clear_tails();
                    // at the bottom of the dip the new algorithm's lengths and taps jump into place: gliding
                    // there would sweep the read heads (a chirp)
                    self.control();
                    self.diff.settle();
                    self.bloom.settle();
                    self.fdn.settle();
                    for t in self.er.iter_mut().chain(self.burst.iter_mut()) {
                        t.settle();
                    }
                }
                if self.pending_era {
                    self.era = idx(self.s.v[p::ERA], 3);
                    self.pending_era = false;
                }
                self.gate.set_target(1.0);
            }
            let gs = 0.5 - 0.5 * (std::f32::consts::PI * g).cos();
            let mix = self.mix.next() * 0.01;
            self.decay.next();
            self.hicut.next();
            let shimmer = self.shimmer.next() * 0.01;
            let duck_db = self.duck.next();
            let pre_ms = self.predelay.next();
            let width = self.width.next() * 0.01;
            let early_g = self.early.next() / 50.0;
            let drive = self.drive.next() * 0.01;
            let dry_g = self.dry_g.next();
            let in_g = self.in_g.next() * self.freeze_in.next();
            let wet_g = self.wet_g.next();
            // the shimmer leaves the loop in freeze, or a frozen tail would climb forever
            self.shim_amt.set_target(if freeze == 0 && shimmer > 0.0 { shimmer.min(1.0) } else { 0.0 });
            let shim = self.shim_amt.next();
            if wet_g == 0.0 && self.wet_g.is_settled() && !on && !trails {
                self.clear_tails();
                self.clear_inputs();
            }

            let x0 = buffers[0][i];
            let x1 = if nch > 1 { buffers[1][i] } else { x0 };
            // ducking detector on the dry (idle when ducking is off: the gain just returns to 1)
            let dt = if duck_db > 0.0 {
                let e = self.duck_env.next(x0.abs().max(x1.abs()));
                let edb = 20.0 * e.max(1e-6).log10();
                let over = ((edb - duck_th) / 6.0).clamp(0.0, 1.0);
                10f32.powf(-duck_db * over / 20.0)
            } else {
                1.0
            };
            self.duck_gr = dt + (self.duck_gr - dt) * 0.999;
            // swell: a pick attack restarts the swell from silence
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
            // input: low cut, drive (dwell on Spring), predelay
            let pre_s = self.synced().filter(|_| !matches!(self.algo, Algo::Gated | Algo::Reverse)).unwrap_or(pre_ms as f64 * 0.001 * sr as f64);
            let mut xin = [0.0f32; 2];
            for (c, xv) in [x0, x1].into_iter().enumerate() {
                let mut u = self.lowcut[c].process(xv * in_g * sw);
                if drive > 0.0 {
                    let kd = 1.0 + 6.0 * drive;
                    let s = self.adaa[c].process(Shaper::Tanh, u * kd) / kd;
                    u += (s - u) * (5.0 * drive).min(1.0);
                }
                self.pre[c].push(u);
                xin[c] = self.pre[c].read(pre_s.max(0.0), Interp::Linear);
                self.er[c].line.push(xin[c]);
                self.burst[c].line.push(xin[c]);
            }
            // era: early converters (fewer bits into the tail)
            let era = self.era;
            if era > 0 {
                let q = if era == 1 { 2f32.powi(-12) } else { 2f32.powi(-10) };
                for xv in xin.iter_mut() {
                    *xv = (*xv / q + 0.5 * self.era_rng.bipolar()).round() * q;
                }
            }
            // the tail's input: the dry, plus the shimmer regeneration
            let ti = self.tail_in.next();
            let feed = [(xin[0] + self.shim_fb) * ti, (xin[1] + self.shim_fb) * ti];
            let mut d8 = [feed[0], feed[1], -feed[0], -feed[1], feed[0], -feed[1], feed[1], -feed[0]];
            let (mut tl, mut tr) = (0.0f32, 0.0f32);
            let (mut el, mut er) = (0.0f32, 0.0f32);
            match self.algo {
                Algo::Plate => {
                    let (l, r) = self.plate.process(0.5 * (feed[0] + feed[1]), &mut self.plate_in);
                    tl = l;
                    tr = r;
                }
                Algo::Spring => {
                    let coef = -0.62f32;
                    let stages = 8 + (v[p::DIFFUSION] * 0.3) as usize;
                    let rt = self.decay.value().max(0.1) as f64;
                    let klp = 1.0 - (-TAU_F * (self.hicut.value() * 0.7).min(0.45 * sr) / sr).exp();
                    let m = 0.5 * (feed[0] + feed[1]);
                    for j in 0..n_springs {
                        let len = (0.02 + 0.0012 * self.size.value() as f64) * sr as f64 * (1.0 + 0.13 * j as f64);
                        let fb = if freeze != 0 { 0.999 } else { 10f64.powf(-3.0 * len / (rt * sr as f64)) as f32 };
                        let y = self.springs[j].process(m, coef, stages.min(40), fb, klp);
                        if j % 2 == 0 {
                            tl += y;
                        } else {
                            tr += y;
                        }
                    }
                    if n_springs == 1 {
                        tr = tl;
                    } else if n_springs == 3 {
                        tr += 0.5 * tl;
                    }
                }
                Algo::Gated | Algo::Reverse => {
                    self.diff.process(&mut d8);
                    let dd = v[p::DIFFUSION] * 0.01;
                    for c in 0..2 {
                        // the burst reads the predelayed input; diffusion blends in the diffused grain
                        let b = self.burst[c].read(0, 1);
                        let y = b + (d8[c] * 0.5 - b) * dd * 0.3;
                        if c == 0 {
                            tl = y;
                        } else {
                            tr = y;
                        }
                    }
                }
                _ => {
                    self.diff.process(&mut d8);
                    if self.algo == Algo::Bloom {
                        // the diffuser fed back on itself: the tail fades in, then the FDN decays it
                        let tb = 0.2 + 1.8 * ((self.size.value() - 5.0) / 55.0) as f64;
                        let gb = 10f64.powf(-3.0 * 0.06 / tb) as f32;
                        let mut b = [0.0f32; 8];
                        for c in 0..8 {
                            b[c] = d8[c] * 0.3 + gb * self.bloom_fb[c];
                        }
                        self.bloom.process(&mut b);
                        self.bloom_fb = b;
                        d8 = b;
                    }
                    let mut in16 = [0.0f32; 16];
                    for c in 0..16 {
                        in16[c] = if c < 8 { d8[c] } else { -d8[c - 8] } * 0.5;
                    }
                    let (l, r) = self.fdn.process(&in16);
                    tl = l;
                    tr = r;
                }
            }
            // early reflections
            let (_, _, er_level) = self.algo.early();
            if er_level > 0.0 {
                el = self.er[0].read(0, 1) * er_level * early_g;
                er = self.er[1].read(0, 1) * er_level * early_g;
            }
            // shimmer regeneration: the tail through a low-pass and the shifter, back into the input
            if shim > 0.0 {
                // the tail's sustained gain grows with its length: the feed is scaled down by it, and a
                // soft limit keeps the regeneration bounded whatever the settings
                let m = 0.5 * (tl + tr);
                let s = self.shifter.process(self.shim_lp.process(m));
                let a = 0.95 * shim / (1.0 + self.decay.value()).sqrt();
                self.shim_fb = (a * s).tanh();
            } else {
                self.shim_fb = 0.0;
            }
            let tail_g = if self.algo == Algo::Ambience { 0.35 } else { 1.0 };
            let shelves = self.wet_low.value() != 50.0 || self.wet_high.value() != 50.0;
            let mut wet = [el + tl * tail_g, er + tr * tail_g];
            for c in 0..2 {
                let mut w = self.out_lp[c].process(wet[c]);
                if era > 0 {
                    w = self.era_lp[c].process(w);
                }
                // the wet shelves are skipped while both sit exactly flat (50 %)
                wet[c] = if shelves { self.high_shelf[c].process(self.low_shelf[c].process(w)) } else { w };
            }
            let mid = 0.5 * (wet[0] + wet[1]);
            let side = 0.5 * (wet[0] - wet[1]) * width;
            let kw = mix * self.duck_gr * wet_g * gs;
            let (ol, or) = (x0 * dry_g + (mid + side) * kw, x1 * dry_g + (mid - side) * kw);
            if nch > 1 {
                buffers[0][i] = ol;
                buffers[1][i] = or;
            } else {
                buffers[0][i] = 0.5 * (ol + or);
            }
        }
    }

    fn clear_tails(&mut self) {
        self.tail_in.set_immediate(0.0);
        self.tail_in.set_target(1.0);
        self.bloom.reset();
        self.bloom_fb = [0.0; 8];
        self.fdn.reset();
        self.plate.reset();
        self.plate_in = 0.0;
        for s in self.springs.iter_mut() {
            s.reset();
        }
        self.shifter.reset();
        self.shim_lp.reset();
        self.shim_fb = 0.0;
    }

    /// The input-side lines (early reflections, bursts): only on reset and when trails are off. An
    /// algorithm change keeps them, since they hold the input, not the tail.
    fn clear_inputs(&mut self) {
        // the diffuser too: it only smears the input (a fresh one restarts sparse, a flam on the switch)
        self.diff.reset();
        for t in self.er.iter_mut().chain(self.burst.iter_mut()) {
            t.reset();
        }
    }
}

const TAU_F: f32 = std::f32::consts::TAU;

impl Default for Reverb {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Reverb {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        for e in [&mut self.mix, &mut self.decay, &mut self.hicut, &mut self.shimmer, &mut self.duck] {
            e.prepare(sample_rate);
        }
        self.size.prepare(sample_rate / GRID as f32);
        self.wet_low.prepare(sample_rate / GRID as f32);
        self.wet_high.prepare(sample_rate / GRID as f32);
        for o in [&mut self.predelay, &mut self.width, &mut self.early, &mut self.drive, &mut self.dry_g, &mut self.freeze_in, &mut self.shim_amt] {
            o.prepare(sample_rate);
        }
        for r in [&mut self.in_g, &mut self.wet_g, &mut self.gate, &mut self.tail_in] {
            r.prepare(sample_rate);
        }
        self.duck_env.prepare(sample_rate);
        let sr = sample_rate as f64;
        for c in 0..2 {
            self.pre[c].prepare((2.1 * sr) as usize);
            self.er[c].prepare((0.6 * sr) as usize, N_ER);
            self.burst[c].prepare((2.1 * sr) as usize, N_BURST);
        }
        self.diff.prepare(sample_rate, 4, 40.0, 0xD1FF);
        self.bloom.prepare(sample_rate, 4, 60.0, 0xB100);
        // the largest FDN: Cloud at 60 m
        self.fdn.prepare(sample_rate, 3.0 * 60.0 / 24.0 + 0.1);
        self.plate.prepare(sample_rate, 2.1);
        for (j, s) in self.springs.iter_mut().enumerate() {
            s.prepare(sample_rate, 40, 0x5B + j as u64);
        }
        self.shifter.prepare(sample_rate, 80.0);
    }

    fn reset(&mut self) {
        let v = self.s.v;
        for (i, e) in [(p::MIX, &mut self.mix), (p::DECAY, &mut self.decay), (p::HICUT, &mut self.hicut), (p::SHIMMER, &mut self.shimmer), (p::DUCK, &mut self.duck)] {
            e.settle(v[i].clamp(PARAMS[i].min, PARAMS[i].max));
        }
        for o in [&mut self.size, &mut self.predelay, &mut self.width, &mut self.early, &mut self.drive, &mut self.dry_g, &mut self.freeze_in] {
            o.settle();
        }
        self.shim_amt.set_immediate(if idx(v[p::FREEZE], 3) == 0 { (v[p::SHIMMER] * 0.01).min(1.0) } else { 0.0 });
        self.algo = Algo::ALL[idx(v[p::ALGO], 11)];
        self.pending = false;
        self.era = idx(v[p::ERA], 3);
        self.pending_era = false;
        self.taps_key = (u32::MAX, 0, 0);
        self.tail_key = [f64::NAN; 11];
        self.filt_key = [f64::NAN; 7];
        self.wet_low.set_target(v[p::WET_LOW]);
        self.wet_high.set_target(v[p::WET_HIGH]);
        self.wet_low.settle();
        self.wet_high.settle();
        self.gate.set_immediate(1.0);
        let on = v[p::BYPASS] < 0.5;
        self.was_on = on;
        self.in_g.set_immediate(if on { 1.0 } else { 0.0 });
        self.wet_g.set_immediate(if on || v[p::TRAILS] >= 0.5 { 1.0 } else { 0.0 });
        for c in 0..2 {
            self.pre[c].reset();
            self.lowcut[c].reset();
            self.adaa[c].reset(Shaper::Tanh);
            for f in [&mut self.out_lp[c], &mut self.era_lp[c], &mut self.low_shelf[c], &mut self.high_shelf[c]] {
                f.reset();
            }
        }
        self.clear_tails();
        self.clear_inputs();
        self.tail_in.set_immediate(1.0);
        // the lengths and taps start where the settings put them (no glide from a previous state)
        self.taps_key = (u32::MAX, 0, 0);
        self.control();
        self.diff.settle();
        self.bloom.settle();
        self.fdn.settle();
        for t in self.er.iter_mut().chain(self.burst.iter_mut()) {
            t.settle();
        }
        self.swell_g = 1.0;
        self.env_fast = 0.0;
        self.env_slow = 0.0;
        self.duck_env.reset();
        self.duck_gr = 1.0;
        self.era_rng.reset();
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

    fn set_transport(&mut self, transport: TransportInfo) {
        self.transport = transport;
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_block(buffers);
    }
}

#[cfg(test)]
mod tests;
