// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-acoustic — golem's acoustic simulator: an electric guitar's DI (or an acoustic-electric's
//! under-saddle piezo) in, a miked acoustic guitar out (design: README.md).
//!
//! The physics in the order it happens, to one reference F̂ (the EMF of a single coil in the modelled
//! guitar's sound hole, x_s = 200 mm from the saddle):
//! 1. **Source** (gfx-dsp `pickup`): the DI back to the pickup's EMF (the inverse, two Butterworth
//!    poles at min(12 kHz, 3 × resonance), matched at the host rate), then the position shelf
//!    R(s) = (s + ω_e) / (s + ω_s) (and +3 dB at 4 kHz for a humbucker's aperture). A piezo goes to
//!    the reference through D(s) = (s + ω₀) / (s + ω_s), with the dynamic quack dip on its attacks.
//! 2. `top` shelf (2.5 kHz, ±10 dB), then the **decay** stage (decay.rs).
//! 3. **12-string** courses (gfx-dsp `erb_bank` octave below 400 Hz + the unison above 150 Hz,
//!    through gfx-dsp's centred shifter at +`twelve_detune` cents, delayed by `twelve_delay`).
//! 4. **Body** (body.rs): modal bank + one blended 1,024-tap FIR per channel; two bodies during a
//!    30 ms `model` crossfade.
//! 5. Piezo voice, `low_cut`, the one-knob compressor (gfx-comp's engine, colour off, Opto), the
//!    room (gfx-dsp `reverb` taps + FDN), × K(model) × `level`, `mix` (linear crossfade with the DI).
//!
//! Zero latency. Control ticks run on the sample clock (every 8 and 32 samples), so any block size
//! renders the same.

#![allow(clippy::needless_range_loop)]

pub mod body;
pub mod bodies;
pub mod decay;

use body::{Body, FirstOrder, Kernels, CTL};
use decay::Decay;
use gfx_comp::{static_curve, Character, Comp, Settings as CompSettings};
use gfx_dsp::delay::{DelayLine, Interp};
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::erb_bank::ErbBank;
use gfx_dsp::matched::{self, Analog};
use gfx_dsp::onset::Pick;
use gfx_dsp::pickup::{self, Guitar};
use gfx_dsp::pitch::CenteredShifter;
use gfx_dsp::reverb::{Fdn, Taps};
use gfx_dsp::smooth::OnePole;
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
mod plugin;

/// Parameter indices (PARAMS order).
pub mod p {
    pub const MODEL: usize = 0;
    pub const BODY: usize = 1;
    pub const TOP: usize = 2;
    pub const MIC: usize = 3;
    pub const MIX: usize = 4;
    pub const TWELVE: usize = 5;
    pub const PICKUP: usize = 6;
    pub const PICKUP_POS: usize = 7;
    pub const SIZE: usize = 8;
    pub const DECAY: usize = 9;
    pub const PIEZO: usize = 10;
    pub const QUACK: usize = 11;
    pub const LOW_CUT: usize = 12;
    pub const COMP: usize = 13;
    pub const ROOM: usize = 14;
    pub const WIDTH: usize = 15;
    pub const TWELVE_LEVEL: usize = 16;
    pub const TWELVE_DETUNE: usize = 17;
    pub const TWELVE_DELAY: usize = 18;
    pub const LEVEL: usize = 19;
    pub const BYPASS: usize = 20;
}

pub const NP: usize = 21;

pub const MODELS: [&str; 6] = ["Dreadnought", "Jumbo", "Concert", "Parlor", "Classical", "Round-back"];
/// gfx-fuzz's four, in its order, and `Piezo` appended here only (README §3).
pub const PICKUPS: [&str; 5] = ["Single Coil", "Humbucker", "P-90", "Active", "Piezo"];
pub const PIEZO_SOURCE: usize = 4;
pub const POSITIONS: [&str; 5] = ["Neck", "Middle", "Bridge", "Neck + Middle", "Middle + Bridge"];
/// x_p from the bridge (m) per `pickup_pos` [U]; a parallel pair is the mean of the two velocities,
/// so its x is the mean of the two positions.
pub const POSITION_X: [f64; 5] = [0.160, 0.100, 0.041, 0.130, 0.0705];

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::enumeration("model", "Model", 6, 0),
    ParamInfo::float("body", "Body", 0.0, 100.0, 50.0).with_exp(),
    ParamInfo::float("top", "Top", 0.0, 100.0, 50.0).with_exp(),
    ParamInfo::float("mic", "Mic", 0.0, 100.0, 75.0).with_exp(),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::boolean("twelve", "12-String", false),
    ParamInfo::enumeration("pickup", "Pickup", 5, 0),
    ParamInfo::enumeration("pickup_pos", "Pickup Position", 5, 0),
    ParamInfo::float("size", "Size", 70.0, 140.0, 100.0).with_exp(),
    ParamInfo::float("decay", "Decay", 0.0, 100.0, 50.0),
    ParamInfo::float("piezo", "Piezo Blend", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("quack", "Quack Cut", 0.0, 100.0, 0.0),
    ParamInfo::float("low_cut", "Low Cut", 20.0, 300.0, 20.0),
    ParamInfo::float("comp", "Comp", 0.0, 100.0, 0.0),
    ParamInfo::float("room", "Room", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("width", "Width", 0.0, 100.0, 40.0),
    ParamInfo::float("twelve_level", "Courses Level", -12.0, 6.0, 0.0),
    ParamInfo::float("twelve_detune", "Courses Detune", 0.0, 20.0, 4.0),
    ParamInfo::float("twelve_delay", "Strum Offset", 0.0, 10.0, 2.5),
    ParamInfo::float("level", "Level", -24.0, 12.0, 0.0).with_exp(),
    ParamInfo::boolean("bypass", "Bypass", false),
];

/// Each model's intrinsic voicing (README §2.3, §2.8): k_b of the decay stage (low → high band)
/// [U, voicing], its own `top` (dB), and K (dB): `level` 0 dB = the bypass loudness at the defaults
/// (measured by `tests::level_is_unity_at_the_defaults` on the DI proxy, hard-coded).
pub struct Voicing {
    pub k: [f64; 4],
    pub top_db: f64,
    pub k_db: f64,
}

pub const VOICINGS: [Voicing; 6] = [
    Voicing { k: [0.4, 0.8, 1.2, 1.6], top_db: 0.0, k_db: -5.0 },
    Voicing { k: [0.35, 0.7, 1.1, 1.5], top_db: 0.0, k_db: -0.7 },
    Voicing { k: [0.4, 0.8, 1.2, 1.6], top_db: 0.0, k_db: -6.4 },
    Voicing { k: [0.5, 0.9, 1.3, 1.7], top_db: 0.0, k_db: -0.7 },
    Voicing { k: [0.5, 1.0, 1.6, 2.2], top_db: -1.0, k_db: -5.2 },
    Voicing { k: [0.3, 0.6, 0.9, 1.2], top_db: 0.0, k_db: -6.0 },
];

/// The decay stage's fallback 1 (README §2.3): the models' k_b halved (with the 12 dB cap,
/// [`decay::CAP_DB`]), because the soft-note bench row failed with the rule as written.
pub const K_SCALE: f64 = 0.5;

/// Model `m`'s k_b as run (× [`K_SCALE`]) at `decay` 50 %.
pub fn model_k(m: usize) -> [f64; 4] {
    let k = VOICINGS[m].k;
    [k[0] * K_SCALE, k[1] * K_SCALE, k[2] * K_SCALE, k[3] * K_SCALE]
}

/// The piezo voice's level match re the mic sim (dB, measured on the DI proxy).
pub const PIEZO_MATCH_DB: f64 = 0.0;

/// The guitar of each magnetic `pickup` option (gfx-fuzz's values).
pub const GUITARS: [Guitar; 4] = [pickup::SINGLE_COIL, pickup::HUMBUCKER, pickup::P90, pickup::ACTIVE];

const MAX_CH: usize = 2;
const FADE_MS: f64 = 30.0;
const TWELVE_HOLD_MS: f64 = 250.0;
/// The compressor's reference level for its automatic make-up (dBFS).
const COMP_REF: f32 = -18.0;
/// Room: 12 early reflections (ms) of a 4 × 5 m room, the mic 1.5 m from the walls [A, voicing].
const ROOM_MS: [f64; 12] = [3.1, 4.7, 6.2, 7.9, 9.4, 11.3, 12.8, 14.6, 16.9, 19.2, 21.7, 24.4];

/// The compressor's settings for `comp` c (0 – 1): threshold −6 − 24c dBFS, ratio 1 + 3c, knee 6 dB,
/// attack 15 ms, release 150 ms auto, Opto, make-up from the static curve at −18 dBFS (README §2.8).
pub fn comp_settings(c: f32) -> CompSettings {
    let thresh = -6.0 - 24.0 * c;
    let ratio = 1.0 + 3.0 * c;
    let knee = 6.0;
    CompSettings {
        thresh,
        ratio,
        makeup: COMP_REF - static_curve(COMP_REF, thresh, ratio, knee),
        attack: 15.0,
        release: 150.0,
        character: Character::Opto,
        knee,
        auto_release: true,
        sc_hpf: 0.0,
        range: 60.0,
        drive: 0.0,
        mix: 100.0,
        link: 100.0,
        sc_listen: false,
    }
}

/// A biquad, transposed direct form II, f64.
#[derive(Clone, Copy, Debug, Default)]
struct Bq {
    b: [f64; 3],
    a: [f64; 2],
    s: [f64; 2],
}

impl Bq {
    fn from(m: matched::Biquad) -> Self {
        Self { b: [m.b0, m.b1, m.b2], a: [m.a1, m.a2], s: [0.0; 2] }
    }
    const IDENTITY: Bq = Bq { b: [1.0, 0.0, 0.0], a: [0.0, 0.0], s: [0.0; 2] };
    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.s[0];
        self.s[0] = self.b[1] * x - self.a[0] * y + self.s[1];
        self.s[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// The EMF inverse of a pickup at `fs`: the analog prototype matched at the host rate (README §2.1).
pub fn emf_biquad(g: &Guitar, fs: f64) -> matched::Biquad {
    let fp = g.acoustic_pole_hz();
    let w = 2.0 * std::f64::consts::PI * fp;
    let n = g.inverse_poly();
    matched::design(Analog { n: [n[0], n[1] * w, n[2] * w * w], d: [1.0, std::f64::consts::SQRT_2, 1.0] }, fp, fs)
}

/// One source path: (pickup, position) → F̂ before `top`.
#[derive(Clone, Copy, Debug, Default)]
struct Source {
    pickup: usize,
    pos: usize,
    emf: Bq,
    shelf: FirstOrder,
    aperture: Bq,
    d: FirstOrder,
}

impl Source {
    fn new(pickup: usize, pos: usize, fs: f64) -> Self {
        let mut s = Source { pickup, pos, emf: Bq::IDENTITY, aperture: Bq::IDENTITY, ..Default::default() };
        if pickup == PIEZO_SOURCE {
            s.d = body::d_forward(fs);
            return s;
        }
        let g = &GUITARS[pickup];
        if g.l > 0.0 {
            s.emf = Bq::from(emf_biquad(g, fs));
        }
        let xe = POSITION_X[pos.min(4)];
        s.shelf = FirstOrder::new(pickup::position_corner_hz(xe), pickup::position_corner_hz(body::X_S), fs);
        if pickup == 1 {
            s.aperture = Bq::from(matched::design(Analog::high_shelf(std::f64::consts::FRAC_1_SQRT_2, 3.0), 4000.0, fs));
        }
        s
    }

    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        if self.pickup == PIEZO_SOURCE {
            return self.d.process(x);
        }
        let e = self.emf.process(x);
        let r = self.shelf.process(e);
        self.aperture.process(r)
    }
}

/// The 12-string course generator (README §2.4).
#[derive(Clone, Debug, Default)]
struct Twelve {
    bank: ErbBank,
    lp: [Svf; 2],
    hp: [Svf; 2],
    pre: DelayLine,
    shifter: CenteredShifter,
    /// the voices' level (20 ms fade) and the hold after "off"
    fade: f64,
    fade_step: f64,
    hold: u32,
    hold_n: u32,
    /// after a cold start the voices run unheard this long (samples) before fading in: the bands,
    /// the strum offset and the shifter's centre fill first (no onset of a delayed tone mid-fade)
    warm: u32,
    warm_n: u32,
}

impl Twelve {
    fn prepare(&mut self, fs: f32) {
        self.bank.prepare(fs, 400.0);
        for f in self.lp.iter_mut() {
            f.set(SvfMode::LowPass, 800.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, fs);
        }
        for f in self.hp.iter_mut() {
            f.set(SvfMode::HighPass, 150.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, fs);
        }
        self.pre.prepare((0.012 * fs as f64) as usize + 8);
        self.shifter.prepare(fs, 3.0, 1.0, 50.0);
        self.fade_step = 1.0 / (0.020 * fs as f64);
        self.hold_n = (TWELVE_HOLD_MS * 0.001 * fs as f64) as u32;
        self.warm_n = (0.025 * fs as f64) as u32;
        self.reset(false);
    }

    fn reset(&mut self, on: bool) {
        self.bank.reset();
        for f in self.lp.iter_mut().chain(self.hp.iter_mut()) {
            f.reset();
        }
        self.pre.reset();
        self.shifter.reset();
        self.fade = if on { 1.0 } else { 0.0 };
        self.hold = if on { self.hold_n } else { 0 };
        self.warm = if on { 0 } else { self.warm_n };
    }

    /// The course voices for one sample of F̂ (before the level).
    #[inline]
    fn process(&mut self, x: f64, delay: f64) -> f64 {
        let o = self.bank.octave_up(x) as f32;
        let o = self.lp[0].process(o);
        let o = self.lp[1].process(o);
        let u = self.hp[0].process(x as f32);
        let u = self.hp[1].process(u);
        self.pre.push(o + u);
        let d = self.pre.read(delay.max(0.0), Interp::Linear);
        self.shifter.process(d) as f64
    }
}

/// The room around the mic (README §2.8).
#[derive(Clone, Default)]
struct Room {
    tail: u32,
    tail_n: u32,
    taps: [Taps; 2],
    fdn: Fdn,
    on: bool,
}

impl Room {
    fn prepare(&mut self, fs: f32) {
        let ms = fs as f64 * 0.001;
        for c in 0..2 {
            self.taps[c].prepare((26.0 * 1.07 * ms) as usize + 16, ROOM_MS.len());
            for (k, t) in ROOM_MS.iter().enumerate() {
                let t = if c == 0 { *t } else { t * 1.07 + 0.3 };
                let g = (if k % 2 == 0 { 0.5 } else { -0.45 }) / (1.0 + 0.12 * k as f64);
                self.taps[c].set(k, t * ms, g as f32);
            }
            self.taps[c].settle();
        }
        self.fdn.prepare(fs, 1.0);
        self.fdn.set_scale(0.3);
        self.fdn.settle();
        self.fdn.set_decay(0.35, 0.35, 0.2, 200.0, 5000.0);
        self.tail_n = fs as u32;
        self.reset();
    }

    fn reset(&mut self) {
        for t in self.taps.iter_mut() {
            t.reset();
        }
        self.fdn.reset();
        self.on = false;
    }

    /// The wet for each channel's dry. Both decorrelated channels always run; `spread` (0 – 1)
    /// blends them from their mean (0: L = R, the mono room) to fully decorrelated (1).
    #[inline]
    fn process(&mut self, x: [f32; 2], nch: usize, spread: f32) -> [f32; 2] {
        let xr = if nch == 2 { x[1] } else { x[0] };
        self.taps[0].line.push(x[0]);
        self.taps[1].line.push(xr);
        let e = [self.taps[0].read(0, 1), self.taps[1].read(0, 1)];
        let m = 0.5 * (x[0] + xr);
        let (l, r) = self.fdn.process(&[0.25 * m; 16]);
        let w = [e[0] + 0.5 * l, e[1] + 0.5 * r];
        let avg = 0.5 * (w[0] + w[1]);
        [avg + (w[0] - avg) * spread, avg + (w[1] - avg) * spread]
    }
}

/// Smoothed control-rate values (advanced once per [`CTL`] tick).
struct Ctl {
    body: ExpSmoother,
    top: ExpSmoother,
    mic: ExpSmoother,
    size: ExpSmoother,
    decay: OnePole,
    quack: OnePole,
    low_cut: OnePole,
    comp: OnePole,
    width: OnePole,
    t_level: OnePole,
    t_detune: OnePole,
    t_delay: OnePole,
}

pub struct Acoustic {
    v: [f32; NP],
    fs: f32,
    channels: usize,
    clock: u64,
    tick: u64,
    ctl: Ctl,
    mix: ExpSmoother,
    level: ExpSmoother,
    piezo: ExpSmoother,
    room_amt: ExpSmoother,
    /// K(model), glided over 30 ms on a model change
    k_db: OnePole,
    bypass: f64,
    bypass_step: f64,
    // source: two paths and their crossfade
    src: [Source; 2],
    src_fade: f64,
    src_fading: bool,
    fade_step: f64,
    top: Svf,
    decay: Decay,
    twelve: Twelve,
    twelve_on: bool,
    // body: two slots, the crossfade
    kernels: Kernels,
    bodies: [Body; 2],
    body_fade: f64,
    body_fading: bool,
    body_g: f64,
    body_step: f64,
    // after the body
    pz_dinv: FirstOrder,
    pz_bell: Svf,
    quack_bell: Svf,
    quack_pick: Pick,
    quack_env: f64,
    quack_rel: f64,
    low_cut: [Svf; 2],
    low_cut_on: bool,
    comp: Comp,
    comp_on: bool,
    comp_set: Option<f32>,
    room: Room,
    // per-block scratch (sized in prepare)
    dry: [Vec<f32>; MAX_CH],
    wet: [Vec<f32>; MAX_CH],
}

impl Default for Acoustic {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn idx(v: f32, n: usize) -> usize {
    (v.round().max(0.0) as usize).min(n - 1)
}

#[inline]
fn db(x: f64) -> f64 {
    10f64.powf(x / 20.0)
}

impl Acoustic {
    pub fn new() -> Self {
        let v: [f32; NP] = std::array::from_fn(|i| PARAMS[i].default);
        let one = |t: f32, x: f32| OnePole::new(t, x, 1e-4);
        Self {
            v,
            fs: 48000.0,
            channels: 1,
            clock: 0,
            tick: 0,
            ctl: Ctl {
                body: ExpSmoother::new(0.0, 100.0, v[p::BODY]),
                top: ExpSmoother::new(0.0, 100.0, v[p::TOP]),
                mic: ExpSmoother::new(0.0, 100.0, v[p::MIC]),
                size: ExpSmoother::new(70.0, 140.0, v[p::SIZE]),
                decay: one(30.0, v[p::DECAY]),
                quack: one(30.0, v[p::QUACK]),
                low_cut: one(30.0, v[p::LOW_CUT]),
                comp: one(20.0, v[p::COMP]),
                width: one(20.0, v[p::WIDTH]),
                t_level: one(20.0, v[p::TWELVE_LEVEL]),
                t_detune: one(30.0, v[p::TWELVE_DETUNE]),
                t_delay: one(30.0, v[p::TWELVE_DELAY]),
            },
            mix: ExpSmoother::new(0.0, 100.0, v[p::MIX]),
            level: ExpSmoother::new(-24.0, 12.0, v[p::LEVEL]),
            piezo: ExpSmoother::new(0.0, 100.0, v[p::PIEZO]),
            room_amt: ExpSmoother::new(0.0, 100.0, v[p::ROOM]),
            k_db: OnePole::new(10.0, 0.0, 1e-6),
            bypass: 0.0,
            bypass_step: 0.0,
            src: [Source::default(); 2],
            src_fade: 0.0,
            src_fading: false,
            fade_step: 0.0,
            top: Svf::new(),
            decay: Decay::default(),
            twelve: Twelve::default(),
            twelve_on: false,
            kernels: Kernels::default(),
            bodies: Default::default(),
            body_fade: 0.0,
            body_fading: false,
            body_g: 1.0,
            body_step: 0.0,
            pz_dinv: FirstOrder::default(),
            pz_bell: Svf::new(),
            quack_bell: Svf::new(),
            quack_pick: Pick::default(),
            quack_env: 0.0,
            quack_rel: 0.0,
            low_cut: [Svf::new(), Svf::new()],
            low_cut_on: false,
            comp: Comp::new(),
            comp_on: false,
            comp_set: None,
            room: Room::default(),
            dry: Default::default(),
            wet: Default::default(),
        }
    }

    pub fn values(&self) -> [f32; NP] {
        self.v
    }

    /// The compressor's gain reduction (dB), the `gr` meter.
    pub fn gain_reduction_db(&self) -> f32 {
        if self.comp_on {
            self.comp.gain_reduction_db()
        } else {
            0.0
        }
    }

    /// The decay stage (tests).
    pub fn decay_stage(&self) -> &Decay {
        &self.decay
    }

    fn model(&self) -> usize {
        idx(self.v[p::MODEL], 6)
    }

    fn mic_positions(&self, mic: f32, width: f32) -> [f32; 2] {
        let m = mic / 100.0;
        if self.channels == 2 {
            let w = width / 200.0;
            [(m - w).clamp(0.0, 1.0), (m + w).clamp(0.0, 1.0)]
        } else {
            [m, m]
        }
    }

    fn decay_k(&self, amount: f32) -> [f64; 4] {
        let k = model_k(self.model());
        let s = amount as f64 / 50.0;
        [k[0] * s, k[1] * s, k[2] * s, k[3] * s]
    }

    fn top_biquad(&self, top: f32) -> matched::Biquad {
        let g = VOICINGS[self.model()].top_db + (top as f64 - 50.0) / 50.0 * 10.0;
        matched::design(Analog::high_shelf(std::f64::consts::FRAC_1_SQRT_2, g), 2500.0, self.fs as f64)
    }

    fn body_gain(&self, body: f32) -> f64 {
        db((body as f64 - 50.0) * 0.24)
    }

    /// Settles every smoother and structure on the current values (reset, prepare).
    fn settle_all(&mut self) {
        let v = self.v;
        let fs = self.fs as f64;
        self.ctl.body.settle(v[p::BODY]);
        self.ctl.top.settle(v[p::TOP]);
        self.ctl.mic.settle(v[p::MIC]);
        self.ctl.size.settle(v[p::SIZE]);
        for (s, i) in [
            (&mut self.ctl.decay, p::DECAY),
            (&mut self.ctl.quack, p::QUACK),
            (&mut self.ctl.low_cut, p::LOW_CUT),
            (&mut self.ctl.comp, p::COMP),
            (&mut self.ctl.width, p::WIDTH),
            (&mut self.ctl.t_level, p::TWELVE_LEVEL),
            (&mut self.ctl.t_detune, p::TWELVE_DETUNE),
            (&mut self.ctl.t_delay, p::TWELVE_DELAY),
        ] {
            s.set_immediate(v[i]);
        }
        self.mix.settle(v[p::MIX]);
        self.level.settle(v[p::LEVEL]);
        self.piezo.settle(v[p::PIEZO]);
        self.room_amt.settle(v[p::ROOM]);
        self.k_db.set_immediate(VOICINGS[idx(v[p::MODEL], 6)].k_db as f32);
        self.bypass = if v[p::BYPASS] > 0.5 { 1.0 } else { 0.0 };
        self.src[0] = Source::new(idx(v[p::PICKUP], 5), idx(v[p::PICKUP_POS], 5), fs);
        self.src_fade = 0.0;
        self.src_fading = false;
        let tb = self.top_biquad(v[p::TOP]);
        self.top.set_biquad(&tb);
        self.top.reset();
        self.decay.reset();
        self.decay.set_k(self.decay_k(v[p::DECAY]));
        self.decay.reset();
        self.twelve_on = v[p::TWELVE] > 0.5;
        self.twelve.reset(self.twelve_on);
        let mic = self.mic_positions(v[p::MIC], v[p::WIDTH]);
        let model = self.model();
        let size = v[p::SIZE];
        self.bodies[0].load(model, &self.kernels, mic, size);
        self.body_fade = 0.0;
        self.body_fading = false;
        self.body_g = self.body_gain(v[p::BODY]);
        self.body_step = 0.0;
        self.pz_dinv.reset();
        self.pz_bell.reset();
        self.quack_bell.reset();
        self.quack_pick.reset();
        self.quack_env = 0.0;
        for f in self.low_cut.iter_mut() {
            f.set(SvfMode::HighPass, v[p::LOW_CUT], std::f32::consts::FRAC_1_SQRT_2, 0.0, self.fs);
            f.reset();
        }
        self.low_cut_on = v[p::LOW_CUT] > 20.0;
        let c = v[p::COMP];
        self.comp.set(comp_settings(c / 100.0));
        self.comp.reset();
        self.comp_on = c > 0.0;
        self.comp_set = Some(c);
        self.room.reset();
        self.room.on = v[p::ROOM] > 0.0;
        self.room.tail = if self.room.on { self.room.tail_n } else { 0 };
        self.clock = 0;
        self.tick = 0;
    }

    /// Every [`CTL`] samples.
    fn control(&mut self) {
        let fs = self.fs as f64;
        let t = self.tick;
        let c = &mut self.ctl;
        let (body, top, mic, size) = (c.body.next(), c.top.next(), c.mic.next(), c.size.next());
        let (dec, quack, low_cut, comp, width) = (c.decay.next(), c.quack.next(), c.low_cut.next(), c.comp.next(), c.width.next());
        let (t_level, t_detune, t_delay) = (c.t_level.next(), c.t_detune.next(), c.t_delay.next());
        let _ = (t_level, t_detune, t_delay, t);
        // source: a structural change crossfades a second path in (30 ms)
        let (pk, pos) = (idx(self.v[p::PICKUP], 5), idx(self.v[p::PICKUP_POS], 5));
        let want = (pk, if pk == PIEZO_SOURCE { 0 } else { pos });
        let have = (self.src[0].pickup, if self.src[0].pickup == PIEZO_SOURCE { 0 } else { self.src[0].pos });
        if !self.src_fading && want != have {
            self.src[1] = Source::new(pk, pos, fs);
            self.src_fade = 0.0;
            self.src_fading = true;
        }
        // top
        let tb = self.top_biquad(top);
        self.top.set_biquad_ramped(&tb, CTL as u32);
        // decay
        self.decay.set_k(self.decay_k(dec));
        // twelve
        let on = self.v[p::TWELVE] > 0.5;
        if on && !self.twelve_on && self.twelve.hold == 0 && self.twelve.fade == 0.0 {
            self.twelve.reset(false);
        }
        self.twelve_on = on;
        self.twelve.shifter.set_semitones(t_detune as f64 / 100.0);
        // body: model crossfade, mic, size, gain
        let micp = self.mic_positions(mic, width);
        let model = self.model();
        if !self.body_fading && model != self.bodies[0].model {
            self.bodies[1].load(model, &self.kernels, micp, size);
            let (a, b) = self.bodies.split_at_mut(1);
            b[0].warm_from(&a[0]);
            self.body_fade = 0.0;
            self.body_fading = true;
        }
        for b in 0..if self.body_fading { 2 } else { 1 } {
            self.bodies[b].control(micp, size, &self.kernels);
        }
        let g = self.body_gain(body);
        self.body_step = (g - self.body_g) / CTL as f64;
        // the piezo voice's bell; the quack dip
        self.pz_bell.set(SvfMode::Bell, 2000.0, 1.2, 4.0, self.fs);
        let _ = quack;
        // low cut
        if low_cut > 20.0 {
            if !self.low_cut_on {
                for f in self.low_cut.iter_mut() {
                    f.reset();
                }
                self.low_cut_on = true;
            }
            for f in self.low_cut.iter_mut() {
                f.set(SvfMode::HighPass, low_cut, std::f32::consts::FRAC_1_SQRT_2, 0.0, self.fs);
            }
        } else {
            self.low_cut_on = false;
        }
        // comp: off (not run, bit-transparent) once it has settled on 0
        if comp == 0.0 {
            if self.comp_on {
                self.comp.set(comp_settings(0.0));
                self.comp.reset();
                self.comp_on = false;
                self.comp_set = Some(0.0);
            }
        } else {
            self.comp_on = true;
            if self.comp_set != Some(comp) {
                self.comp.set(comp_settings(comp / 100.0));
                self.comp_set = Some(comp);
            }
        }
        self.tick += 1;
    }

    /// Every [`decay::TICK`] samples: the decay rule, the quack dip.
    fn control8(&mut self) {
        self.decay.control();
        let pk = idx(self.v[p::PICKUP], 5);
        let q = self.ctl.quack.value();
        if pk == PIEZO_SOURCE && q > 0.0 {
            let depth = -9.0 * q as f64 / 100.0 * self.quack_env;
            self.quack_bell.set(SvfMode::Bell, 2200.0, 1.2, depth as f32, self.fs);
        }
    }

    fn render(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(MAX_CH).min(self.channels.max(1));
        let n = buffers[0].len();
        let mut i = 0;
        while i < n {
            // chunks aligned on the 64-sample grid (the compressor's blocks never depend on the host's)
            let len = (64 - (self.clock % 64) as usize).min(n - i);
            self.chunk(buffers, i, len, nch);
            i += len;
        }
    }

    fn chunk(&mut self, buffers: &mut [&mut [f32]], at: usize, len: usize, nch: usize) {
        let fs = self.fs as f64;
        let quack_on = idx(self.v[p::PICKUP], 5) == PIEZO_SOURCE || self.src[0].pickup == PIEZO_SOURCE;
        for k in 0..len {
            if self.clock.is_multiple_of(CTL as u64) {
                self.control();
            }
            if self.clock.is_multiple_of(decay::TICK as u64) {
                self.control8();
            }
            self.clock += 1;
            let x = if nch == 2 { 0.5 * (buffers[0][at + k] + buffers[1][at + k]) } else { buffers[0][at + k] };
            for c in 0..nch {
                self.dry[c][k] = buffers[c][at + k];
            }
            let xd = x as f64;
            // source
            let mut f = self.src[0].process(xd);
            let mut pz_share = if self.src[0].pickup == PIEZO_SOURCE { 1.0 } else { 0.0 };
            if self.src_fading {
                let g = self.src[1].process(xd);
                self.src_fade += self.fade_step;
                let u = self.src_fade.min(1.0);
                f += (g - f) * u;
                let s1 = if self.src[1].pickup == PIEZO_SOURCE { 1.0 } else { 0.0 };
                pz_share += (s1 - pz_share) * u;
                if self.src_fade >= 1.0 {
                    self.src[0] = self.src[1];
                    self.src_fading = false;
                }
            }
            // the quack dip (piezo source)
            if quack_on && self.ctl.quack.value() > 0.0 {
                if self.quack_pick.next(x, x.abs() > 1e-4) {
                    self.quack_env = 1.0;
                }
                self.quack_env *= self.quack_rel;
                f = self.quack_bell.process(f as f32) as f64;
            }
            f = self.top.process(f as f32) as f64;
            f = self.decay.process(f);
            // twelve
            if self.twelve_on || self.twelve.hold > 0 {
                let delay = (self.ctl.t_delay.value() as f64) * 0.001 * fs;
                let c = self.twelve.process(f, delay);
                let target = if self.twelve_on { 1.0 } else { 0.0 };
                if self.twelve.warm > 0 {
                    self.twelve.warm -= 1;
                } else if self.twelve.fade < target {
                    self.twelve.fade = (self.twelve.fade + self.twelve.fade_step).min(1.0);
                } else if self.twelve.fade > target {
                    self.twelve.fade = (self.twelve.fade - self.twelve.fade_step).max(0.0);
                }
                if self.twelve_on {
                    self.twelve.hold = self.twelve.hold_n;
                } else if self.twelve.fade == 0.0 {
                    self.twelve.hold -= 1;
                }
                f += c * self.twelve.fade * db(self.ctl.t_level.value() as f64);
            }
            // body
            let (mut m, mut h) = self.bodies[0].process(f);
            if self.body_fading {
                let (m1, h1) = self.bodies[1].process(f);
                self.body_fade += self.fade_step;
                let u = self.body_fade.min(1.0);
                for c in 0..2 {
                    m[c] += (m1[c] - m[c]) * u;
                    h[c] += (h1[c] - h[c]) * u;
                }
                if self.body_fade >= 1.0 {
                    self.bodies.swap(0, 1);
                    self.body_fading = false;
                }
            }
            self.body_g += self.body_step;
            let mut y = [m[0] * self.body_g + h[0], m[1] * self.body_g + h[1]];
            if nch == 1 {
                y[1] = 0.0;
            }
            // the piezo voice
            let pzv = self.piezo.next() as f64 / 100.0;
            if pzv > 0.0 || !self.piezo.is_settled() {
                let sim = self.pz_bell.process(self.pz_dinv.process(f) as f32) as f64;
                let pz = (sim + (xd - sim) * pz_share) * db(PIEZO_MATCH_DB);
                let norm = 1.0 / (1.0 + pzv * pzv).sqrt();
                for c in 0..nch {
                    y[c] = (y[c] + pzv * pz) * norm;
                }
            }
            for c in 0..nch {
                let mut v = y[c] as f32;
                if self.low_cut_on {
                    v = self.low_cut[c].process(v);
                }
                self.wet[c][k] = v;
            }
        }
        if self.comp_on {
            let (a, b) = self.wet.split_at_mut(1);
            if nch == 2 {
                self.comp.process_block(&mut [&mut a[0][..len], &mut b[0][..len]]);
            } else {
                self.comp.process_block(&mut [&mut a[0][..len]]);
            }
        }
        self.k_db.set_target(VOICINGS[self.model()].k_db as f32);
        // the room's L / R decorrelation follows `width` (full from 40 %): width 0 is the mono room
        let spread = if nch == 2 { (self.ctl.width.value() / 40.0).min(1.0) } else { 0.0 };
        for k in 0..len {
            let room = self.room_amt.next() / 100.0;
            // the room's input is scaled by `room` (its own smoothing fades it in and out), and the
            // room runs on for 1 s after `room` reaches 0 (its tail)
            if room > 0.0 || !self.room_amt.is_settled() {
                if !self.room.on {
                    self.room.reset();
                    self.room.on = true;
                }
                self.room.tail = self.room.tail_n;
            }
            if self.room.on {
                let w = self.room.process([room * self.wet[0][k], if nch == 2 { room * self.wet[1][k] } else { 0.0 }], nch, spread);
                for c in 0..nch {
                    self.wet[c][k] += w[c];
                }
                if room <= 0.0 {
                    self.room.tail = self.room.tail.saturating_sub(1);
                    if self.room.tail == 0 {
                        self.room.on = false;
                    }
                }
            }
            let g = db(self.k_db.next() as f64 + self.level.next() as f64) as f32;
            let mix = self.mix.next() / 100.0;
            let target = if self.v[p::BYPASS] > 0.5 { 1.0 } else { 0.0 };
            if self.bypass != target {
                self.bypass = if target > self.bypass { (self.bypass + self.bypass_step).min(1.0) } else { (self.bypass - self.bypass_step).max(0.0) };
            }
            let bp = self.bypass as f32;
            for c in 0..nch {
                let d = self.dry[c][k];
                let mut o = if mix >= 1.0 {
                    self.wet[c][k] * g
                } else if mix <= 0.0 {
                    d
                } else {
                    d + (self.wet[c][k] * g - d) * mix
                };
                if bp > 0.0 {
                    o = if bp >= 1.0 { d } else { o + (d - o) * bp };
                }
                buffers[c][at + k] = o;
            }
        }
    }
}

impl Effect for Acoustic {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, max_block: usize, channels: usize) {
        let _ = max_block;
        self.fs = sample_rate;
        self.channels = channels.clamp(1, MAX_CH);
        let fs = sample_rate as f64;
        let tick_rate = sample_rate / CTL as f32;
        for e in [&mut self.ctl.body, &mut self.ctl.top, &mut self.ctl.mic, &mut self.ctl.size] {
            e.prepare(tick_rate);
        }
        for o in [
            &mut self.ctl.decay,
            &mut self.ctl.quack,
            &mut self.ctl.low_cut,
            &mut self.ctl.comp,
            &mut self.ctl.width,
            &mut self.ctl.t_level,
            &mut self.ctl.t_detune,
            &mut self.ctl.t_delay,
        ] {
            o.prepare(tick_rate);
        }
        for e in [&mut self.mix, &mut self.level, &mut self.piezo, &mut self.room_amt] {
            e.prepare(sample_rate);
        }
        {
            self.k_db.prepare(sample_rate);
        }
        self.bypass_step = 1.0 / (0.010 * fs);
        self.fade_step = 1.0 / (FADE_MS * 0.001 * fs);
        self.decay.prepare(sample_rate);
        self.twelve.prepare(sample_rate);
        self.kernels = Kernels::build(fs);
        for b in self.bodies.iter_mut() {
            b.prepare(fs, self.kernels.taps, self.channels);
        }
        self.pz_dinv = body::d_inverse(fs);
        self.pz_bell.set(SvfMode::Bell, 2000.0, 1.2, 4.0, sample_rate);
        self.quack_pick.prepare(sample_rate);
        self.quack_rel = (-1.0 / (0.030 * fs)).exp();
        self.comp.set_colour(false);
        self.comp.prepare(sample_rate, 64, self.channels);
        self.room.prepare(sample_rate);
        for c in 0..MAX_CH {
            self.dry[c] = vec![0.0; 64];
            self.wet[c] = vec![0.0; 64];
        }
        self.settle_all();
    }

    fn reset(&mut self) {
        self.ctl.body.reset();
        self.ctl.top.reset();
        self.ctl.mic.reset();
        self.ctl.size.reset();
        self.mix.reset();
        self.level.reset();
        self.piezo.reset();
        self.room_amt.reset();
        for b in self.bodies.iter_mut() {
            b.reset();
        }
        self.settle_all();
    }

    fn set_param(&mut self, id: &str, value: f32) {
        let Some(i) = PARAMS.iter().position(|p| p.id == id) else {
            return;
        };
        let pi = &PARAMS[i];
        let v = value.clamp(pi.min, pi.max);
        self.v[i] = v;
        let now = self.clock;
        let tick = self.tick;
        match i {
            p::BODY => self.ctl.body.set_target(v, tick),
            p::TOP => self.ctl.top.set_target(v, tick),
            p::MIC => self.ctl.mic.set_target(v, tick),
            p::SIZE => self.ctl.size.set_target(v, tick),
            p::DECAY => self.ctl.decay.set_target(v),
            p::QUACK => self.ctl.quack.set_target(v),
            p::LOW_CUT => self.ctl.low_cut.set_target(v),
            p::COMP => self.ctl.comp.set_target(v),
            p::WIDTH => self.ctl.width.set_target(v),
            p::TWELVE_LEVEL => self.ctl.t_level.set_target(v),
            p::TWELVE_DETUNE => self.ctl.t_detune.set_target(v),
            p::TWELVE_DELAY => self.ctl.t_delay.set_target(v),
            p::MIX => self.mix.set_target(v, now),
            p::LEVEL => self.level.set_target(v, now),
            p::PIEZO => self.piezo.set_target(v, now),
            p::ROOM => self.room_amt.set_target(v, now),
            _ => {}
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        if buffers.is_empty() || buffers[0].is_empty() {
            return;
        }
        self.render(buffers);
    }
}

#[cfg(test)]
mod tests;
