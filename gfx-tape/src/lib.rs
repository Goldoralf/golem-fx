// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-tape — golem's tape and lo-fi media pedal (design: README.md, its Review decides).
//!
//! One chain for every medium (README §2):
//!
//! ```text
//! in ─┬────────────────────────── dry: D0 delay + the same half-band pairs ─────────────────┐
//!     └▶ [ALC] ▶ × input ▶ [NR encode] ▶ ↑8× ▶ record shelf ▶ J-A (bias) ▶ inverse shelf ▶ ↓8×
//!        ▶ DC 10 Hz ▶ read head D = D0 − ∫ε + stop lag ▶ × (1 + modulation noise)
//!        ▶ + hiss, mechanical, crackle (Follow / Constant / Duck) ▶ × dropout dip
//!        ▶ spacing (static + wear + dropouts + crinkle + azimuth + bias erasure) ▶ bias HF sensitivity
//!        ▶ band edges, head bump ▶ [NR decode] ▶ low / high cut ▶ × compensation × level ▶ mix ┘
//! ```
//!
//! - The core runs at ≥ 352.8 kHz (8× at 44.1 / 48 kHz, 4× at 88.2 / 96, 2× at 176.4 / 192) through
//!   gfx-dsp `oversample`'s minimum-phase half-bands; gfx-dsp `tape::TapeSat` (record shelf, J-A along
//!   H, inverse shelf, makeup) on tape models, the disc core (RIAA, tracing, cutter) on Vinyl / Shellac.
//! - The transport is generated as speed at the control rate (fs / 16) and read with Lagrange-5;
//!   the dry follows D0. With `wow`, `flutter`, `snags` at 0 (and no azimuth in stereo, no stop) the
//!   line is bypassed: exact zero delay.
//! - A change of `model`, `speed`, `formula`, `nr` or `alc` dips the wet for 30 ms.
//! - A mono input on 2 / 2 with `spread` and `azimuth` at 0 computes once and copies L to R.
//! - Reported latency 0. Real-time safe, deterministic (seeded, phase 0 on reset).

#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

pub mod medium;
pub mod models;
pub mod transport;

#[cfg(feature = "plugin")]
mod plugin;

#[cfg(test)]
mod tests;

use gfx_dsp::delay::{DelayLine, Interp};
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::oversample::Oversampler2x;
use gfx_dsp::smooth::OnePole;
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::tape::{level_loss_db, record_shelf, Shelf1, SpacingFilter, TapeSat};
use gfx_dsp::transport::{Sync, TransportInfo};
use gfx_dsp::{Effect, ParamInfo};
use medium::{disc_loss_db, Alc, Compander, Crackle, Disc, Lane, NoiseLaw, TrackNoise, DISC_REF};
use models::{Kind, Model, MODELS};
use std::f64::consts::PI;
use transport::{Knobs, Stage, Stop, Transport};

pub const NP: usize = 32;

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::float("input", "Record", -12.0, 24.0, 6.0).with_exp(),
    ParamInfo::float("wow", "Wow", 0.0, 4.0, 0.1).with_exp(),
    ParamInfo::float("flutter", "Flutter", 0.0, 1.0, 0.05).with_exp(),
    ParamInfo::float("wear", "Wear", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::enumeration("model", "Model", 8, 0),
    ParamInfo::enumeration("speed", "Speed", 6, 0),
    ParamInfo::boolean("noise", "Noise", true),
    ParamInfo::enumeration("formula", "Formula", 4, 0),
    ParamInfo::float("bias", "Bias", 50.0, 150.0, 100.0).with_exp(),
    ParamInfo::float("head_bump", "Head Bump", 0.0, 200.0, 100.0),
    ParamInfo::float("spacing", "Spacing", 0.0, 20.0, 0.0).with_exp(),
    ParamInfo::float("azimuth", "Azimuth", -30.0, 30.0, 0.0),
    ParamInfo::enumeration("alc", "ALC", 3, 0),
    ParamInfo::enumeration("nr", "Noise Reduction", 2, 0),
    ParamInfo::float("wow_rate", "Wow Speed", 25.0, 400.0, 100.0).with_exp(),
    ParamInfo::float("wow_rand", "Wow Randomness", -100.0, 100.0, 0.0),
    ParamInfo::float("snags", "Snags", 0.0, 100.0, 0.0),
    ParamInfo::float("dropouts", "Dropouts", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::enumeration("drop_sync", "Dropout Sync", 17, 0),
    ParamInfo::float("crinkle", "Crinkle", 0.0, 100.0, 0.0),
    ParamInfo::float("crackle", "Crackle", 0.0, 100.0, 0.0),
    ParamInfo::enumeration("noise_mode", "Noise Mode", 3, 0),
    ParamInfo::float("hiss", "Hiss", -40.0, 20.0, 0.0),
    ParamInfo::float("mech", "Mechanical", -40.0, 20.0, 0.0),
    ParamInfo::boolean("stop", "Stop", false),
    ParamInfo::float("stop_time", "Stop Time", 50.0, 3000.0, 600.0),
    ParamInfo::float("spread", "Spread", 0.0, 100.0, 0.0),
    ParamInfo::float("low_cut", "Low Cut", 20.0, 1000.0, 20.0),
    ParamInfo::float("high_cut", "High Cut", 1000.0, 20000.0, 20000.0).with_exp(),
    ParamInfo::float("level", "Level", -24.0, 12.0, 0.0).with_exp(),
    ParamInfo::boolean("bypass", "Bypass", false),
];

pub mod p {
    pub const INPUT: usize = 0;
    pub const WOW: usize = 1;
    pub const FLUTTER: usize = 2;
    pub const WEAR: usize = 3;
    pub const MIX: usize = 4;
    pub const MODEL: usize = 5;
    pub const SPEED: usize = 6;
    pub const NOISE: usize = 7;
    pub const FORMULA: usize = 8;
    pub const BIAS: usize = 9;
    pub const HEAD_BUMP: usize = 10;
    pub const SPACING: usize = 11;
    pub const AZIMUTH: usize = 12;
    pub const ALC: usize = 13;
    pub const NR: usize = 14;
    pub const WOW_RATE: usize = 15;
    pub const WOW_RAND: usize = 16;
    pub const SNAGS: usize = 17;
    pub const DROPOUTS: usize = 18;
    pub const DROP_SYNC: usize = 19;
    pub const CRINKLE: usize = 20;
    pub const CRACKLE: usize = 21;
    pub const NOISE_MODE: usize = 22;
    pub const HISS: usize = 23;
    pub const MECH: usize = 24;
    pub const STOP: usize = 25;
    pub const STOP_TIME: usize = 26;
    pub const SPREAD: usize = 27;
    pub const LOW_CUT: usize = 28;
    pub const HIGH_CUT: usize = 29;
    pub const LEVEL: usize = 30;
    pub const BYPASS: usize = 31;
}

const MAX_CH: usize = 2;
/// Control rate divider (the transport, coefficient updates).
pub const CTL: u32 = 16;
/// Shortest read delay (Lagrange-5 needs 2 samples of history ahead of the point).
pub const D_MIN: f64 = 3.0;
/// Longest transport centre delay.
pub const D0_MAX_S: f64 = 0.040;
/// D0 slews: up 20 ms/s (≤ 2 % pitch sag), down 1 ms/s (0.1 %).
pub const D0_UP: f64 = 0.020;
pub const D0_DOWN: f64 = 0.001;
/// The stop buffer (s).
pub const STOP_BUF_S: f64 = 3.5;
/// Wet dip on a structural change (each half).
pub const DIP_S: f64 = 0.015;
/// Leak of the transport integral (Hz): bounds the drift's random walk.
pub const LEAK_HZ: f64 = 0.02;

/// What a dip is needed to change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Config {
    model: usize,
    speed: usize,
    formula: usize,
    nr: bool,
    alc: bool,
}

/// One channel's signal state (everything but its noise generators: copied when a linked pair
/// splits).
#[derive(Clone)]
struct Chan {
    comp: Compander,
    ups: [Oversampler2x; 3],
    downs: [Oversampler2x; 3],
    sat: TapeSat,
    disc: Disc,
    dc: Svf,
    line: DelayLine,
    integ: f64,
    spacing: SpacingFilter,
    sens: Svf,
    lf: Svf,
    contour: Svf,
    bump: Svf,
    dip: Svf,
    hf: [Svf; 2],
    lc: Svf,
    hc: [Svf; 2],
    dry: DelayLine,
    /// the dry's own cascade: the wet's half-bands, each at its own rate, nothing between
    dry_up: [Oversampler2x; 3],
    dry_dn: [Oversampler2x; 3],
}

impl Chan {
    fn new() -> Self {
        Self {
            comp: Compander::new(),
            ups: [Oversampler2x::new(), Oversampler2x::new(), Oversampler2x::new()],
            downs: [Oversampler2x::new(), Oversampler2x::new(), Oversampler2x::new()],
            sat: TapeSat::new(),
            disc: Disc::default(),
            dc: Svf::new(),
            line: DelayLine::new(),
            integ: 0.0,
            spacing: SpacingFilter::new(),
            sens: Svf::new(),
            lf: Svf::new(),
            contour: Svf::new(),
            bump: Svf::new(),
            dip: Svf::new(),
            hf: [Svf::new(), Svf::new()],
            lc: Svf::new(),
            hc: [Svf::new(), Svf::new()],
            dry: DelayLine::new(),
            dry_up: [Oversampler2x::new(), Oversampler2x::new(), Oversampler2x::new()],
            dry_dn: [Oversampler2x::new(), Oversampler2x::new(), Oversampler2x::new()],
        }
    }

    fn reset(&mut self) {
        self.comp.reset();
        for o in self.ups.iter_mut().chain(self.downs.iter_mut()).chain(self.dry_up.iter_mut()).chain(self.dry_dn.iter_mut()) {
            o.reset();
        }
        self.sat.reset();
        self.disc.reset();
        self.dc.reset();
        self.line.reset();
        self.integ = 0.0;
        self.spacing.reset();
        for f in [&mut self.sens, &mut self.lf, &mut self.contour, &mut self.bump, &mut self.dip, &mut self.lc] {
            f.reset();
        }
        for f in self.hf.iter_mut().chain(self.hc.iter_mut()) {
            f.reset();
        }
        self.dry.reset();
    }

    /// Becomes a copy of `o` without allocating.
    fn sync_from(&mut self, o: &Chan) {
        self.comp = o.comp.clone();
        for k in 0..3 {
            self.ups[k].copy_from(&o.ups[k]);
            self.downs[k].copy_from(&o.downs[k]);
            self.dry_up[k].copy_from(&o.dry_up[k]);
            self.dry_dn[k].copy_from(&o.dry_dn[k]);
        }
        self.sat = o.sat.clone();
        self.disc = o.disc.clone();
        self.dc = o.dc.clone();
        self.line.copy_from(&o.line);
        self.integ = o.integ;
        self.spacing = o.spacing.clone();
        self.sens = o.sens.clone();
        self.lf = o.lf.clone();
        self.contour = o.contour.clone();
        self.bump = o.bump.clone();
        self.dip = o.dip.clone();
        self.hf = o.hf.clone();
        self.lc = o.lc.clone();
        self.hc = o.hc.clone();
        self.dry.copy_from(&o.dry);
    }
}

/// The current configuration's derived values.
#[derive(Clone, Copy, Debug)]
struct Derived {
    kind: Kind,
    /// tape speed (m/s) and the ratio to the model's own
    v: f64,
    rv: f64,
    h_ref_db: f64,
    boost_db: f64,
    /// record shelf gain at 1 kHz (dB): the reference sine meets the tape that much hotter
    shelf_1k_db: f64,
    hiss_norm: f64,
    lf_edge: Option<f64>,
    hf_edge: Option<f64>,
    bump_hz: Option<f64>,
    alc: bool,
    nr: bool,
}

/// Per-sample constants, recomputed only while a control moves.
#[derive(Clone, Copy, Debug, Default)]
struct Cache {
    hiss: f64,
    mech: f64,
    modn: f64,
    d_static: f64,
    v_play: f64,
    az_off: f64,
}

pub struct Tape {
    v: [f32; NP],
    cache: Cache,
    sr: f64,
    nch: usize,
    stages: usize,
    ch: Vec<Chan>,
    tn: Vec<TrackNoise>,
    ch1_stale: bool,
    transport: Transport,
    stop: Stop,
    lane_a: Lane,
    lane_b: Lane,
    crackle: Crackle,
    law: NoiseLaw,
    alc: Alc,
    host: TransportInfo,
    now: u64,
    ctl: u32,
    eps_prev: [f64; 2],
    eps_cur: [f64; 2],
    d0: f64,
    d0_target: f64,
    /// the line's bypass blend: 1 = direct (no transport), 0 = read head
    byp_line: f64,
    line_on: bool,
    grid_phase: f64,
    free_phase: f64,
    noise_fade: f64,
    cfg: Config,
    der: Derived,
    dip_gain: f64,
    dipping: bool,
    in_g: f64,
    out_g: f64,
    level_g: f64,
    // EXP-capable
    input: ExpSmoother,
    wow: ExpSmoother,
    flutter: ExpSmoother,
    wear: ExpSmoother,
    mix: ExpSmoother,
    bias: ExpSmoother,
    spacing: ExpSmoother,
    wow_rate: ExpSmoother,
    dropouts: ExpSmoother,
    high_cut: ExpSmoother,
    level: ExpSmoother,
    // the others
    head_bump: OnePole,
    azimuth: OnePole,
    wow_rand: OnePole,
    snags: OnePole,
    crinkle: OnePole,
    crackle_amt: OnePole,
    hiss: OnePole,
    mech: OnePole,
    spread: OnePole,
    low_cut: OnePole,
    byp: OnePole,
    designed: [f32; 6],
    dry_on: bool,
    wf: f32,
    /// low / high cut engaged (crossfaded)
    cut_w: [f32; 2],
    /// Tests only: the modulation noise off (it is part of the medium, never switched in use).
    pub no_mod_noise: bool,
}

impl Default for Tape {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn db(x: f64) -> f64 {
    10f64.powf(x / 20.0)
}

impl Tape {
    pub fn new() -> Self {
        let mut v = [0.0; NP];
        for (k, pi) in PARAMS.iter().enumerate() {
            v[k] = pi.default;
        }
        let exp = |i: usize| ExpSmoother::new(PARAMS[i].min, PARAMS[i].max, PARAMS[i].default);
        let pole = |i: usize, ms: f32| OnePole::new(ms, PARAMS[i].default, 1e-4);
        let cfg = Config { model: 0, speed: 0, formula: 0, nr: false, alc: false };
        Self {
            v,
            cache: Cache::default(),
            sr: 48000.0,
            nch: 1,
            stages: 3,
            ch: Vec::new(),
            tn: Vec::new(),
            ch1_stale: false,
            transport: Transport::new(),
            stop: Stop::default(),
            lane_a: Lane::new(501),
            lane_b: Lane::new(601),
            crackle: Crackle::new(701),
            law: NoiseLaw::new(),
            alc: Alc::default(),
            host: TransportInfo::default(),
            now: 0,
            ctl: 0,
            eps_prev: [0.0; 2],
            eps_cur: [0.0; 2],
            d0: 0.0,
            d0_target: 0.0,
            byp_line: 1.0,
            line_on: false,
            grid_phase: 0.0,
            free_phase: 0.0,
            noise_fade: 1.0,
            cfg,
            der: Derived { kind: Kind::Tape, v: 0.381, rv: 1.0, h_ref_db: -10.5, boost_db: 3.5, shelf_1k_db: 0.0, hiss_norm: 1.0, lf_edge: None, hf_edge: None, bump_hz: None, alc: false, nr: false },
            dip_gain: 1.0,
            dipping: false,
            in_g: 1.0,
            out_g: 1.0,
            level_g: 1.0,
            input: exp(p::INPUT),
            wow: exp(p::WOW),
            flutter: exp(p::FLUTTER),
            wear: exp(p::WEAR),
            mix: exp(p::MIX),
            bias: exp(p::BIAS),
            spacing: exp(p::SPACING),
            wow_rate: exp(p::WOW_RATE),
            dropouts: exp(p::DROPOUTS),
            high_cut: exp(p::HIGH_CUT),
            level: exp(p::LEVEL),
            head_bump: pole(p::HEAD_BUMP, 50.0),
            azimuth: pole(p::AZIMUTH, 50.0),
            wow_rand: pole(p::WOW_RAND, 200.0),
            snags: pole(p::SNAGS, 50.0),
            crinkle: pole(p::CRINKLE, 50.0),
            crackle_amt: pole(p::CRACKLE, 50.0),
            hiss: pole(p::HISS, 50.0),
            mech: pole(p::MECH, 50.0),
            spread: pole(p::SPREAD, 200.0),
            low_cut: OnePole::new(30.0, 20.0, 1e-2),
            byp: OnePole::new(10.0 / 3.0, 0.0, 1e-5),
            designed: [f32::NAN; 6],
            dry_on: false,
            wf: 0.0,
            cut_w: [0.0; 2],
            no_mod_noise: false,
        }
    }

    fn exps(&mut self) -> [(&mut ExpSmoother, usize); 11] {
        [
            (&mut self.input, p::INPUT),
            (&mut self.wow, p::WOW),
            (&mut self.flutter, p::FLUTTER),
            (&mut self.wear, p::WEAR),
            (&mut self.mix, p::MIX),
            (&mut self.bias, p::BIAS),
            (&mut self.spacing, p::SPACING),
            (&mut self.wow_rate, p::WOW_RATE),
            (&mut self.dropouts, p::DROPOUTS),
            (&mut self.high_cut, p::HIGH_CUT),
            (&mut self.level, p::LEVEL),
        ]
    }

    fn poles(&mut self) -> [(&mut OnePole, usize); 11] {
        [
            (&mut self.head_bump, p::HEAD_BUMP),
            (&mut self.azimuth, p::AZIMUTH),
            (&mut self.wow_rand, p::WOW_RAND),
            (&mut self.snags, p::SNAGS),
            (&mut self.crinkle, p::CRINKLE),
            (&mut self.crackle_amt, p::CRACKLE),
            (&mut self.hiss, p::HISS),
            (&mut self.mech, p::MECH),
            (&mut self.spread, p::SPREAD),
            (&mut self.low_cut, p::LOW_CUT),
            (&mut self.byp, p::BYPASS),
        ]
    }

    /// The weighted W&F of the current transport (%, WRMS over 2 s) — the `wf_out` meter.
    pub fn wf_out(&self) -> f32 {
        self.wf
    }

    /// The transport's centre delay now (samples): what a parallel lane runs late by.
    pub fn d0_samples(&self) -> f64 {
        if self.line_on {
            self.d0
        } else {
            0.0
        }
    }

    /// The configuration's speed ratio (v / the model's own) and record drive boost at 10 kHz (dB).
    pub fn speed_and_boost(&self) -> (f64, f64) {
        (self.der.rv, self.der.boost_db)
    }

    /// J-A statistics of the left channel: (RK4 steps per internal sample, resets).
    pub fn ja_stats(&self) -> (f64, u64) {
        self.ch.first().map_or((0.0, 0), |c| {
            let h = c.sat.hysteresis();
            (h.steps as f64 / h.samples.max(1) as f64, h.resets)
        })
    }

    fn model(&self) -> &'static Model {
        &MODELS[self.cfg.model]
    }

    fn target_cfg(&self) -> Config {
        let model = (self.v[p::MODEL].round() as usize).min(7);
        let alc = match self.v[p::ALC].round() as usize {
            0 => MODELS[model].alc,
            1 => false,
            _ => true,
        };
        Config { model, speed: (self.v[p::SPEED].round() as usize).min(5), formula: (self.v[p::FORMULA].round() as usize).min(3), nr: self.v[p::NR] > 0.5, alc }
    }

    /// Everything a configuration decides (allocation-free: called at a dip's bottom).
    fn apply_config(&mut self) {
        let c = self.cfg;
        let m = &MODELS[c.model];
        let tape = m.kind == Kind::Tape;
        let ips = if tape && c.speed > 0 { models::SPEEDS_IPS[c.speed] } else { m.ips };
        let rv = if tape { ips / m.ips } else { 1.0 };
        let formula = if c.formula == 0 {
            if m.hot {
                2
            } else {
                1
            }
        } else {
            c.formula
        };
        let chrome = formula == 3;
        let h_ref_db = m.h_ref_db - if formula == 2 { 3.0 } else { 0.0 };
        let boost_db = if tape { models::drive_boost_db(ips, chrome) } else { 0.0 };
        let sr = self.sr;
        let isr = sr * (1 << self.stages) as f64;
        let shelf_1k_db = record_shelf(boost_db).map_or(0.0, |(fz, fp)| 20.0 * Shelf1::design(fz, fp, isr).gain_at(1000.0, isr).log10());
        let hf = m.hf_edge * if tape { rv } else { 1.0 };
        let hf_edge = if hf < 0.45 * sr { Some(hf) } else { None };
        let bump_hz = if tape { m.bump_mm.map(|l| ips * 0.0254 / (l * 1e-3)) } else { None };
        // hiss spectrum
        let shape = match m.kind {
            Kind::Tape => {
                let tau = models::replay_tau(m, ips, chrome);
                Shelf1::design(1.0 / (2.0 * PI * tau), 20_000.0, sr)
            }
            Kind::Vinyl => Shelf1::design(1e9, 2122.0, sr),
            Kind::Shellac => Shelf1::design(1e9, 4000.0, sr),
        };
        self.der = Derived { kind: m.kind, v: ips * 0.0254, rv, h_ref_db, boost_db, shelf_1k_db, hiss_norm: 1.0, lf_edge: m.lf_edge, hf_edge, bump_hz, alc: c.alc, nr: c.nr };
        let g_h = db(h_ref_db) / medium::REF_PEAK;
        let wear = self.wear.value() as f64 / 100.0;
        for ch in self.ch.iter_mut() {
            ch.sat.set_drive(g_h);
            ch.sat.set_boost(boost_db);
            ch.disc.set(m.kind == Kind::Shellac, isr, wear);
        }
        for t in self.tn.iter_mut() {
            t.set_hiss(shape);
            t.set_mech(m.mech.motor_hz, m.mech.band);
        }
        self.transport.set_model(m, rv);
        self.designed = [f32::NAN; 6];
        self.design_filters();
        self.der.hiss_norm = self.hiss_norm(shape);
        self.update_gains();
        self.update_cache();
    }

    /// RMS of unit white noise through the hiss shape and the band edges: the hiss's calibration.
    fn hiss_norm(&self, shape: Shelf1) -> f64 {
        let Some(c0) = self.ch.first() else { return 1.0 };
        let mut sh = shape;
        sh.reset();
        let mut f = [c0.lf.clone(), c0.contour.clone(), c0.bump.clone(), c0.dip.clone(), c0.hf[0].clone(), c0.hf[1].clone()];
        for x in f.iter_mut() {
            x.reset();
        }
        let n = (0.25 * self.sr) as usize;
        let mut e = 0.0;
        for i in 0..n {
            let mut y = sh.process(if i == 0 { 1.0 } else { 0.0 }) as f32;
            if self.der.lf_edge.is_some() {
                y = f[0].process(y);
            }
            if self.der.bump_hz.is_some() {
                y = f[1].process(y);
                y = f[2].process(y);
                y = f[3].process(y);
            }
            if self.der.hf_edge.is_some() {
                y = f[4].process(y);
                y = f[5].process(y);
            }
            e += (y as f64).powi(2);
        }
        1.0 / e.sqrt().max(1e-9)
    }

    /// Band edges, head bump, bias sensitivity and cuts (keeping the filters' states).
    fn design_filters(&mut self) {
        let sr = self.sr as f32;
        let hb = self.head_bump.value();
        let bias = self.bias.value();
        let lc = self.low_cut.value();
        let hc = self.high_cut.value();
        let d = self.der;
        let model_dirty = self.designed[0].is_nan();
        // HF sensitivity: +1 dB at 50 %, −2 dB at 150 % (tape)
        let sens = if d.kind == Kind::Tape {
            if bias <= 100.0 {
                (100.0 - bias) / 50.0
            } else {
                -2.0 * (bias - 100.0) / 50.0
            }
        } else {
            0.0
        };
        for ch in self.ch.iter_mut() {
            if model_dirty {
                ch.dc.set(SvfMode::HighPass, 10.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
                if let Some(f) = d.lf_edge {
                    ch.lf.set(SvfMode::HighPass, f as f32, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
                }
                if let Some(f) = d.hf_edge {
                    ch.hf[0].set(SvfMode::LowPass, f as f32, 0.541_196_1, 0.0, sr);
                    ch.hf[1].set(SvfMode::LowPass, f as f32, 1.306_563, 0.0, sr);
                }
                if let Some(fb) = d.bump_hz {
                    ch.contour.set(SvfMode::HighPass, (0.5 * fb) as f32, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
                }
            }
            if model_dirty || hb != self.designed[1] {
                if let Some(fb) = d.bump_hz {
                    let g = hb / 100.0;
                    ch.bump.set(SvfMode::Bell, fb as f32, 1.4, 2.0 * g, sr);
                    ch.dip.set(SvfMode::Bell, (2.2 * fb) as f32, 1.4, -0.7 * g, sr);
                }
            }
            if model_dirty || bias != self.designed[2] {
                ch.sens.set(SvfMode::HighShelf, 5000.0, std::f32::consts::FRAC_1_SQRT_2, sens, sr);
            }
            if model_dirty || lc != self.designed[3] {
                ch.lc.set(SvfMode::HighPass, lc, std::f32::consts::FRAC_1_SQRT_2, 0.0, sr);
            }
            if model_dirty || hc != self.designed[4] {
                ch.hc[0].set(SvfMode::LowPass, hc, 0.541_196_1, 0.0, sr);
                ch.hc[1].set(SvfMode::LowPass, hc, 1.306_563, 0.0, sr);
            }
        }
        self.designed = [0.0, hb, bias, lc, hc, 0.0];
    }

    /// Input gain, the static makeup, the level.
    fn update_gains(&mut self) {
        let input = self.input.value() as f64;
        self.in_g = db(input);
        let loss = match self.der.kind {
            Kind::Tape => level_loss_db(self.der.h_ref_db + input + self.der.shelf_1k_db),
            Kind::Vinyl => disc_loss_db(DISC_REF * self.in_g, 1.0),
            Kind::Shellac => disc_loss_db(DISC_REF * self.in_g, 0.7),
        };
        self.out_g = db(-input - loss);
        self.level_g = db(self.level.value() as f64);
    }

    fn update_cache(&mut self) {
        let m = self.model();
        let tape = self.der.kind == Kind::Tape;
        let wear = self.wear.value() as f64 / 100.0;
        let bias = self.bias.value() as f64;
        // static spacing: `spacing`, wear's dulling, bias erasure, azimuth's equivalent (m)
        let az = self.azimuth.value() as f64 / 60.0 * PI / 180.0;
        let az_d = if tape { 0.182 * m.track_w_mm * 1e-3 * az.tan().abs() } else { 0.0 };
        let erase = if tape { 0.5e-6 * ((bias - 100.0) / 50.0).max(0.0) } else { 0.0 };
        self.cache = Cache {
            hiss: db(-18.0 + m.hiss_db + self.hiss.value() as f64) * self.der.hiss_norm,
            mech: db(-18.0 + m.mech_db + self.mech.value() as f64),
            modn: db(m.mod_db + 10.0 * wear),
            d_static: self.spacing.value() as f64 * 1e-6 + 3e-6 * wear + erase + az_d,
            v_play: if tape { self.der.v } else { self.der.v * (146.0 - 86.0 * wear) / 146.0 },
            az_off: self.az_offset(),
        };
    }

    fn knobs(&self) -> Knobs {
        let m = self.model();
        let wear = self.wear.value() as f64 / 100.0;
        let rand = self.wow_rand.value() as f64 / 100.0;
        let shift = |rho: f64| {
            let r = if rand >= 0.0 { rho + rand * (1.0 - rho) } else { rho * (1.0 + rand) };
            (r + 0.3 * wear).clamp(0.0, 1.0)
        };
        let tape = m.kind == Kind::Tape;
        Knobs {
            wow: self.wow.value() as f64 / 100.0,
            flutter: self.flutter.value() as f64 / 100.0,
            rho_wow: shift(m.rho_wow),
            rho_flutter: shift(m.rho_flutter),
            wow_rate: self.wow_rate.value() as f64 / 100.0,
            spread: if self.nch > 1 { self.spread.value() as f64 / 100.0 } else { 0.0 },
            snag_rate: 0.5 * self.snags.value() as f64 / 100.0 + if tape { 0.2 * wear } else { 0.0 },
        }
    }

    fn reset_audio(&mut self) {
        for c in self.ch.iter_mut() {
            c.reset();
        }
        for t in self.tn.iter_mut() {
            t.reset();
        }
        self.ch1_stale = false;
        self.transport.reset();
        self.stop.settle(self.v[p::STOP] > 0.5);
        self.lane_a.reset();
        self.lane_b.reset();
        self.crackle.reset();
        self.law.reset((self.v[p::NOISE_MODE].round() as usize).min(2));
        self.alc.reset();
        self.eps_prev = [0.0; 2];
        self.eps_cur = [0.0; 2];
        self.grid_phase = 0.0;
        self.free_phase = 0.0;
        self.noise_fade = if self.v[p::NOISE] > 0.5 { 1.0 } else { 0.0 };
        self.cfg = self.target_cfg();
        self.dip_gain = 1.0;
        self.dipping = false;
        self.apply_config();
        // the transport starts settled on its knobs
        let k = self.knobs();
        let need = self.line_needed(&k);
        self.d0_target = self.d0_for(&k, need);
        self.d0 = self.d0_target;
        self.line_on = need;
        self.byp_line = if need { 0.0 } else { 1.0 };
        let mix_full = self.mix.value() >= 100.0 && self.mix.target() >= 100.0;
        self.dry_on = !mix_full;
        self.wf = 0.0;
        self.cut_w = [if self.low_cut.value() > 20.5 { 1.0 } else { 0.0 }, if self.high_cut.value() < 19999.0 { 1.0 } else { 0.0 }];
    }

    fn az_offset(&self) -> f64 {
        if self.nch < 2 || self.der.kind != Kind::Tape {
            return 0.0;
        }
        let th = self.azimuth.value() as f64 / 60.0 * PI / 180.0;
        self.model().track_s_mm * 1e-3 * th.tan() / self.der.v * self.sr
    }

    fn line_needed(&self, k: &Knobs) -> bool {
        k.wow > 0.0 || k.flutter > 0.0 || k.snag_rate > 0.0 || self.az_offset() != 0.0 || !self.stop.idle()
    }

    fn d0_for(&self, k: &Knobs, need: bool) -> f64 {
        if need {
            (D_MIN + 1.2 * self.transport.excursion_s(k) * self.sr).min(D0_MAX_S * self.sr)
        } else {
            0.0
        }
    }

    /// The record path at the internal rate for one host sample.
    #[inline]
    fn record(ch: &mut Chan, x: f32, stages: usize, tape: bool) -> f32 {
        let mut buf = [0.0f32; 8];
        let mut tmp = [0.0f32; 8];
        buf[0] = x;
        let mut len = 1;
        for st in 0..stages {
            for j in 0..len {
                let u = ch.ups[st].up(buf[j]);
                tmp[2 * j] = u[0];
                tmp[2 * j + 1] = u[1];
            }
            len *= 2;
            buf[..len].copy_from_slice(&tmp[..len]);
        }
        if tape {
            for s in buf[..len].iter_mut() {
                *s = ch.sat.process(*s as f64) as f32;
            }
        } else {
            for s in buf[..len].iter_mut() {
                *s = ch.disc.process(*s as f64) as f32;
            }
        }
        for st in (0..stages).rev() {
            len /= 2;
            for j in 0..len {
                buf[j] = ch.downs[st].down([buf[2 * j], buf[2 * j + 1]]);
            }
        }
        buf[0]
    }

    /// The dry through the same cascade of half-bands as the wet (a stage at 2^k fs is not the
    /// same filter as an up / down pair at fs: one identity per stage would comb at 5 kHz).
    #[inline]
    fn identity(ch: &mut Chan, x: f32, stages: usize) -> f32 {
        let mut buf = [0.0f32; 8];
        let mut tmp = [0.0f32; 8];
        buf[0] = x;
        let mut len = 1;
        for st in 0..stages {
            for j in 0..len {
                let u = ch.dry_up[st].up(buf[j]);
                tmp[2 * j] = u[0];
                tmp[2 * j + 1] = u[1];
            }
            len *= 2;
            buf[..len].copy_from_slice(&tmp[..len]);
        }
        for st in (0..stages).rev() {
            len /= 2;
            for j in 0..len {
                buf[j] = ch.dry_dn[st].down([buf[2 * j], buf[2 * j + 1]]);
            }
        }
        buf[0]
    }
}

impl Effect for Tape {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate as f64;
        self.nch = channels.clamp(1, MAX_CH);
        // internal rate ≥ 352.8 kHz
        self.stages = if self.sr < 88_000.0 {
            3
        } else if self.sr < 176_000.0 {
            2
        } else {
            1
        };
        let isr = self.sr * (1 << self.stages) as f64;
        let line_len = ((STOP_BUF_S + D0_MAX_S) * self.sr) as usize + 64;
        let dry_len = (D0_MAX_S * self.sr) as usize + 64;
        self.ch = (0..self.nch)
            .map(|_| {
                let mut c = Chan::new();
                c.sat.prepare(isr, 1 << self.stages);
                c.line.prepare(line_len);
                c.dry.prepare(dry_len);
                c.spacing.prepare(self.sr);
                c.comp.prepare(self.sr);
                c
            })
            .collect();
        self.tn = (0..self.nch)
            .map(|k| {
                let mut t = TrackNoise::new(801 + 10 * k as u64);
                t.prepare(self.sr);
                t
            })
            .collect();
        self.transport.prepare(self.sr / CTL as f64);
        self.lane_a.prepare(self.sr);
        self.lane_b.prepare(self.sr);
        self.crackle.prepare(self.sr);
        self.law.prepare(self.sr);
        self.alc.prepare(self.sr);
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
        self.now = 0;
        self.ctl = 0;
        for c in self.ch.iter_mut() {
            c.sat.set_bias(v[p::BIAS] as f64);
        }
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

    fn set_transport(&mut self, transport: TransportInfo) {
        self.host = transport;
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(self.ch.len());
        if nch == 0 {
            return;
        }
        let n = buffers[0].len();
        let sr = self.sr;
        // a mono input on 2 / 2 (spread and azimuth at 0) computes once
        let linked = nch == 2 && self.v[p::SPREAD] == 0.0 && self.spread.value() == 0.0 && self.v[p::AZIMUTH] == 0.0 && self.azimuth.value() == 0.0 && buffers[0] == buffers[1];
        if !linked && self.ch1_stale && nch == 2 {
            let (a, b) = self.ch.split_at_mut(1);
            b[0].sync_from(&a[0]);
            self.ch1_stale = false;
        }
        let run = if linked { 1 } else { nch };
        let stages = self.stages;
        let mode = (self.v[p::NOISE_MODE].round() as usize).min(2);
        let sync = Sync::from_index(self.v[p::DROP_SYNC].round() as usize);
        let stop_on = self.v[p::STOP] > 0.5;
        let stop_time = self.v[p::STOP_TIME] as f64 / 1000.0;
        let noise_on = self.v[p::NOISE] > 0.5;
        let dt = 1.0 / sr;
        let leak = 1.0 - 2.0 * PI * LEAK_HZ / sr;
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

            // ---- structure: dip the wet across a change of model, speed, formula, NR or ALC
            let tcfg = self.target_cfg();
            if !self.dipping && tcfg != self.cfg {
                self.dipping = true;
            }
            if self.dipping {
                let step = dt / DIP_S;
                if tcfg != self.cfg {
                    self.dip_gain -= step;
                    if self.dip_gain <= 0.0 {
                        self.dip_gain = 0.0;
                        self.cfg = tcfg;
                        for c in self.ch.iter_mut() {
                            c.sat.reset();
                            c.disc.reset();
                            c.comp.reset();
                        }
                        self.apply_config();
                    }
                } else {
                    self.dip_gain += step;
                    if self.dip_gain >= 1.0 {
                        self.dip_gain = 1.0;
                        self.dipping = false;
                    }
                }
            }

            // ---- control rate
            if self.ctl == 0 {
                let k = self.knobs();
                self.eps_prev = self.eps_cur;
                self.eps_cur = self.transport.tick(&k);
                self.wf = self.transport.wf_percent() as f32;
                let need = self.line_needed(&k);
                self.d0_target = self.d0_for(&k, need);
                if need && !self.line_on {
                    self.line_on = true;
                    self.d0 = D_MIN;
                    for c in self.ch.iter_mut() {
                        c.integ = 0.0;
                    }
                }
                if moving {
                    let bias = self.bias.value() as f64;
                    let wear = self.wear.value() as f64 / 100.0;
                    let isr = sr * (1 << stages) as f64;
                    let shellac = self.der.kind == Kind::Shellac;
                    for c in self.ch.iter_mut() {
                        c.sat.set_bias(bias);
                        c.disc.set(shellac, isr, wear);
                    }
                }
                let mix_full = self.mix.value() >= 100.0 && self.mix.target() >= 100.0;
                if mix_full && self.dry_on {
                    for c in self.ch.iter_mut() {
                        for o in c.dry_up.iter_mut().chain(c.dry_dn.iter_mut()) {
                            o.reset();
                        }
                    }
                }
                self.dry_on = !mix_full;
            }
            // gains and tone filters follow their smoothers every sample (no control-rate zipper)
            if moving {
                self.design_filters();
                self.update_gains();
                self.update_cache();
            }
            let frac = (self.ctl + 1) as f64 / CTL as f64;
            self.ctl = (self.ctl + 1) % CTL;
            let eps = [self.eps_prev[0] + (self.eps_cur[0] - self.eps_prev[0]) * frac, self.eps_prev[1] + (self.eps_cur[1] - self.eps_prev[1]) * frac];

            // ---- the transport's centre, its bypass blend, the stop
            self.stop.tick(stop_on, stop_time, dt);
            if self.line_on {
                let tgt = self.d0_target.max(D_MIN);
                if tgt > self.d0 {
                    self.d0 = (self.d0 + D0_UP).min(tgt);
                } else {
                    self.d0 = (self.d0 - D0_DOWN).max(tgt);
                }
                let leaving = self.d0_target == 0.0 && self.d0 <= D_MIN + 1e-9;
                let step = dt / 0.005;
                if leaving {
                    self.byp_line = (self.byp_line + step).min(1.0);
                    if self.byp_line >= 1.0 {
                        self.line_on = false;
                        self.d0 = 0.0;
                        for c in self.ch.iter_mut() {
                            c.integ = 0.0;
                        }
                    }
                } else {
                    self.byp_line = (self.byp_line - step).max(0.0);
                }
            }

            // ---- shared per-sample sources
            let mut lvl = 0.0f32;
            for c in 0..nch {
                lvl = lvl.max(buffers[c][i].abs());
            }
            let law = self.law.next(lvl, mode);
            let nf_step = dt / 0.020;
            self.noise_fade = if noise_on { (self.noise_fade + nf_step).min(1.0) } else { (self.noise_fade - nf_step).max(0.0) };
            let g_alc = if self.der.alc { self.alc.next(lvl) } else { 1.0 };
            let m = &MODELS[self.cfg.model];
            let tape = self.der.kind == Kind::Tape;
            let wear = self.wear.value() as f64 / 100.0;
            let drop_amt = self.dropouts.value() as f64 / 100.0;
            // dropouts: Poisson, or on the host grid
            let mut rate = 3.0 * drop_amt + if tape { 1.5 * wear } else { 0.0 };
            if let Some(beats) = sync.beats() {
                let ph = match self.host.phase_at(i, beats, 0.0, sr as f32) {
                    Some(ph) => ph,
                    None => {
                        self.free_phase = (self.free_phase + self.host.bpm() / 60.0 / beats / sr).fract();
                        self.free_phase
                    }
                };
                let p_step = (drop_amt + if tape { 0.5 * wear } else { 0.0 }).min(1.0);
                if ph < self.grid_phase && p_step > 0.0 {
                    if self.lane_a.rng_draw() < p_step {
                        self.lane_a.trigger();
                    }
                    if self.lane_b.rng_draw() < p_step {
                        self.lane_b.trigger();
                    }
                }
                self.grid_phase = ph;
                rate = 0.0;
            }
            let crink = self.crinkle.value() as f64 / 100.0;
            let crink_amt = (crink + if tape { 0.5 * wear } else { 0.0 }).min(1.0);
            let crink_rate = 0.6 * crink + if tape { 0.2 * wear } else { 0.0 };
            let (da, dipa) = self.lane_a.next(rate, crink_amt, crink_rate);
            let spread = if nch > 1 { self.spread.value() as f64 / 100.0 } else { 0.0 };
            let (dd, ddip) = if spread > 0.0 && !linked {
                let (db_, dipb) = self.lane_b.next(rate, crink_amt, crink_rate);
                ([da[0], (1.0 - spread) * da[1] + spread * db_[0]], [dipa[0], (1.0 - spread) * dipa[1] + spread * dipb[0]])
            } else {
                (da, dipa)
            };
            // crackle (one stylus / one tape: shared)
            let cr = self.crackle_amt.value() as f64 / 100.0;
            let (cr_rate, cr_amp, scratches, rot) = match self.der.kind {
                Kind::Tape => (6.0 * cr, 0.125_89 * db(-40.0), 0, 0.0),
                Kind::Vinyl => (40.0 * cr + 15.0 * wear, 0.125_89 * db(-32.0), (6.0 * (0.5 * cr + 0.6 * wear).min(1.0)).round() as usize, m.rpm / 60.0),
                Kind::Shellac => (60.0 * cr + 25.0 * wear, 0.125_89 * db(-26.0), (6.0 * (0.5 * cr + 0.6 * wear).min(1.0)).round() as usize, m.rpm / 60.0),
            };
            let crk = if cr_rate > 0.0 || scratches > 0 { self.crackle.next(cr_rate, cr_amp, scratches, rot, !tape) as f64 } else { 0.0 };
            // additive noise gains, in the post-`input` domain (output-referred: `input` does not move them)
            let ng = law * self.noise_fade / self.out_g;
            let c = self.cache;
            let (hiss_a, mech_a, d_static, v_play, az_off) = (c.hiss * ng, c.mech * ng, c.d_static, c.v_play, c.az_off);
            let mod_s = if self.no_mod_noise { 0.0 } else { c.modn };
            let bias = self.bias.value() as f64;
            let extra = [(-az_off).max(0.0), az_off.max(0.0)];
            let (in_g, out_g) = (self.in_g, self.out_g * self.level_g);
            let mix = (self.mix.value() / 100.0) as f64;
            let dip = self.dip_gain;
            let byp = self.byp.value();
            let stop = self.stop;
            let (line_on, b_line, d0) = (self.line_on, self.byp_line, self.d0);
            let dry_on = self.dry_on;
            let nr = self.der.nr;
            let (lf_on, bump_on, hf_on) = (self.der.lf_edge.is_some(), self.der.bump_hz.is_some(), self.der.hf_edge.is_some());
            let lc_on = self.low_cut.value() > 20.5;
            let hc_on = self.high_cut.value() < 19999.0;
            let sens_on = tape && bias != 100.0;
            let cstep = (dt / 0.005) as f32;
            for (w, on) in self.cut_w.iter_mut().zip([lc_on, hc_on]) {
                *w = if on { (*w + cstep).min(1.0) } else { (*w - cstep).max(0.0) };
            }
            let cut_w = self.cut_w;
            for k in 0..run {
                let ch = &mut self.ch[k];
                let tn = &mut self.tn[k];
                let x = buffers[k][i];
                // record
                let mut s = x as f64 * g_alc * in_g;
                if nr {
                    s = ch.comp.encode(s);
                }
                let y = Self::record(ch, s as f32, stages, tape);
                let y = ch.dc.process(y);
                ch.line.push(y);
                // read head
                let mut w = y as f64;
                if line_on {
                    ch.integ = ch.integ * leak + eps[k];
                    let dmax = (ch.line.max_delay() - 4) as f64;
                    let d = (d0 - ch.integ + extra[k]).max(D_MIN);
                    let r = if stop.stage == Stage::Catch {
                        let a = ch.line.read((d + stop.lag).min(dmax), Interp::Lagrange5) as f64;
                        let b = ch.line.read(d.min(dmax), Interp::Lagrange5) as f64;
                        a + (b - a) * stop.x
                    } else {
                        ch.line.read((d + stop.lag).min(dmax), Interp::Lagrange5) as f64
                    };
                    w = if b_line > 0.0 { r + (w - r) * b_line } else { r };
                }
                // the medium
                w *= 1.0 + mod_s * tn.modn.next();
                if ng > 0.0 {
                    w += hiss_a * tn.hiss() + mech_a * tn.mech(m.mech.motor_share) + crk * ng;
                }
                if ddip[k] != 0.0 {
                    w *= db(ddip[k]);
                }
                ch.spacing.set(d_static + dd[k], v_play);
                let mut z = ch.spacing.process(w as f32);
                if sens_on {
                    z = ch.sens.process(z);
                }
                if lf_on {
                    z = ch.lf.process(z);
                }
                if bump_on {
                    z = ch.contour.process(z);
                    z = ch.bump.process(z);
                    z = ch.dip.process(z);
                }
                if hf_on {
                    z = ch.hf[0].process(z);
                    z = ch.hf[1].process(z);
                }
                let mut z = z as f64;
                if nr {
                    z = ch.comp.decode(z);
                }
                let mut z = z as f32;
                // the cuts engage through a 5 ms crossfade (a filter switched in mid-note steps)
                let zl = ch.lc.process(z);
                if cut_w[0] > 0.0 {
                    z += (zl - z) * cut_w[0];
                }
                let h0 = ch.hc[0].process(z);
                let h = ch.hc[1].process(h0);
                if cut_w[1] > 0.0 {
                    z += (h - z) * cut_w[1];
                }
                let wet = z as f64 * out_g * stop.amp * dip;
                // the dry: D0 and the same half-bands
                ch.dry.push(x);
                let out = if dry_on {
                    let dl = if line_on {
                        let r = ch.dry.read(d0.max(D_MIN), Interp::Lagrange5) as f64;
                        r + (x as f64 - r) * b_line
                    } else {
                        x as f64
                    };
                    let dryv = Self::identity(ch, dl as f32, stages) as f64;
                    dryv + (wet - dryv) * mix
                } else {
                    wet
                };
                let out = out as f32;
                buffers[k][i] = if byp > 0.0 { out + (x - out) * byp } else { out };
            }
            if linked {
                buffers[1][i] = buffers[0][i];
            }
            self.now += 1;
        }
        if linked {
            self.ch1_stale = true;
        }
    }
}
