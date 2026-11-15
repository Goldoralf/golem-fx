// SPDX-License-Identifier: GPL-3.0-or-later
//! The model tables (README §2.1): a model is a parameter set on one chain, not an algorithm.
//! Every value is a voicing starting point [I] unless the README marks it otherwise.

/// What sits between the record and playback EQs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Jiles-Atherton hysteresis.
    Tape,
    /// RIAA + tracing distortion + cutter limit.
    Vinyl,
    /// The early 78 curve (≈ 500 Hz turnover, no treble pre-emphasis), heavier tracing.
    Shellac,
}

/// A rotating part of the transport: centre frequency (Hz at the model's own speed), the range its
/// frequency glides over (reels), and its share of the band's periodic amplitude.
#[derive(Clone, Copy, Debug)]
pub struct Comp {
    pub f: f64,
    pub lo: f64,
    pub hi: f64,
    pub amp: f64,
}

const fn c(f: f64, amp: f64) -> Comp {
    Comp { f, lo: f, hi: f, amp }
}

const fn glide(lo: f64, hi: f64, amp: f64) -> Comp {
    Comp { f: 0.5 * (lo + hi), lo, hi, amp }
}

/// Mechanical noise: a motor's harmonic series plus a whir band, or a disc's rumble.
#[derive(Clone, Copy, Debug)]
pub struct Mech {
    /// motor fundamental (Hz), 0 = none
    pub motor_hz: f64,
    /// share of the motor series in the power (the rest is the band)
    pub motor_share: f64,
    pub band: (f64, f64),
}

#[derive(Clone, Copy, Debug)]
pub struct Model {
    pub name: &'static str,
    pub kind: Kind,
    /// Native speed in ips (tape); for discs the outer-groove speed, used by the spacing filter.
    pub ips: f64,
    /// Rated −3 dB points at the native speed (the high one moves with the speed on tape).
    pub lf_edge: Option<f64>,
    pub hf_edge: f64,
    /// Effective head length for the bump, mm (None: no bump modelled).
    pub bump_mm: Option<f64>,
    /// Peak field of the reference sine with ferric tape, dB re a (sets MOL: 3 % at +3.5 dB re a).
    pub h_ref_db: f64,
    /// Default formula High Output (the studio's "hot" tape).
    pub hot: bool,
    /// IEC (Studio) or NAB / cassette replay time constants for the hiss shape.
    pub iec: bool,
    /// Track width and track-centre spacing (mm), for azimuth.
    pub track_w_mm: f64,
    pub track_s_mm: f64,
    /// Hiss S/N, mechanical noise and modulation noise (dB re the reference level).
    pub hiss_db: f64,
    pub mech_db: f64,
    pub mod_db: f64,
    pub mech: Mech,
    /// Owns an ALC (`alc` Model).
    pub alc: bool,
    /// Transport: periodic components of the wow and flutter bands, random shares.
    pub wow: &'static [Comp],
    pub flutter: &'static [Comp],
    pub rho_wow: f64,
    pub rho_flutter: f64,
    /// Disc rotation (rpm), for scratches; 0 on tape.
    pub rpm: f64,
}

pub const STUDIO: usize = 0;
pub const HOME: usize = 1;
pub const CASSETTE: usize = 2;
pub const PORTABLE: usize = 3;
pub const MICRO: usize = 4;
pub const VHS: usize = 5;
pub const VINYL: usize = 6;
pub const SHELLAC: usize = 7;

/// Option labels (enum order, final: append only). AM Radio lives in gfx-radio and is never appended.
pub const MODEL_NAMES: [&str; 8] = ["Studio Reel", "Home Reel", "Cassette", "Portable Cassette", "Microcassette", "VHS", "Vinyl", "Shellac 78"];
pub const SPEED_NAMES: [&str; 6] = ["Model", "1⅞ ips", "3¾ ips", "7½ ips", "15 ips", "30 ips"];
pub const SPEEDS_IPS: [f64; 6] = [0.0, 1.875, 3.75, 7.5, 15.0, 30.0];
pub const FORMULA_NAMES: [&str; 4] = ["Model", "Ferric", "High Output", "Chrome"];
pub const ALC_NAMES: [&str; 3] = ["Model", "Off", "On"];
/// `Sliding Band` is appended when coded.
pub const NR_NAMES: [&str; 2] = ["Off", "Compander"];
pub const NOISE_MODE_NAMES: [&str; 3] = ["Follow", "Constant", "Duck"];

const H2: f64 = 0.316;
/// Second harmonic of every rotating part (eccentricity is not a sine: −10 dB).
pub const WOBBLE_H2: f64 = H2;

const MOTOR: Mech = Mech { motor_hz: 50.0, motor_share: 0.5, band: (300.0, 3000.0) };

pub const MODELS: [Model; 8] = [
    Model {
        name: "Studio Reel",
        kind: Kind::Tape,
        ips: 15.0,
        lf_edge: None,
        hf_edge: 22_000.0,
        bump_mm: Some(10.0),
        h_ref_db: -7.5,
        hot: true,
        iec: true,
        track_w_mm: 2.0,
        track_s_mm: 3.4,
        hiss_db: -68.0,
        mech_db: -80.0,
        mod_db: -55.0,
        mech: MOTOR,
        alc: false,
        wow: &[c(3.2, 0.6), glide(0.47, 1.2, 1.0)],
        flutter: &[c(4.8, 1.0), glide(15.0, 30.0, 0.5)],
        rho_wow: 0.3,
        rho_flutter: 0.3,
        rpm: 0.0,
    },
    Model {
        name: "Home Reel",
        kind: Kind::Tape,
        ips: 7.5,
        lf_edge: None,
        hf_edge: 16_000.0,
        bump_mm: Some(8.0),
        h_ref_db: -7.5,
        hot: false,
        iec: false,
        track_w_mm: 1.0,
        track_s_mm: 1.4,
        hiss_db: -58.0,
        mech_db: -70.0,
        mod_db: -50.0,
        mech: MOTOR,
        alc: false,
        wow: &[glide(1.0, 2.0, 0.8), glide(0.3, 0.8, 1.0)],
        flutter: &[c(10.0, 1.0)],
        rho_wow: 0.5,
        rho_flutter: 0.3,
        rpm: 0.0,
    },
    Model {
        name: "Cassette",
        kind: Kind::Tape,
        ips: 1.875,
        lf_edge: Some(40.0),
        hf_edge: 14_000.0,
        bump_mm: Some(4.0),
        h_ref_db: -3.5,
        hot: false,
        iec: false,
        track_w_mm: 0.6,
        track_s_mm: 1.0,
        hiss_db: -50.0,
        mech_db: -66.0,
        mod_db: -45.0,
        mech: MOTOR,
        alc: false,
        wow: &[c(1.5, 0.8), glide(0.28, 0.69, 1.0)],
        flutter: &[c(6.3, 1.0)],
        rho_wow: 0.4,
        rho_flutter: 0.3,
        rpm: 0.0,
    },
    Model {
        name: "Portable Cassette",
        kind: Kind::Tape,
        ips: 1.875,
        lf_edge: Some(80.0),
        hf_edge: 10_000.0,
        bump_mm: None,
        h_ref_db: -3.5,
        hot: false,
        iec: false,
        track_w_mm: 0.6,
        track_s_mm: 1.0,
        hiss_db: -46.0,
        mech_db: -56.0,
        mod_db: -42.0,
        mech: Mech { motor_hz: 30.0, motor_share: 0.6, band: (400.0, 4000.0) },
        alc: true,
        wow: &[c(1.5, 0.8), glide(0.28, 0.69, 1.0), glide(0.1, 0.3, 1.0)],
        flutter: &[c(6.3, 0.7), glide(20.0, 40.0, 0.7)],
        rho_wow: 0.7,
        rho_flutter: 0.5,
        rpm: 0.0,
    },
    Model {
        name: "Microcassette",
        kind: Kind::Tape,
        ips: 0.945,
        lf_edge: Some(300.0),
        hf_edge: 5_000.0,
        bump_mm: None,
        h_ref_db: -2.5,
        hot: false,
        iec: false,
        track_w_mm: 0.5,
        track_s_mm: 0.8,
        hiss_db: -40.0,
        mech_db: -46.0,
        mod_db: -35.0,
        mech: Mech { motor_hz: 35.0, motor_share: 0.7, band: (500.0, 5000.0) },
        alc: true,
        wow: &[glide(0.3, 0.8, 1.0)],
        flutter: &[c(10.0, 1.0), c(35.0, 0.5)],
        rho_wow: 0.6,
        rho_flutter: 0.4,
        rpm: 0.0,
    },
    Model {
        name: "VHS",
        kind: Kind::Tape,
        ips: 1.313,
        lf_edge: Some(100.0),
        hf_edge: 10_000.0,
        bump_mm: None,
        h_ref_db: -4.5,
        hot: false,
        iec: false,
        track_w_mm: 0.35,
        track_s_mm: 0.7,
        hiss_db: -42.0,
        mech_db: -60.0,
        mod_db: -42.0,
        mech: Mech { motor_hz: 30.0, motor_share: 0.4, band: (200.0, 2000.0) },
        alc: false,
        wow: &[c(2.0, 0.8), glide(0.5, 1.0, 1.0)],
        flutter: &[c(8.0, 0.5), c(25.0, 0.5)],
        rho_wow: 0.5,
        rho_flutter: 0.3,
        rpm: 0.0,
    },
    Model {
        name: "Vinyl",
        kind: Kind::Vinyl,
        // outer groove at 33⅓: 2π · 146 mm · 33⅓ / 60 = 0.51 m/s
        ips: 20.06,
        lf_edge: Some(20.0),
        hf_edge: 16_000.0,
        bump_mm: None,
        h_ref_db: 0.0,
        hot: false,
        iec: false,
        track_w_mm: 0.0,
        track_s_mm: 0.0,
        hiss_db: -60.0,
        mech_db: -50.0,
        mod_db: -60.0,
        mech: Mech { motor_hz: 0.0, motor_share: 0.0, band: (10.0, 40.0) },
        alc: false,
        // eccentricity and warp at the rotation (0.556 Hz) and its second harmonic
        wow: &[c(0.556, 1.0), c(1.111, 0.5)],
        flutter: &[c(5.0, 1.0)],
        rho_wow: 0.15,
        rho_flutter: 0.3,
        rpm: 33.333,
    },
    Model {
        name: "Shellac 78",
        kind: Kind::Shellac,
        // 2π · 140 mm · 78 / 60 = 1.14 m/s
        ips: 45.0,
        lf_edge: Some(100.0),
        hf_edge: 5_000.0,
        bump_mm: None,
        h_ref_db: 0.0,
        hot: false,
        iec: false,
        track_w_mm: 0.0,
        track_s_mm: 0.0,
        hiss_db: -35.0,
        mech_db: -40.0,
        mod_db: -45.0,
        mech: Mech { motor_hz: 0.0, motor_share: 0.0, band: (10.0, 40.0) },
        alc: false,
        wow: &[c(1.3, 1.0)],
        flutter: &[c(10.0, 1.0)],
        rho_wow: 0.3,
        rho_flutter: 0.3,
        rpm: 78.0,
    },
];

/// The record amplifier's drive boost at 10 kHz (README §2.2 table), interpolated in log2 speed,
/// +3 dB / octave below 1⅞ ips, −2 dB / octave above 30. Chrome −2 dB.
pub fn drive_boost_db(ips: f64, chrome: bool) -> f64 {
    const T: [(f64, f64); 5] = [(1.875, 12.0), (3.75, 9.0), (7.5, 6.0), (15.0, 3.5), (30.0, 1.5)];
    let x = ips.log2();
    let b = if ips <= T[0].0 {
        T[0].1 + 3.0 * (T[0].0.log2() - x)
    } else if ips >= T[4].0 {
        T[4].1 - 2.0 * (x - T[4].0.log2())
    } else {
        let mut v = T[4].1;
        for w in T.windows(2) {
            if ips <= w[1].0 {
                let t = (x - w[0].0.log2()) / (w[1].0.log2() - w[0].0.log2());
                v = w[0].1 + (w[1].1 - w[0].1) * t;
                break;
            }
        }
        v
    };
    (b - if chrome { 2.0 } else { 0.0 }).max(0.0)
}

/// The replay time constant (s) of the model's standard at `ips` (shapes the hiss only, README §1).
pub fn replay_tau(m: &Model, ips: f64, chrome: bool) -> f64 {
    let us = if ips <= 2.5 {
        if chrome {
            70.0
        } else {
            120.0
        }
    } else if ips <= 5.0 {
        90.0
    } else if m.iec {
        if ips <= 10.0 {
            70.0
        } else if ips <= 20.0 {
            35.0
        } else {
            17.5
        }
    } else if ips <= 20.0 {
        50.0
    } else {
        17.5
    };
    us * 1e-6
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boost_table() {
        assert!((drive_boost_db(1.875, false) - 12.0).abs() < 1e-9);
        assert!((drive_boost_db(1.875, true) - 10.0).abs() < 1e-9);
        assert!((drive_boost_db(15.0, false) - 3.5).abs() < 1e-9);
        assert!((drive_boost_db(0.945, false) - 14.97).abs() < 0.05);
    }

    #[test]
    fn head_bump_frequencies() {
        // README §2.1: 38 Hz at 15 ips (Studio), 24 Hz at 7½ (Home), 12 Hz at 1⅞ (Cassette)
        let fb = |m: usize| MODELS[m].ips * 0.0254 / (MODELS[m].bump_mm.unwrap() * 1e-3);
        assert!((fb(STUDIO) - 38.1).abs() < 0.1 && (fb(HOME) - 23.8).abs() < 0.1 && (fb(CASSETTE) - 11.9).abs() < 0.1);
    }
}
