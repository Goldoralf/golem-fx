// SPDX-License-Identifier: GPL-3.0-or-later
//! The device tables (README §2.1 – §2.6). **Every value is a voicing starting point [I]**, to be fixed
//! against measured devices (README §7.7). Levels are in dB re the reference programme level
//! (−18 dBFS RMS); frequencies in Hz.

use gfx_dsp::speaker::{MotorParams, Role, Section, Shape};

/// The microphone (README §2.2). `mic` option k (1 – 5) is `Mic` k; `None` is a model without a mic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mic {
    None,
    Clean,
    Carbon,
    Dynamic,
    Crystal,
    Electret,
}

impl Mic {
    pub fn from_option(i: usize) -> Mic {
        [Mic::None, Mic::Clean, Mic::Carbon, Mic::Dynamic, Mic::Crystal, Mic::Electret][i.min(5)]
    }
}

/// The transmission (README §2.3). `channel` option k (1 – 6) is `Chan` k − 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chan {
    None,
    PhoneLine,
    DigitalLine,
    Cell,
    Am,
    Nbfm,
}

impl Chan {
    pub fn from_option(i: usize) -> Chan {
        [Chan::None, Chan::None, Chan::PhoneLine, Chan::DigitalLine, Chan::Cell, Chan::Am, Chan::Nbfm][i.min(6)]
    }
}

/// The receiver amplifier (README §2.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmpKind {
    /// line-driven: a gentle driver tanh only
    Line,
    /// smart amp: peak limiter (+ bass enhancer on the laptop)
    Smart,
    /// class-B transistor: crossover distortion and a hard rail
    ClassB,
    /// class-AB: a hard rail, little crossover
    ClassAb,
    /// small IC amp: a hard rail
    Ic,
    /// single-ended tube: asymmetric tanh
    Tube,
    /// tube PA amp and its 100 V line transformer (low band saturates)
    TubePa,
    /// the gramophone's soundbox overload: strongly asymmetric
    Soundbox,
}

impl AmpKind {
    /// Hard-rail amps run their island at 4×, the others at 2× (README §2.4): oversampling stages. The
    /// gramophone's soundbox overload joins the 4× group: at 2× its worst alias was −66 dB at `drive`
    /// 100 % (bound −70 dB).
    pub fn stages(self) -> usize {
        match self {
            AmpKind::ClassB | AmpKind::ClassAb | AmpKind::Ic | AmpKind::Soundbox => 2,
            _ => 1,
        }
    }
    /// Whether `battery` acts (battery-powered transistor amps).
    pub fn battery(self) -> bool {
        matches!(self, AmpKind::ClassB | AmpKind::ClassAb | AmpKind::Ic)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Amp {
    pub kind: AmpKind,
    /// the rail (or tanh knee), full-scale units at the amp's input
    pub rail: f64,
    /// crossover width (class-B)
    pub cross: f64,
    /// tanh bias (tube, soundbox)
    pub bias: f64,
    /// the model's own noise at the amp: hiss, hum (dB re reference; −200 = none), crackle (per s)
    pub hiss_db: f64,
    pub hum_db: f64,
    pub crackle: f64,
    /// the laptop's bass enhancer (missing-fundamental harmonics), 0 = off
    pub enhance: f64,
}

/// What a speaker's `distance` range is (README §2.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cat {
    Handset,
    Horn,
    Other,
}

#[derive(Clone, Copy, Debug)]
pub struct Speaker {
    pub motor: MotorParams,
    /// drive into the motor (X_max per full-scale unit)
    pub gx: f64,
    /// horn throat: (η, upper edge)
    pub throat: Option<(f64, f64)>,
    pub body: &'static [Section],
    pub cat: Cat,
}

const fn bell(hz: f64, q: f64, db: f64) -> Section {
    Section::new(Shape::Bell(q, db), hz, Role::Bell)
}
const fn hp(hz: f64, q: f64) -> Section {
    Section::new(Shape::HighPass(q), hz, Role::Low)
}
const fn lp(hz: f64, q: f64) -> Section {
    Section::new(Shape::LowPass(q), hz, Role::High)
}
const fn lp_fixed(hz: f64, q: f64) -> Section {
    Section::new(Shape::LowPass(q), hz, Role::Fixed)
}

const fn motor(fs_hz: f64, q_ts: f64, beta: f64, x_b: f64, kappa1: f64, kappa: f64, kappa3: f64) -> MotorParams {
    MotorParams { fs_hz, q_ts, q_ms: 4.0, beta, x_b, kappa1, kappa, kappa3 }
}

/// The speaker voicings, by `speaker` option (0 = None is not a voicing: index 0 unused), plus the cell
/// phone's own receiver at [`PHONE_RECEIVER`] (a model voicing, not an option).
pub const PHONE_RECEIVER: usize = 10;

pub static SPEAKERS: [Speaker; 11] = [
    // 0: unused (None)
    Speaker { motor: MotorParams::linear(100.0, 0.7, 4.0), gx: 1.0, throat: None, body: &[], cat: Cat::Other },
    // 1 Handset earpiece: 300 – 3,400 Hz, peaks ≈ 1 kHz and 2.6 kHz. f_s and the bells are fitted so the
    // whole Telephone chain (carbon mic, line, earpiece) meets the README's −3 dB edges (300 / 3,400 Hz)
    Speaker {
        motor: motor(290.0, 1.33, 0.4, 0.0, 0.0, 1.5, 10.0),
        gx: 1.0,
        throat: None,
        body: &[bell(1000.0, 1.2, -1.8), bell(2600.0, 1.64, 5.6), lp(3280.0, 4.2)],
        cat: Cat::Handset,
    },
    // 2 Megaphone re-entrant horn: 500 – 4,500 Hz, peaks ≈ 1.2 / 2.4 kHz, throat nonlinearity
    Speaker {
        motor: motor(450.0, 0.9, 0.4, 0.0, 0.0, 2.0, 20.0),
        gx: 1.2,
        throat: Some((0.1, 4500.0)),
        body: &[hp(500.0, 0.8), bell(1200.0, 3.0, 6.0), bell(2400.0, 3.0, 5.0)],
        cat: Cat::Horn,
    },
    // 3 Walkie 40 mm speaker: 450 – 4,000 Hz, box resonance ≈ 650 Hz
    Speaker {
        motor: motor(650.0, 1.4, 0.45, 0.05, 0.1, 2.5, 30.0),
        gx: 0.9,
        throat: None,
        body: &[hp(400.0, 0.7), bell(2500.0, 1.5, 3.0), lp(4000.0, 0.9)],
        cat: Cat::Other,
    },
    // 4 Table radio, 5″ in an open-back cabinet: 150 – 4,500 Hz, cabinet bump ≈ 200 Hz
    Speaker {
        motor: motor(120.0, 0.8, 0.3, 0.02, 0.05, 1.0, 5.0),
        gx: 1.5,
        throat: None,
        body: &[hp(140.0, 0.7), bell(200.0, 1.5, 4.0), bell(1500.0, 1.0, 2.0), lp(4500.0, 0.8), lp_fixed(6500.0, 0.6)],
        cat: Cat::Other,
    },
    // 5 PA reflex trumpet: 400 – 6,000 Hz, strong horn peaks
    Speaker {
        motor: motor(350.0, 1.0, 0.4, 0.0, 0.0, 2.0, 20.0),
        gx: 1.2,
        throat: Some((0.125, 6000.0)),
        body: &[hp(400.0, 0.9), bell(900.0, 4.0, 6.0), bell(1800.0, 4.0, 7.0), bell(3200.0, 4.0, 5.0)],
        cat: Cat::Horn,
    },
    // 6 Intercom 50 mm speaker in a metal box: 500 – 4,000 Hz, box ring ≈ 1.5 kHz
    Speaker {
        motor: motor(500.0, 1.2, 0.45, 0.03, 0.1, 2.5, 30.0),
        gx: 0.9,
        throat: None,
        body: &[hp(450.0, 0.7), bell(1500.0, 6.0, 8.0), bell(3000.0, 3.0, 3.0), lp(4000.0, 1.0)],
        cat: Cat::Other,
    },
    // 7 Pocket radio 57 mm speaker: 250 – 6,000 Hz, resonance ≈ 350 Hz
    Speaker {
        motor: motor(350.0, 1.3, 0.45, 0.05, 0.15, 2.5, 30.0),
        gx: 0.9,
        throat: None,
        body: &[hp(230.0, 0.7), bell(2500.0, 1.2, 3.0), lp(6000.0, 0.9)],
        cat: Cat::Other,
    },
    // 8 Gramophone horn + mica soundbox: 250 – 4,000 Hz, soundbox resonance ≈ 2.8 kHz, horn ripples
    Speaker {
        motor: motor(250.0, 1.2, 0.3, 0.0, 0.0, 1.5, 10.0),
        gx: 1.0,
        throat: Some((0.2, 4000.0)),
        body: &[hp(250.0, 0.8), bell(2800.0, 4.0, 9.0), bell(600.0, 5.0, 3.0), bell(1100.0, 5.0, 3.0), bell(1700.0, 5.0, 2.0)],
        cat: Cat::Horn,
    },
    // 9 Laptop micro-speaker: 4th-order high-pass at 700 Hz (motor + body), 700 Hz – 14 kHz, peak ≈ 1 kHz
    Speaker {
        motor: motor(700.0, 1.0, 0.5, 0.05, 0.1, 3.0, 40.0),
        gx: 0.8,
        throat: None,
        body: &[hp(700.0, 0.71), bell(1000.0, 1.5, 4.0), lp(14000.0, 0.7)],
        cat: Cat::Other,
    },
    // 10 the cell phone's receiver: 300 – 3,400 Hz, smooth
    Speaker {
        motor: motor(300.0, 1.2, 0.4, 0.0, 0.0, 1.5, 10.0),
        gx: 1.0,
        throat: None,
        body: &[bell(1500.0, 0.8, 1.0), lp(3400.0, 1.3)],
        cat: Cat::Handset,
    },
];

/// The room (README §2.6). `room` option k (1 – 4) is `Room` k − 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Room {
    None,
    Small,
    Hall,
    Street,
}

impl Room {
    pub fn from_option(i: usize) -> Room {
        [Room::None, Room::None, Room::Small, Room::Hall, Room::Street][i.min(4)]
    }
    /// The room's diffuse level re the direct sound at `distance` 0 (r_near / critical distance) [I].
    pub fn diffuse(self, cat: Cat) -> f64 {
        let rc = match self {
            Room::None => return 0.0,
            Room::Small => 0.6,
            Room::Hall => 3.0,
            Room::Street => 4.0,
        };
        (near(cat) / rc).min(0.7)
    }
}

/// r_near and r_far by speaker category (m).
pub fn near(cat: Cat) -> f64 {
    match cat {
        Cat::Handset => 0.02,
        Cat::Horn => 1.0,
        Cat::Other => 0.3,
    }
}
pub fn far(cat: Cat) -> f64 {
    match cat {
        Cat::Handset => 2.0,
        Cat::Horn => 40.0,
        Cat::Other => 4.0,
    }
}

/// A channel as a model uses it.
#[derive(Clone, Copy, Debug)]
pub struct ChanCfg {
    pub kind: Chan,
    /// the intercom's short line: hum and AGC, no crosstalk, little hiss
    pub short: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Model {
    pub name: &'static str,
    pub mic: Mic,
    pub chan: ChanCfg,
    pub amp: Amp,
    /// speaker voicing index (0 = none)
    pub speaker: usize,
    pub room: Room,
    /// wet polarity (README §2.7: the one whose deepest notch at `mix` 30 – 50 % is shallower)
    pub polarity: f64,
}

const fn amp(kind: AmpKind, rail: f64, cross: f64, bias: f64, hiss_db: f64, hum_db: f64, crackle: f64) -> Amp {
    Amp { kind, rail, cross, bias, hiss_db, hum_db, crackle, enhance: 0.0 }
}
const NONE_DB: f64 = -200.0;
const fn ch(kind: Chan) -> ChanCfg {
    ChanCfg { kind, short: false }
}

pub static MODELS: [Model; 10] = [
    Model { name: "Telephone", mic: Mic::Carbon, chan: ch(Chan::PhoneLine), amp: amp(AmpKind::Line, 2.0, 0.0, 0.0, NONE_DB, NONE_DB, 0.0), speaker: 1, room: Room::Small, polarity: -1.0 },
    Model { name: "Cell Phone", mic: Mic::Clean, chan: ch(Chan::Cell), amp: amp(AmpKind::Smart, 2.0, 0.0, 0.0, NONE_DB, NONE_DB, 0.0), speaker: PHONE_RECEIVER, room: Room::Small, polarity: 1.0 },
    Model { name: "Megaphone", mic: Mic::Dynamic, chan: ch(Chan::None), amp: amp(AmpKind::ClassB, 0.35, 0.01, 0.0, -60.0, NONE_DB, 0.0), speaker: 2, room: Room::Street, polarity: -1.0 },
    Model { name: "Walkie-Talkie", mic: Mic::Dynamic, chan: ch(Chan::Nbfm), amp: amp(AmpKind::ClassAb, 0.5, 0.003, 0.0, NONE_DB, NONE_DB, 0.0), speaker: 3, room: Room::Small, polarity: -1.0 },
    Model { name: "AM Radio", mic: Mic::Clean, chan: ch(Chan::Am), amp: amp(AmpKind::Tube, 0.8, 0.0, 0.15, NONE_DB, NONE_DB, 0.0), speaker: 4, room: Room::Small, polarity: 1.0 },
    Model { name: "PA Horn", mic: Mic::Dynamic, chan: ch(Chan::None), amp: amp(AmpKind::TubePa, 0.7, 0.0, 0.1, -58.0, -55.0, 0.0), speaker: 5, room: Room::Hall, polarity: -1.0 },
    Model { name: "Intercom", mic: Mic::Electret, chan: ChanCfg { kind: Chan::PhoneLine, short: true }, amp: amp(AmpKind::Ic, 0.6, 0.0, 0.0, NONE_DB, NONE_DB, 0.0), speaker: 6, room: Room::None, polarity: -1.0 },
    Model { name: "Pocket Radio", mic: Mic::Clean, chan: ch(Chan::None), amp: amp(AmpKind::ClassB, 0.6, 0.02, 0.0, -65.0, NONE_DB, 0.0), speaker: 7, room: Room::None, polarity: 1.0 },
    Model { name: "Gramophone", mic: Mic::None, chan: ch(Chan::None), amp: amp(AmpKind::Soundbox, 0.5, 0.0, 0.4, -50.0, NONE_DB, 1.5), speaker: 8, room: Room::Small, polarity: -1.0 },
    Model { name: "Laptop", mic: Mic::Clean, chan: ch(Chan::None), amp: Amp { enhance: 0.5, ..amp(AmpKind::Smart, 2.0, 0.0, 0.0, NONE_DB, NONE_DB, 0.0) }, speaker: 9, room: Room::Small, polarity: 1.0 },
];

/// The model's static makeup (dB) at `drive` 0, 25, 50, 75, 100 % (README §2.4, §2.7): measured on the
/// pink reference at −18 dBFS RMS (`noise` 0, `distance` 0, its own chain), K-weighted; a constant table
/// (tests::calibrate prints it).
pub static MAKEUP_DB: [[f64; 5]; 10] = [
    [-5.4, -12.8, -20.8, -28.8, -33.4],
    [3.3, -4.2, -9.4, -10.0, -10.0],
    [0.1, -5.3, -7.1, -7.6, -7.7],
    [2.2, -9.1, -13.5, -14.5, -14.8],
    [-7.4, -14.5, -17.7, -18.6, -18.8],
    [-2.1, -8.8, -14.1, -17.0, -18.1],
    [-5.6, -12.3, -15.1, -15.8, -16.1],
    [2.1, -6.3, -11.3, -12.7, -13.0],
    [-1.4, -8.1, -12.6, -14.7, -15.4],
    [0.1, -7.4, -9.5, -9.5, -9.5],
];
