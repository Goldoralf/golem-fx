// SPDX-License-Identifier: GPL-3.0-or-later
//! Profile and mode anchors (VOICING.md §2 – §4) resolved against the expert groups.
//!
//! The mode sets the shape of the modulation and what the reference unit does by itself. The profile
//! sets everything else. A group whose Custom switch is on uses its own controls instead. Warmth then
//! moves the colour around whichever anchor applies.

use crate::p;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    Clean,
    Bbd,
    Tape,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    GuitarClean,
    GuitarDrive,
    Bass,
    Keys,
    Voice,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Classic,
    DualI,
    DualII,
    DualI2,
    Dim1,
    Dim2,
    Dim3,
    Ensemble,
    Doubler,
}

impl Profile {
    pub const ALL: [Profile; 5] = [Profile::GuitarClean, Profile::GuitarDrive, Profile::Bass, Profile::Keys, Profile::Voice];
}

impl Mode {
    pub const ALL: [Mode; 9] = [Mode::Classic, Mode::DualI, Mode::DualII, Mode::DualI2, Mode::Dim1, Mode::Dim2, Mode::Dim3, Mode::Ensemble, Mode::Doubler];
}

/// LFO shapes, in the `lfo_shape` enum's order.
pub const SINE: u32 = 0;
pub const TRIANGLE: u32 = 1;
pub const ROUNDED: u32 = 2;
pub const RANDOM: u32 = 3;
pub const LOG: u32 = 4;

/// Chip stage counts, in the `bbd_chip` enum's order.
pub const CHIPS: [usize; 5] = [256, 512, 1024, 2048, 4096];

/// What changes the structure (crossfaded through a dip of the wet).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Structure {
    pub engine: Engine,
    pub interp: u32,
    pub chip: usize,
    pub shape: u32,
    pub voices: usize,
    /// voices share one modulation (Dual I+II): one voice to both sides
    pub mono: bool,
    pub fast_lfo: bool,
    pub xover_on: bool,
    pub xover_24: bool,
    pub mono_safe: bool,
}

/// Continuous targets, smoothed at control rate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colour {
    pub xover_freq: f32,
    pub drive: f32,
    pub bbd_bw: f32,
    pub emphasis: f32,
    pub compander: f32,
    pub noise: f32,
    pub wow: f32,
    pub flutter: f32,
    pub centre_ms: f32,
    pub spread_deg: f32,
    pub imperfection: f32,
    pub drift: f32,
    pub crossmix: f32,
    pub env_depth: f32,
    pub env_mix: f32,
    pub width: f32,
    pub hpf: f32,
    pub lpf: f32,
}

pub const N_COLOUR: usize = 18;

impl Colour {
    pub fn to_array(self) -> [f32; N_COLOUR] {
        [
            self.xover_freq,
            self.drive,
            self.bbd_bw,
            self.emphasis,
            self.compander,
            self.noise,
            self.wow,
            self.flutter,
            self.centre_ms,
            self.spread_deg,
            self.imperfection,
            self.drift,
            self.crossmix,
            self.env_depth,
            self.env_mix,
            self.width,
            self.hpf,
            self.lpf,
        ]
    }
}

/// Indices into [`Colour::to_array`].
pub mod c {
    pub const XOVER: usize = 0;
    pub const DRIVE: usize = 1;
    pub const BW: usize = 2;
    pub const EMPH: usize = 3;
    pub const COMP: usize = 4;
    pub const NOISE: usize = 5;
    pub const WOW: usize = 6;
    pub const FLUTTER: usize = 7;
    pub const CENTRE: usize = 8;
    pub const SPREAD: usize = 9;
    pub const IMPERF: usize = 10;
    pub const DRIFT: usize = 11;
    pub const CROSSMIX: usize = 12;
    pub const ENV_DEPTH: usize = 13;
    pub const ENV_MIX: usize = 14;
    pub const WIDTH: usize = 15;
    pub const HPF: usize = 16;
    pub const LPF: usize = 17;
}

struct ModeAnchors {
    centre: f32,
    shape: u32,
    voices: usize,
    spread: f32,
    chip: usize,
    bw: f32,
    compander: f32,
    crossmix: f32,
    imperfection: Option<f32>,
    mono: bool,
    fast: bool,
    /// the spread is the profile's stereo phase (Classic)
    spread_is_stereo: bool,
}

fn mode_anchors(m: Mode) -> ModeAnchors {
    let base = ModeAnchors {
        centre: 8.0,
        shape: TRIANGLE,
        voices: 2,
        spread: 180.0,
        chip: 1024,
        bw: 6600.0,
        compander: 0.0,
        crossmix: 0.0,
        imperfection: None,
        mono: false,
        fast: false,
        spread_is_stereo: false,
    };
    match m {
        Mode::Classic => ModeAnchors { spread_is_stereo: true, ..base },
        Mode::DualI | Mode::DualII => ModeAnchors { centre: 3.5, chip: 256, bw: 9000.0, ..base },
        Mode::DualI2 => ModeAnchors { centre: 3.5, chip: 256, bw: 9000.0, voices: 1, mono: true, ..base },
        Mode::Dim1 | Mode::Dim2 | Mode::Dim3 => ModeAnchors {
            centre: match m {
                Mode::Dim1 => 6.0,
                Mode::Dim2 => 3.0,
                _ => 4.5,
            },
            bw: 10000.0,
            compander: 1.0,
            crossmix: 1.0,
            ..base
        },
        Mode::Ensemble => ModeAnchors { centre: 5.0, shape: SINE, voices: 3, spread: 120.0, chip: 512, bw: 8000.0, imperfection: Some(0.3), fast: true, ..base },
        Mode::Doubler => ModeAnchors { centre: 18.0, shape: RANDOM, voices: 2, spread: 0.0, chip: 4096, bw: 12000.0, ..base },
    }
}

struct ProfileAnchors {
    drift: f32,
    imperfection: f32,
    xover_on: bool,
    xover_freq: f32,
    engine: Engine,
    drive: f32,
    emphasis: f32,
    noise: f32,
    env_mix: f32,
    hpf: f32,
    lpf: f32,
    stereo_phase: f32,
    width: f32,
    mono_safe: bool,
    /// follower attack / release (ms)
    pub att: f32,
    pub rel: f32,
    /// warmth: wet low-pass octaves up at 0 %, down at 100 %; BBD band ± octaves; noise multiplier at 100 %
    lpf_up: f32,
    lpf_down: f32,
    bw_oct: f32,
    noise_top: f32,
    warm_drive: bool,
}

fn profile_anchors(p: Profile) -> ProfileAnchors {
    match p {
        Profile::GuitarClean => ProfileAnchors { drift: 0.10, imperfection: 0.10, xover_on: false, xover_freq: 120.0, engine: Engine::Bbd, drive: 0.25, emphasis: 1.0, noise: 0.15, env_mix: 0.0, hpf: 80.0, lpf: 8000.0, stereo_phase: 90.0, width: 1.0, mono_safe: false, att: 5.0, rel: 150.0, lpf_up: 1.0, lpf_down: 1.0, bw_oct: 0.5, noise_top: 1.5, warm_drive: true },
        Profile::GuitarDrive => ProfileAnchors { drift: 0.15, imperfection: 0.10, xover_on: true, xover_freq: 150.0, engine: Engine::Clean, drive: 0.05, emphasis: 1.0, noise: 0.0, env_mix: -0.3, hpf: 250.0, lpf: 5000.0, stereo_phase: 180.0, width: 1.0, mono_safe: true, att: 5.0, rel: 150.0, lpf_up: 0.5, lpf_down: 1.0, bw_oct: 0.0, noise_top: 1.0, warm_drive: true },
        Profile::Bass => ProfileAnchors { drift: 0.05, imperfection: 0.05, xover_on: true, xover_freq: 200.0, engine: Engine::Bbd, drive: 0.30, emphasis: 1.0, noise: 0.10, env_mix: 0.0, hpf: 40.0, lpf: 4000.0, stereo_phase: 90.0, width: 0.6, mono_safe: true, att: 10.0, rel: 200.0, lpf_up: 0.5, lpf_down: 1.0, bw_oct: 0.5, noise_top: 1.5, warm_drive: true },
        Profile::Keys => ProfileAnchors { drift: 0.05, imperfection: 0.15, xover_on: true, xover_freq: 100.0, engine: Engine::Bbd, drive: 0.20, emphasis: 0.0, noise: 0.40, env_mix: 0.0, hpf: 60.0, lpf: 10000.0, stereo_phase: 180.0, width: 1.0, mono_safe: false, att: 20.0, rel: 300.0, lpf_up: 1.0, lpf_down: 1.0, bw_oct: 0.5, noise_top: 2.0, warm_drive: true },
        Profile::Voice => ProfileAnchors { drift: 0.60, imperfection: 0.0, xover_on: false, xover_freq: 120.0, engine: Engine::Clean, drive: 0.0, emphasis: 0.0, noise: 0.0, env_mix: -0.2, hpf: 150.0, lpf: 12000.0, stereo_phase: 90.0, width: 0.7, mono_safe: true, att: 15.0, rel: 250.0, lpf_up: 0.5, lpf_down: 0.5, bw_oct: 0.0, noise_top: 1.0, warm_drive: false },
    }
}

/// The follower times of a profile (ms).
pub fn follower_times(profile: Profile) -> (f32, f32) {
    let a = profile_anchors(profile);
    (a.att, a.rel)
}

/// Resolves the structure from the settings.
pub fn structure(v: &[f32; p::N]) -> Structure {
    let profile = Profile::ALL[idx(v[p::PROFILE], 5)];
    let mode = Mode::ALL[idx(v[p::MODE], 9)];
    let (pa, ma) = (profile_anchors(profile), mode_anchors(mode));
    let color = v[p::COLOR_CUSTOM] >= 0.5;
    let modc = v[p::MOD_CUSTOM] >= 0.5;
    let xo = v[p::XOVER_CUSTOM] >= 0.5;
    let st = v[p::STEREO_CUSTOM] >= 0.5;
    let engine = if color { [Engine::Clean, Engine::Bbd, Engine::Tape][idx(v[p::ENGINE], 3)] } else { pa.engine };
    let voices = match mode {
        Mode::Ensemble | Mode::Doubler if modc => idx(v[p::VOICES], 6) + 1,
        _ => ma.voices,
    };
    Structure {
        engine,
        interp: idx(v[p::INTERP], 3) as u32,
        chip: if color { CHIPS[idx(v[p::BBD_CHIP], 5)] } else { ma.chip },
        shape: if modc { idx(v[p::LFO_SHAPE], 5) as u32 } else { ma.shape },
        voices,
        mono: ma.mono && !(modc && voices > 1),
        fast_lfo: ma.fast,
        xover_on: if xo { v[p::XOVER_ON] >= 0.5 } else { pa.xover_on },
        xover_24: if xo { v[p::XOVER_SLOPE] >= 0.5 } else { true },
        mono_safe: if st { v[p::MONO_SAFE] >= 0.5 } else { pa.mono_safe },
    }
}

/// Resolves the continuous colour from the settings at warmth `w` (0 … 1).
pub fn colour(v: &[f32; p::N], w: f32) -> Colour {
    let profile = Profile::ALL[idx(v[p::PROFILE], 5)];
    let mode = Mode::ALL[idx(v[p::MODE], 9)];
    let (pa, ma) = (profile_anchors(profile), mode_anchors(mode));
    let color = v[p::COLOR_CUSTOM] >= 0.5;
    let modc = v[p::MOD_CUSTOM] >= 0.5;
    let xo = v[p::XOVER_CUSTOM] >= 0.5;
    let st = v[p::STEREO_CUSTOM] >= 0.5;
    let fc = v[p::FILTER_CUSTOM] >= 0.5;
    let pct = |i: usize| v[i] * 0.01;
    // warmth: 0.5 = the anchors
    let t = w.clamp(0.0, 1.0);
    let oct = |up: f32, down: f32| if t < 0.5 { up * (0.5 - t) * 2.0 } else { -down * (t - 0.5) * 2.0 };
    let noise_mul = if t < 0.5 { 0.5 + t } else { 1.0 + (pa.noise_top - 1.0) * (t - 0.5) * 2.0 };
    let drive_mul = if pa.warm_drive { 2.0 * t } else { 1.0 };
    let lpf0 = if fc { v[p::WET_LPF] } else { pa.lpf };
    let bw0 = if color { v[p::BBD_BW] } else { ma.bw };
    let stereo_phase = if st { v[p::STEREO_PHASE] } else { pa.stereo_phase };
    Colour {
        xover_freq: if xo { v[p::XOVER_FREQ] } else { pa.xover_freq },
        drive: ((if color { pct(p::DRIVE) } else { pa.drive }) * drive_mul).clamp(0.0, 1.0),
        bbd_bw: (bw0 * (2f32).powf(oct(pa.bw_oct, pa.bw_oct))).clamp(2000.0, 15000.0),
        emphasis: if color { pct(p::EMPHASIS) } else { pa.emphasis },
        compander: if color { pct(p::COMPANDER) } else { ma.compander },
        noise: ((if color { pct(p::NOISE) } else { pa.noise }) * noise_mul).clamp(0.0, 1.0),
        wow: if color { pct(p::WOW) } else { 0.0 },
        flutter: if color { pct(p::FLUTTER) } else { 0.0 },
        centre_ms: if modc { v[p::DELAY_BASE] } else { ma.centre },
        spread_deg: if modc {
            v[p::VOICE_SPREAD]
        } else if ma.spread_is_stereo {
            stereo_phase
        } else {
            ma.spread
        },
        imperfection: if modc { pct(p::IMPERFECTION) } else { ma.imperfection.unwrap_or(pa.imperfection) },
        drift: if modc { pct(p::DRIFT) } else { pa.drift },
        crossmix: if modc { pct(p::CROSSMIX) } else { ma.crossmix },
        env_depth: if modc { pct(p::ENV_DEPTH) } else { 0.0 },
        env_mix: if modc { pct(p::ENV_MIX) } else { pa.env_mix },
        width: if st { pct(p::WIDTH) } else { pa.width },
        hpf: if fc { v[p::WET_HPF] } else { pa.hpf },
        lpf: (lpf0 * (2f32).powf(oct(pa.lpf_up, pa.lpf_down))).clamp(1000.0, 20000.0),
    }
}

#[inline]
pub fn idx(v: f32, n: usize) -> usize {
    (v.round().max(0.0) as usize).min(n - 1)
}
