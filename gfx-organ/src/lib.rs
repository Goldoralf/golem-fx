// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-organ — golem's guitar organ (design: README.md), displayed "GoleM FX Organ".
//!
//! A polyphonic tonewheel organ played from a guitar or bass DI: gfx-dsp `spectral` (polysynth's core)
//! splits the guitar so that each band holds one partial; the root sieve (gfx-dsp `spectral`, §2.3)
//! finds the bands that are a note's fundamental; on those the nine drawbars are powers of the band's
//! phase (§2.2, [`law`]). Then percussion and the click's bounce (in the law), the click's burst, the
//! swell, the scanner vibrato / chorus (gfx-dsp `scanner`), the `organ` gain (the swell pedal, before
//! the Leslie's drive), the Leslie (gfx-dsp `rotary`), a ceiling, and the untouched dry.
//!
//! - Bank rate 22.05 – 24 kHz at every host rate (÷ 1, 2, 4, 8, polysynth's path); the scanner and
//!   the Leslie at the host rate.
//! - Zero reported latency; the dry is sample-aligned.
//! - Deterministic: fixed seeds, reset on `reset()`.

#![allow(clippy::needless_range_loop)]

pub mod law;

#[cfg(feature = "plugin")]
pub mod plugin;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod sieve_bench;

use crate::law::OrganLaw;
use gfx_dsp::delay::DelayLine;
use gfx_dsp::envelope::SlidingMax;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::noise::White;
use gfx_dsp::onset::Pick;
use gfx_dsp::oversample::{OversamplerN, MAX_FACTOR};
use gfx_dsp::rotary::{self, Rotary};
use gfx_dsp::scanner::{Scanner, V_DEPTH};
use gfx_dsp::smooth::OnePole;
use gfx_dsp::spectral::{BankMode, Core, CoreSettings};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::{Effect, ParamInfo, ParamKind};
use std::f64::consts::TAU;

pub const NP: usize = 36;

const fn int(id: &'static str, name: &'static str, min: f32, max: f32, default: f32) -> ParamInfo {
    ParamInfo { id, name, min, max, default, kind: ParamKind::Int, exp: false }
}

/// The parameters (README §3; order and ids final, append only).
pub static PARAMS: [ParamInfo; NP] = [
    // front panel
    ParamInfo::enumeration("register", "Register", 10, 1),
    ParamInfo::float("organ", "Organ", -60.0, 12.0, 0.0).with_exp(),
    ParamInfo::float("dry", "Dry", -60.0, 0.0, -60.0).with_exp(),
    ParamInfo::float("mod", "Mod", 0.0, 100.0, 50.0).with_exp(),
    ParamInfo::float("click", "Click", 0.0, 100.0, 30.0),
    ParamInfo::enumeration("leslie", "Rotor", 3, 1),
    // expert: golem `organ`'s drawbars and percussion
    int("db16", "16'", 0.0, 8.0, 8.0),
    int("db513", "5⅓'", 0.0, 8.0, 8.0),
    int("db8", "8'", 0.0, 8.0, 8.0),
    int("db4", "4'", 0.0, 8.0, 0.0),
    int("db223", "2⅔'", 0.0, 8.0, 0.0),
    int("db2", "2'", 0.0, 8.0, 0.0),
    int("db135", "1⅗'", 0.0, 8.0, 0.0),
    int("db113", "1⅓'", 0.0, 8.0, 0.0),
    int("db1", "1'", 0.0, 8.0, 0.0),
    ParamInfo::boolean("perc", "Percussion", false),
    ParamInfo::enumeration("perc_harm", "Perc Harmonic", 2, 1),
    ParamInfo::enumeration("perc_decay", "Perc Decay", 2, 1),
    ParamInfo::enumeration("perc_vol", "Perc Volume", 2, 0),
    ParamInfo::enumeration("perc_trig", "Perc Trigger", 2, 0),
    ParamInfo::enumeration("vibrato", "Vibrato", 7, 0),
    ParamInfo::enumeration("speaker", "Speaker", 6, 1),
    ParamInfo::enumeration("tuning", "Tuning", 2, 0),
    ParamInfo::float("drive", "Drive", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("ramp", "Ramp", 0.0, 400.0, 100.0).with_exp(),
    ParamInfo::float("spacing", "Mic Spacing", 0.0, 100.0, 50.0),
    ParamInfo::float("hold", "Hold", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::float("touch", "Touch", 0.0, 100.0, 30.0),
    ParamInfo::float("strings", "Strings", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("swell", "Swell", 0.0, 2000.0, 0.0).with_exp(),
    ParamInfo::float("release", "Release", 5.0, 2000.0, 60.0),
    ParamInfo::enumeration("bank", "Resolution", 3, 0),
    ParamInfo::float("lowest", "Lowest Note", 30.0, 150.0, 70.0),
    ParamInfo::float("thresh", "Noise Floor", -96.0, -30.0, -66.0),
    ParamInfo::boolean("bypass", "Bypass", false),
    ParamInfo::boolean("trails", "Trails", true),
];

pub mod p {
    pub const REGISTER: usize = 0;
    pub const ORGAN: usize = 1;
    pub const DRY: usize = 2;
    pub const MOD: usize = 3;
    pub const CLICK: usize = 4;
    pub const LESLIE: usize = 5;
    pub const DB16: usize = 6;
    pub const PERC: usize = 15;
    pub const PERC_HARM: usize = 16;
    pub const PERC_DECAY: usize = 17;
    pub const PERC_VOL: usize = 18;
    pub const PERC_TRIG: usize = 19;
    pub const VIBRATO: usize = 20;
    pub const SPEAKER: usize = 21;
    pub const TUNING: usize = 22;
    pub const DRIVE: usize = 23;
    pub const RAMP: usize = 24;
    pub const SPACING: usize = 25;
    pub const HOLD: usize = 26;
    pub const TOUCH: usize = 27;
    pub const STRINGS: usize = 28;
    pub const SWELL: usize = 29;
    pub const RELEASE: usize = 30;
    pub const BANK: usize = 31;
    pub const LOWEST: usize = 32;
    pub const THRESH: usize = 33;
    pub const BYPASS: usize = 34;
    pub const TRAILS: usize = 35;
}

/// Option labels (enum order, final; append only).
pub const REGISTERS: [&str; 10] = ["Tutti", "Trio", "Choir", "Bark", "Nave", "Chime", "Velvet", "Combo", "Coupler", "Custom"];
pub const ROTOR: [&str; 3] = ["Stop", "Slow", "Fast"];
pub const PERC_HARMS: [&str; 2] = ["2nd", "3rd"];
pub const PERC_DECAYS: [&str; 2] = ["Slow", "Fast"];
pub const PERC_VOLS: [&str; 2] = ["Normal", "Soft"];
pub const PERC_TRIGS: [&str; 2] = ["Pick", "Single"];
pub const VIBRATOS: [&str; 7] = ["Off", "V1", "C1", "V2", "C2", "V3", "C3"];
pub const SPEAKERS: [&str; 6] = ["Off", "122", "147", "145", "Vibratone", "122 Rear"];
pub const TUNINGS: [&str; 2] = ["Tonewheel", "Pure"];
pub const BANKS: [&str; 3] = ["Dual", "Fast", "Clean"];

/// Percussion of a register: (harmonic 2 / 3, fast, soft).
type Perc = Option<(u32, bool, bool)>;

/// A register's voicing (README §2.7).
#[derive(Clone, Copy, Debug)]
pub struct Register {
    pub drawbars: [u8; 9],
    pub perc: Perc,
    /// Leslie depth (gfx-rotary `depth`, %) at `mod` 50 %; 0 = none
    pub leslie_depth: f32,
    /// scanner: `vibrato` option index (0 off, 1 V1, 2 C1, 3 V2, 4 C2, 5 V3, 6 C3)
    pub scanner: usize,
    /// speaker: `speaker` option index (0 Off, 1 122, 2 147 …); parked: rotors stopped
    pub speaker: usize,
    pub parked: bool,
    /// Leslie drive (%), attack (ms), `strings` floor (0 – 1)
    pub drive: f32,
    pub attack_ms: f32,
    pub strings_floor: f64,
}

#[allow(clippy::too_many_arguments)]
const fn reg(drawbars: [u8; 9], perc: Perc, leslie_depth: f32, scanner: usize, speaker: usize, parked: bool, drive: f32, attack_ms: f32, strings_floor: f64) -> Register {
    Register { drawbars, perc, leslie_depth, scanner, speaker, parked, drive, attack_ms, strings_floor }
}

/// The nine registers in their order (Custom is the tenth, from the expert panel).
pub const REGISTER_TABLE: [Register; 9] = [
    // Tutti: 88 8800 008, Leslie 60 %, 122, drive 20 %
    reg([8, 8, 8, 8, 0, 0, 0, 0, 8], None, 60.0, 0, 1, false, 20.0, 0.0, 0.0),
    // Trio: 88 8000 000, 3rd fast normal, Leslie 60 %, 122, 10 %
    reg([8, 8, 8, 0, 0, 0, 0, 0, 0], Some((3, true, false)), 60.0, 0, 1, false, 10.0, 0.0, 0.0),
    // Choir: 66 8848 588, Leslie 70 % + scanner C3, 122, 30 %
    reg([6, 6, 8, 8, 4, 8, 5, 8, 8], None, 70.0, 6, 1, false, 30.0, 0.0, 0.0),
    // Bark: 88 8800 000, 2nd fast normal, Leslie 70 %, 147, 60 %
    reg([8, 8, 8, 8, 0, 0, 0, 0, 0], Some((2, true, false)), 70.0, 0, 2, false, 60.0, 0.0, 0.0),
    // Nave: 80 8808 008, scanner V1, no Leslie, attack 180 ms
    reg([8, 0, 8, 8, 0, 8, 0, 0, 8], None, 0.0, 1, 0, false, 0.0, 180.0, 0.0),
    // Chime: 00 8000 808, 3rd slow normal, scanner C2, 122 parked
    reg([0, 0, 8, 0, 0, 0, 8, 0, 8], Some((3, false, false)), 0.0, 4, 1, true, 0.0, 0.0, 0.0),
    // Velvet: 80 3600 000, Leslie 40 %, 122, 0 %, attack 20 ms
    reg([8, 0, 3, 6, 0, 0, 0, 0, 0], None, 40.0, 0, 1, false, 0.0, 20.0, 0.0),
    // Combo: 00 8030 200 + strings 20 %, scanner V2, no Leslie
    reg([0, 0, 8, 0, 3, 0, 2, 0, 0], None, 0.0, 3, 0, false, 0.0, 0.0, 0.2),
    // Coupler: 00 8800 000, 2nd fast soft, Leslie 50 %, 122, 0 %
    reg([0, 0, 8, 8, 0, 0, 0, 0, 0], Some((2, true, true)), 50.0, 0, 1, false, 0.0, 0.0, 0.0),
];

/// Drawbar position → gain: 3 dB per step, 8 = 0 dB, 0 = out (§1.2, open question 6).
pub fn drawbar_gain(n: u8) -> f64 {
    if n == 0 {
        0.0
    } else {
        10f64.powf(-3.0 * (8.0 - n.min(8) as f64) / 20.0)
    }
}

/// Percussion: τ = T60 / 6.91 of setBfree's 1 s (fast) and 4 s (slow).
pub const PERC_TAU_FAST: f64 = 1.0 / 6.91;
pub const PERC_TAU_SLOW: f64 = 4.0 / 6.91;
/// Normal's volume drop on the drawbars (setBfree 0.605, −4.4 dB); Soft −6 dB on the percussion;
/// Fast +3.5 dB on the percussion.
pub const NORMAL_DROP: f64 = 0.605;
pub const SOFT_GAIN: f64 = 0.501;
pub const FAST_GAIN: f64 = 1.496;
/// `touch`'s reference body level (−18 dBFS) and its range (±24 dB).
const P_REF: f64 = 0.125_892_541;
const TOUCH_RANGE_DB: f64 = 24.0;
/// The scanner's rate (Hz): the instrument's fixed ≈ 6.9 Hz.
pub const SCANNER_HZ: f64 = 6.9;
/// The control tick (ms).
const CTRL_MS: f64 = 1.0;
/// The pick detector listens down to `thresh` − 24 dB.
const PICK_UNDER_DB: f64 = 24.0;

fn db_to_lin(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

fn idx(v: f32, n: usize) -> usize {
    (v.round().max(0.0) as usize).min(n - 1)
}

// ---------------------------------------------------------------- the ceiling (gfx-polysynth's)

/// Transparent under −6 dBFS, 0.5 + 0.5 tanh(2 (|x| − 0.5)) above (ADAA1), never past 0 dBFS.
#[derive(Clone, Debug, Default)]
struct Ceiling {
    x1: f64,
}

impl Ceiling {
    #[inline]
    fn f(x: f64) -> f64 {
        let a = x.abs();
        if a <= 0.5 {
            x
        } else {
            (0.5 + 0.5 * (2.0 * (a - 0.5)).tanh()).copysign(x)
        }
    }
    #[inline]
    fn big_f(x: f64) -> f64 {
        let a = x.abs();
        if a <= 0.5 {
            0.5 * x * x
        } else {
            let u = 2.0 * (a - 0.5);
            let lc = u + (-2.0 * u).exp().ln_1p() - std::f64::consts::LN_2;
            0.125 + 0.5 * (a - 0.5) + 0.25 * lc
        }
    }
    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let x1 = self.x1;
        self.x1 = x;
        if x.abs() <= 0.5 && x1.abs() <= 0.5 {
            return x;
        }
        let d = x - x1;
        let y = if d.abs() < 1e-9 { Self::f(0.5 * (x + x1)) } else { (Self::big_f(x) - Self::big_f(x1)) / d };
        y.clamp(-1.0, 1.0)
    }
}

/// The resolved voicing of a control tick (register or Custom).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Voicing {
    drawbars: [u8; 9],
    perc: Perc,
    leslie_depth: f32,
    scanner: usize,
    speaker: usize,
    parked: bool,
    drive: f32,
    attack_ms: f32,
    strings_floor: f64,
}

pub struct Organ {
    v: [f32; NP],
    sr: f32,
    nch: usize,
    factor: usize,
    down: OversamplerN,
    up: OversamplerN,
    dbuf: [f64; MAX_FACTOR],
    ua: [f64; MAX_FACTOR],
    dn: usize,
    hpf: Svf,
    core: Core<OrganLaw>,
    // detection
    pick: Pick,
    lvl: SlidingMax,
    lvl_now: f64,
    pick_flag: bool,
    strength_peak: f64,
    strength_ref: f64,
    strength_until: u64,
    clock: u64,
    ctrl_n: u64,
    // smoothed parameters
    organ: ExpSmoother,
    dry: ExpSmoother,
    modp: ExpSmoother,
    drive: ExpSmoother,
    strings: ExpSmoother,
    hold: ExpSmoother,
    swell_p: ExpSmoother,
    ramp: ExpSmoother,
    touch_g: OnePole,
    // swell
    swell: f64,
    // click burst
    noise: White,
    burst_bp: [Svf; 2],
    burst_n: u64,
    burst_len: u64,
    burst_amp: f64,
    tail_ph: f64,
    tail_w: f64,
    tail_amp: f64,
    tail_k: f64,
    // scanner
    line: DelayLine,
    scanner: Scanner,
    scan_ph: f64,
    scan_depth: OnePole,
    scan_mix: OnePole,
    scan_on: OnePole,
    // Leslie
    rotary: Rotary,
    rot_s: rotary::Settings,
    rot_on: OnePole,
    rbuf: [Vec<f32>; 2],
    // ceiling, bypass
    ceil: [Ceiling; 2],
    byp: bool,
    byp_x: f64,
    byp_step: f64,
    voicing: Voicing,
    // meters
    m_state: f32,
    m_roots: f32,
}

impl Default for Organ {
    fn default() -> Self {
        Self::new()
    }
}

impl Organ {
    pub fn new() -> Self {
        let v: [f32; NP] = core::array::from_fn(|i| PARAMS[i].default);
        let smooth = |i: usize| ExpSmoother::new(PARAMS[i].min, PARAMS[i].max, PARAMS[i].default);
        let mut o = Self {
            v,
            sr: 48000.0,
            nch: 1,
            factor: 2,
            down: OversamplerN::new(1),
            up: OversamplerN::new(1),
            dbuf: [0.0; MAX_FACTOR],
            ua: [0.0; MAX_FACTOR],
            dn: 0,
            hpf: Svf::new(),
            core: Core::new(),
            pick: Pick::default(),
            lvl: SlidingMax::new(),
            lvl_now: 0.0,
            pick_flag: false,
            strength_peak: 0.0,
            strength_ref: 0.0,
            strength_until: 0,
            clock: 0,
            ctrl_n: 48,
            organ: smooth(p::ORGAN),
            dry: smooth(p::DRY),
            modp: smooth(p::MOD),
            drive: smooth(p::DRIVE),
            strings: smooth(p::STRINGS),
            hold: smooth(p::HOLD),
            swell_p: smooth(p::SWELL),
            ramp: smooth(p::RAMP),
            touch_g: OnePole::new(10.0, 1.0, 1e-6),
            swell: 1.0,
            noise: White::new(0xC11C_0002),
            burst_bp: [Svf::new(), Svf::new()],
            burst_n: u64::MAX / 2,
            burst_len: 192,
            burst_amp: 0.0,
            tail_ph: 0.0,
            tail_w: 0.0,
            tail_amp: 0.0,
            tail_k: 0.0,
            line: DelayLine::new(),
            scanner: Scanner::new(),
            scan_ph: 0.0,
            scan_depth: OnePole::new(20.0, 0.0, 1e-6),
            scan_mix: OnePole::new(20.0, 1.0, 1e-6),
            scan_on: OnePole::new(20.0, 0.0, 1e-6),
            rotary: Rotary::new(),
            rot_s: rotary::Settings::default(),
            rot_on: OnePole::new(20.0, 1.0, 1e-6),
            rbuf: [Vec::new(), Vec::new()],
            ceil: Default::default(),
            byp: false,
            byp_x: 0.0,
            byp_step: 0.0,
            voicing: Self::voicing_of(&v),
            m_state: 0.0,
            m_roots: 0.0,
        };
        o.apply_law();
        o
    }

    fn b(&self, i: usize) -> bool {
        self.v[i] >= 0.5
    }

    /// The register's voicing, or Custom's from the expert panel.
    fn voicing_of(v: &[f32; NP]) -> Voicing {
        let r = idx(v[p::REGISTER], 10);
        if r < 9 {
            let g = REGISTER_TABLE[r];
            Voicing { drawbars: g.drawbars, perc: g.perc, leslie_depth: g.leslie_depth, scanner: g.scanner, speaker: g.speaker, parked: g.parked, drive: g.drive, attack_ms: g.attack_ms, strings_floor: g.strings_floor }
        } else {
            let drawbars: [u8; 9] = core::array::from_fn(|i| v[p::DB16 + i].round().clamp(0.0, 8.0) as u8);
            let perc = if v[p::PERC] >= 0.5 { Some((if idx(v[p::PERC_HARM], 2) == 0 { 2 } else { 3 }, idx(v[p::PERC_DECAY], 2) == 1, idx(v[p::PERC_VOL], 2) == 1)) } else { None };
            let speaker = idx(v[p::SPEAKER], 6);
            Voicing { drawbars, perc, leslie_depth: 60.0, scanner: idx(v[p::VIBRATO], 7), speaker, parked: false, drive: 0.0, attack_ms: 0.0, strings_floor: 0.0 }
        }
    }

    /// The law's settings from the voicing (drawbars, percussion, its 1' mute and volume drop).
    fn apply_law(&mut self) {
        let vo = self.voicing;
        let mut d: [f64; 9] = core::array::from_fn(|i| drawbar_gain(vo.drawbars[i]));
        let mut s = self.core.shaper.s;
        match vo.perc {
            Some((h, fast, soft)) => {
                // percussion disconnects the 1' drawbar; Normal lowers the organ by 4.4 dB
                d[8] = 0.0;
                if !soft {
                    d.iter_mut().for_each(|x| *x *= NORMAL_DROP);
                }
                s.perc = true;
                s.perc_h = h;
                s.perc_tau = if fast { PERC_TAU_FAST } else { PERC_TAU_SLOW };
                s.perc_gain = (if soft { SOFT_GAIN } else { 1.0 }) * if fast { FAST_GAIN } else { 1.0 };
            }
            None => s.perc = false,
        }
        s.d = d;
        s.perc_single = idx(self.v[p::PERC_TRIG], 2) == 1;
        s.tonewheel = idx(self.v[p::TUNING], 2) == 0;
        s.click = (self.v[p::CLICK] as f64 * 0.01).clamp(0.0, 1.0);
        self.core.shaper.s = s;
    }

    /// The `state` meter: 0 silence · 1 attack (undecided partials only) · 2 organ (roots sounding).
    pub fn state(&self) -> f32 {
        self.m_state
    }

    /// The `roots` meter: confirmed roots (0 – 12).
    pub fn roots(&self) -> f32 {
        self.m_roots
    }

    /// The core (tests and the bench).
    pub fn core(&self) -> &Core<OrganLaw> {
        &self.core
    }

    pub fn core_mut(&mut self) -> &mut Core<OrganLaw> {
        &mut self.core
    }

    fn control(&mut self) {
        let v = self.v;
        let sr = self.sr as f64;
        let now = self.clock;
        // ---- voicing and the law
        let vo = Self::voicing_of(&v);
        self.voicing = vo;
        self.apply_law();
        let strings = (self.strings.value() as f64 * 0.01).max(vo.strings_floor);
        self.core.shaper.s.strings = strings;
        // ---- bypass
        let bypass = self.b(p::BYPASS);
        if bypass != self.byp {
            self.byp = bypass;
            let len = if bypass && self.b(p::TRAILS) { v[p::RELEASE] as f64 * 1e-3 } else { 0.010 };
            self.byp_step = 1.0 / (len * sr).max(1.0);
        }
        // ---- a pick: the core to the fast bank, percussion and click restart
        let was_silent = self.core.shaper.state() == 0;
        if std::mem::take(&mut self.pick_flag) && !bypass {
            self.core.pick();
            self.strength_peak = self.lvl_now;
            self.strength_until = now + (0.010 * sr) as u64;
            self.burst_n = 0;
            // the swell restarts only when the organ had stopped
            if was_silent {
                self.swell = 0.0;
            }
            // the tail: the 6th harmonic of the loudest root or undecided band
            let f = self.core.shaper.loudest_hz();
            self.tail_w = if f > 0.0 && 6.0 * f < 0.45 * sr { TAU * 6.0 * f / sr } else { 0.0 };
            self.tail_ph = 0.0;
        }
        if now <= self.strength_until {
            self.strength_peak = self.strength_peak.max(self.lvl_now);
            if self.strength_peak > self.strength_ref {
                self.strength_ref = self.strength_peak;
            }
            let s = if self.strength_ref > 0.0 { (self.strength_peak / self.strength_ref).clamp(0.0, 1.0) } else { 1.0 };
            let click = v[p::CLICK] as f64 * 0.01;
            // burst: 4 ms of band-passed noise; tail −20 dB, 70 ms (τ 10 ms)
            self.burst_amp = 2.0 * P_REF * click * s;
            self.tail_amp = 0.1 * self.burst_amp;
        }
        self.strength_ref *= (-CTRL_MS * 1e-3 / 2.0).exp();
        // ---- the core's analysis settings
        let bank = match idx(v[p::BANK], 3) {
            1 => BankMode::Fast,
            2 => BankMode::Clean,
            _ => BankMode::Dual,
        };
        let hold = (self.hold.value() as f64 * 0.01).clamp(0.0, 1.0);
        self.core.set(CoreSettings { bank, compress: hold, thresh_db: v[p::THRESH] as f64, lowest: v[p::LOWEST] as f64, mono: 0.0, residue: 1.0, release: (v[p::RELEASE] as f64 * 1e-3).max(0.005) });
        // ---- touch: (P_ref / P_max)^(1 − touch), a global gain
        let touch = (v[p::TOUCH] as f64 * 0.01).clamp(0.0, 1.0);
        let pm = self.core.shaper.p_max();
        let tg = if pm > 1e-9 { ((1.0 - touch) * 20.0 * (P_REF / pm).log10()).clamp(-TOUCH_RANGE_DB, TOUCH_RANGE_DB) } else { 0.0 };
        self.touch_g.set_target(db_to_lin(tg) as f32);
        // ---- modulation
        let modr = (self.modp.value() as f64 / 50.0).clamp(0.0, 2.0);
        let (sdepth, smix) = match vo.scanner {
            0 => (0.0, 1.0),
            k => (V_DEPTH[(k - 1) / 2] * modr, if k.is_multiple_of(2) { 0.5 } else { 1.0 }),
        };
        self.scan_on.set_target(if vo.scanner > 0 { 1.0 } else { 0.0 });
        if vo.scanner > 0 {
            self.scan_depth.set_target(sdepth.min(1.0) as f32);
            self.scan_mix.set_target(smix as f32);
        }
        let leslie = vo.speaker > 0;
        self.rot_on.set_target(if leslie { 1.0 } else { 0.0 });
        if leslie {
            let speed = if vo.parked { 0.0 } else { idx(v[p::LESLIE], 3) as f32 };
            let depth = (vo.leslie_depth as f64 * modr).min(100.0) as f32;
            let drive = (vo.drive + self.drive.value()).min(100.0);
            let s = self.rot_s.with(rotary::p::SPEED, speed).with(rotary::p::CABINET, (vo.speaker - 1) as f32).with(rotary::p::DEPTH, depth).with(rotary::p::DRIVE, drive).with(rotary::p::RAMP, self.ramp.value()).with(rotary::p::SPACING, v[p::SPACING]).with(rotary::p::MIX, 100.0);
            if s != self.rot_s {
                self.rot_s = s;
                self.rotary.set(s);
            }
        }
        // ---- meters
        self.m_state = self.core.shaper.state() as f32;
        self.m_roots = self.core.shaper.roots().min(12) as f32;
    }

    /// One host sample → the organ bus (before scanner and Leslie), at the host rate.
    #[inline]
    fn organ_sample(&mut self, x: f32) -> f64 {
        let sr = self.sr as f64;
        // detection
        let l = self.lvl.next(x.abs()) as f64;
        self.lvl_now = l;
        let thresh_db = self.v[p::THRESH] as f64;
        if self.pick.next(x, l >= db_to_lin(thresh_db - PICK_UNDER_DB)) {
            self.pick_flag = true;
        }
        if self.clock.is_multiple_of(self.ctrl_n) {
            self.control();
        }
        self.clock += 1;
        // the bank path
        let h = self.hpf.process(x) as f64;
        self.dbuf[self.dn] = h;
        let y = self.ua[self.dn];
        self.dn += 1;
        if self.dn == self.factor {
            self.dn = 0;
            let mut buf = self.dbuf;
            let d = if self.factor > 1 { self.down.down(&mut buf) } else { buf[0] };
            let (a, _) = self.core.process(d);
            if self.factor > 1 {
                self.up.up(a, &mut self.ua);
            } else {
                self.ua[0] = a;
            }
        }
        // touch, swell (the register's attack plus `swell`)
        let mut out = y * self.touch_g.next() as f64;
        let att = (self.voicing.attack_ms as f64 + self.swell_p.next() as f64) * 1e-3;
        if self.swell < 1.0 {
            self.swell = if att > 1e-4 { (self.swell + 1.0 / (att * sr)).min(1.0) } else { 1.0 };
        }
        out *= self.swell;
        // the click's burst and tail
        if self.burst_n < self.burst_len {
            let nz = self.noise.next_f64();
            let w = 0.5 - 0.5 * (TAU * self.burst_n as f64 / self.burst_len as f64).cos();
            let b0 = self.burst_bp[0].process((nz * w) as f32);
            let b1 = self.burst_bp[1].process(b0);
            out += self.burst_amp * b1 as f64;
            if self.burst_n == 0 {
                self.tail_amp = 0.1 * self.burst_amp;
            }
        }
        if self.burst_n < u64::MAX / 2 {
            self.burst_n += 1;
        }
        if self.tail_w > 0.0 && self.tail_amp > 1e-7 {
            self.tail_ph += self.tail_w;
            if self.tail_ph > TAU {
                self.tail_ph -= TAU;
            }
            out += self.tail_amp * self.tail_ph.sin();
            self.tail_amp *= self.tail_k;
        }
        out
    }

    fn tick_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(self.nch.max(1));
        let n = buffers[0].len();
        if self.rbuf[0].len() < n {
            // only if a host sends a block longer than announced (prepare sizes them)
            self.rbuf = [vec![0.0; n], vec![0.0; n]];
        }
        // organ bus → scanner, at the host rate, into rbuf
        let sr = self.sr as f64;
        let guard = 4.0 / sr;
        for i in 0..n {
            let x = if nch == 2 { 0.5 * (buffers[0][i] + buffers[1][i]) } else { buffers[0][i] };
            let y = self.organ_sample(x);
            let og = self.organ.next() as f64;
            let g = if og <= -59.999 { 0.0 } else { db_to_lin(og) };
            let mut z = (y * g) as f32;
            // scanner vibrato / chorus
            self.line.push(z);
            let on = self.scan_on.next();
            let depth = self.scan_depth.next() as f64;
            let mix = self.scan_mix.next();
            self.scan_ph += SCANNER_HZ / sr;
            if self.scan_ph >= 1.0 {
                self.scan_ph -= 1.0;
            }
            if on > 0.0 {
                let w = (TAU * self.scan_ph).sin();
                let wet = self.scanner.process(&self.line, depth, w, guard, sr);
                let sc = z + (wet - z) * mix;
                z += (sc - z) * on;
            }
            self.rbuf[0][i] = z;
            self.rbuf[1][i] = z;
        }
        // the Leslie (stereo, at the host rate)
        let rot_on = self.rot_on.value();
        if rot_on > 0.0 || self.rot_on.target() > 0.0 {
            let (a, b) = self.rbuf.split_at_mut(1);
            let l = &mut a[0][..n];
            let r = &mut b[0][..n];
            // keep the unrotated bus for the crossfade in r's place only when fading
            if rot_on >= 1.0 && self.rot_on.target() >= 1.0 {
                self.rotary.process_block(&mut [l, r]);
                for _ in 0..n {
                    self.rot_on.next();
                }
            } else {
                let mut tmp = [0.0f32; 1024];
                let mut k = 0;
                while k < n {
                    let e = (k + 1024).min(n);
                    let m = e - k;
                    tmp[..m].copy_from_slice(&l[k..e]);
                    self.rotary.process_block(&mut [&mut l[k..e], &mut r[k..e]]);
                    for j in 0..m {
                        let g = self.rot_on.next();
                        l[k + j] = tmp[j] + (l[k + j] - tmp[j]) * g;
                        r[k + j] = tmp[j] + (r[k + j] - tmp[j]) * g;
                    }
                    k = e;
                }
            }
        } else {
            for _ in 0..n {
                self.rot_on.next();
            }
        }
        // ceiling, bypass, dry
        for i in 0..n {
            let d = self.dry.next() as f64;
            let dg = if d <= -59.999 { 0.0 } else { db_to_lin(d) };
            self.byp_x = if self.byp { (self.byp_x + self.byp_step).min(1.0) } else { (self.byp_x - self.byp_step).max(0.0) };
            let dry = (dg * (1.0 - self.byp_x) + self.byp_x) as f32;
            let wet_g = (1.0 - self.byp_x) as f32;
            let yl = self.ceil[0].process(self.rbuf[0][i] as f64) as f32 * wet_g;
            if nch == 2 {
                let yr = self.ceil[1].process(self.rbuf[1][i] as f64) as f32 * wet_g;
                buffers[0][i] = buffers[0][i] * dry + yl;
                buffers[1][i] = buffers[1][i] * dry + yr;
            } else {
                // 1 / 1: the mics summed
                let yr = self.ceil[1].process(self.rbuf[1][i] as f64) as f32 * wet_g;
                buffers[0][i] = buffers[0][i] * dry + 0.5 * (yl + yr);
            }
        }
    }
}

impl Effect for Organ {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, max_block: usize, channels: usize) {
        self.sr = sample_rate;
        self.nch = channels.clamp(1, 2);
        let srd = sample_rate as f64;
        self.ctrl_n = (CTRL_MS * 1e-3 * srd).round().max(1.0) as u64;
        for e in [&mut self.organ, &mut self.dry, &mut self.modp, &mut self.drive, &mut self.strings, &mut self.hold, &mut self.swell_p, &mut self.ramp] {
            e.prepare(sample_rate);
        }
        for o in [&mut self.touch_g, &mut self.scan_depth, &mut self.scan_mix, &mut self.scan_on, &mut self.rot_on] {
            o.prepare(sample_rate);
        }
        // the bank rate: 22.05 – 24 kHz at every host rate (÷ 2, ÷ 4, ÷ 8)
        let stages = (srd / 24000.0).log2().round().clamp(0.0, 3.0) as usize;
        self.factor = 1 << stages;
        self.down = OversamplerN::new(stages);
        self.up = OversamplerN::new(stages);
        self.core.prepare(srd / self.factor as f64);
        self.hpf.set(SvfMode::HighPass, 25.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, sample_rate);
        self.lvl.prepare(((srd / 25.0).ceil() as usize + 2).max(4));
        self.lvl.set_window(((srd / 25.0).ceil() as usize).max(1));
        self.pick.prepare(sample_rate);
        for f in self.burst_bp.iter_mut() {
            f.set(SvfMode::BandPass, 2000.0f32.min(0.4 * sample_rate), 0.7, 0.0, sample_rate);
        }
        self.burst_len = (0.004 * srd) as u64;
        self.tail_k = (-1.0 / (0.010 * srd)).exp();
        // the scanner's line: 19 taps × 55 µs + guard, sized for 192 kHz
        self.line.prepare((0.002 * srd) as usize + 16);
        self.scanner.prepare(sample_rate);
        self.rotary.prepare(sample_rate, max_block, 2);
        let mb = max_block.max(1);
        self.rbuf = [vec![0.0; mb], vec![0.0; mb]];
        self.reset();
    }

    fn reset(&mut self) {
        let v = self.v;
        for (i, e) in [(p::ORGAN, &mut self.organ), (p::DRY, &mut self.dry), (p::MOD, &mut self.modp), (p::DRIVE, &mut self.drive), (p::STRINGS, &mut self.strings), (p::HOLD, &mut self.hold), (p::SWELL, &mut self.swell_p), (p::RAMP, &mut self.ramp)] {
            e.settle(v[i]);
        }
        self.voicing = Self::voicing_of(&v);
        self.apply_law();
        self.core.reset();
        self.down.reset();
        self.up.reset();
        self.dbuf = [0.0; MAX_FACTOR];
        self.ua = [0.0; MAX_FACTOR];
        self.dn = 0;
        self.hpf.reset();
        self.pick.reset();
        self.lvl.reset();
        self.lvl_now = 0.0;
        self.pick_flag = false;
        self.strength_peak = 0.0;
        self.strength_ref = 0.0;
        self.strength_until = 0;
        self.clock = 0;
        self.touch_g.set_immediate(1.0);
        self.swell = 1.0;
        self.noise.reset();
        for f in self.burst_bp.iter_mut() {
            f.reset();
        }
        self.burst_n = u64::MAX / 2;
        self.burst_amp = 0.0;
        self.tail_ph = 0.0;
        self.tail_w = 0.0;
        self.tail_amp = 0.0;
        self.line.reset();
        self.scanner.reset();
        self.scan_ph = 0.0;
        let vo = self.voicing;
        let modr = (v[p::MOD] as f64 / 50.0).clamp(0.0, 2.0);
        self.scan_on.set_immediate(if vo.scanner > 0 { 1.0 } else { 0.0 });
        if vo.scanner > 0 {
            self.scan_depth.set_immediate((V_DEPTH[(vo.scanner - 1) / 2] * modr).min(1.0) as f32);
            self.scan_mix.set_immediate(if vo.scanner.is_multiple_of(2) { 0.5 } else { 1.0 });
        }
        self.rot_on.set_immediate(if vo.speaker > 0 { 1.0 } else { 0.0 });
        if vo.speaker > 0 {
            let speed = if vo.parked { 0.0 } else { idx(v[p::LESLIE], 3) as f32 };
            let s = rotary::Settings::default()
                .with(rotary::p::SPEED, speed)
                .with(rotary::p::CABINET, (vo.speaker - 1) as f32)
                .with(rotary::p::DEPTH, (vo.leslie_depth as f64 * modr).min(100.0) as f32)
                .with(rotary::p::DRIVE, (vo.drive + v[p::DRIVE]).min(100.0))
                .with(rotary::p::RAMP, v[p::RAMP])
                .with(rotary::p::SPACING, v[p::SPACING])
                .with(rotary::p::MIX, 100.0);
            self.rot_s = s;
            self.rotary.set(s);
        }
        self.rotary.reset();
        self.ceil = Default::default();
        self.byp = self.b(p::BYPASS);
        self.byp_x = if self.byp { 1.0 } else { 0.0 };
        self.m_state = 0.0;
        self.m_roots = 0.0;
    }

    fn set_param(&mut self, id: &str, value: f32) {
        if let Some(i) = PARAMS.iter().position(|pi| pi.id == id) {
            let v = value.clamp(PARAMS[i].min, PARAMS[i].max);
            self.v[i] = v;
            let now = self.clock;
            match i {
                p::ORGAN => self.organ.set_target(v, now),
                p::DRY => self.dry.set_target(v, now),
                p::MOD => self.modp.set_target(v, now),
                p::DRIVE => self.drive.set_target(v, now),
                p::STRINGS => self.strings.set_target(v, now),
                p::HOLD => self.hold.set_target(v, now),
                p::SWELL => self.swell_p.set_target(v, now),
                p::RAMP => self.ramp.set_target(v, now),
                _ => {}
            }
        }
    }

    fn meter(&self, id: &str) -> Option<f32> {
        match id {
            "state" => Some(self.m_state),
            "roots" => Some(self.m_roots),
            _ => None,
        }
    }

    fn latency(&self) -> usize {
        0
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        if buffers.is_empty() {
            return;
        }
        let n = buffers[0].len();
        let mb = self.rbuf[0].len().max(1);
        let mut k = 0;
        while k < n {
            let e = (k + mb).min(n);
            if buffers.len() >= 2 && self.nch == 2 {
                let (a, b) = buffers.split_at_mut(1);
                self.tick_block(&mut [&mut a[0][k..e], &mut b[0][k..e]]);
            } else {
                self.tick_block(&mut [&mut buffers[0][k..e]]);
            }
            k = e;
        }
    }
}
