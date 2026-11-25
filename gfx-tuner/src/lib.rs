// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-tuner — golem's tuner, measured inside the audio engine (design: README.md).
//!
//! Stands for golem's `tuner` block (id `tuning` kept). **A coarse detector names the note; a strobe
//! measures it**:
//! - the input's (L + R) / 2 is high-passed at 18 Hz, low-passed and decimated to fd ≈ 12 kHz (shared);
//!   a second ÷4 stage (600 Hz, 4th order) feeds the long detector;
//! - coarse: two YIN windows (gfx-dsp `detect`): short at fd (60 Hz – 1.5 kHz, 16.7 ms), long at fd / 4
//!   (25 – 150 Hz, 40 ms). Their two candidates go to the attribution, which uses the tuning as a prior
//!   (octave errors, the 12th-fret harmonic, ties: the fundamental wins);
//! - fine: a gfx-dsp `zoom` track demodulated at the coarse reading: nine Hann frames and a weighted
//!   phase line fit over a finite window growing from 8 periods to 24 (Fast) or 48 (Strobe); an error
//!   bar; "settled" when the readings of the last max(150 ms, T / 2) span ≤ 0.2 ¢ (Fast) or of the last
//!   max(300 ms, T / 2) ≤ 0.07 ¢ (Strobe); a ±35 ¢ capture guard;
//! - poly: one zoom track per open string at its target, a Blackman-Harris grid search and a two-frame
//!   phase refinement every 50 ms, windows from the clean / shared collision table (README §2.5);
//! - outputs: 23 read-only meters, published once per 10 ms hop (`seq` + 1 each hop while analysing).
//!
//! The audio path is a copy (bit-exact, zero latency) or a gain: `mute` while engaged, by a 10 ms
//! raised cosine. Everything runs per sample on clocks counted from reset, so the output and the meters
//! do not depend on the host's block size. Nothing allocates in `process`.

#![allow(clippy::needless_range_loop)]

use gfx_dsp::detect::PitchDetector;
use gfx_dsp::envelope::PeakFollower;
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::zoom::{cents, Zoom};
use gfx_dsp::{Effect, ParamInfo, ParamKind};

#[cfg(feature = "plugin")]
pub mod plugin;
#[cfg(test)]
mod tests;

pub const NP: usize = 25;
const fn int(id: &'static str, name: &'static str, min: f32, max: f32, default: f32) -> ParamInfo {
    ParamInfo { id, name, min, max, default, kind: ParamKind::Int, exp: false }
}

/// Front panel, then expert (README §3.3 – 3.4). `tuning`: rank = value, append-only (§3.2).
pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::enumeration("tuning", "Tuning", 22, 0),
    ParamInfo::enumeration("mode", "Mode", 3, 0),
    ParamInfo::enumeration("precision", "Precision", 2, 0),
    ParamInfo::float("reference", "Reference A", 410.0, 470.0, 440.0),
    ParamInfo::boolean("mute", "Mute", false),
    ParamInfo::boolean("bypass", "Bypass", false),
    int("transpose", "Transpose", -12.0, 12.0, 0.0),
    ParamInfo::float("offset_1", "Offset 1", -50.0, 50.0, 0.0),
    ParamInfo::float("offset_2", "Offset 2", -50.0, 50.0, 0.0),
    ParamInfo::float("offset_3", "Offset 3", -50.0, 50.0, 0.0),
    ParamInfo::float("offset_4", "Offset 4", -50.0, 50.0, 0.0),
    ParamInfo::float("offset_5", "Offset 5", -50.0, 50.0, 0.0),
    ParamInfo::float("offset_6", "Offset 6", -50.0, 50.0, 0.0),
    ParamInfo::float("offset_7", "Offset 7", -50.0, 50.0, 0.0),
    ParamInfo::float("offset_8", "Offset 8", -50.0, 50.0, 0.0),
    int("string_1", "String 1", 0.0, 96.0, 40.0),
    int("string_2", "String 2", 0.0, 96.0, 45.0),
    int("string_3", "String 3", 0.0, 96.0, 50.0),
    int("string_4", "String 4", 0.0, 96.0, 55.0),
    int("string_5", "String 5", 0.0, 96.0, 59.0),
    int("string_6", "String 6", 0.0, 96.0, 64.0),
    int("string_7", "String 7", 0.0, 96.0, 0.0),
    int("string_8", "String 8", 0.0, 96.0, 0.0),
    ParamInfo::float("threshold", "Threshold", -90.0, -30.0, -65.0),
    ParamInfo::boolean("always_on", "Always On", false),
];

/// `tuning` option names, rank = value (README §3.2): Chromatic, Custom, then golem's ids.
pub static TUNING_NAMES: [&str; 22] = [
    "Chromatic", "Custom", "6-std", "eb-std", "d-std", "drop-d", "dadgad", "drop-c", "7-std", "7-drop-a", "7-gdgcfad",
    "8-std", "8-e", "8-drop-e", "8-dbdadgbe", "bass-4-std", "bass-4-eb", "bass-4-d-std", "bass-4-drop-d", "bass-5-std",
    "bass-5-high-c", "bass-5-drop-a",
];

/// The open strings of ranks 2 – 21, MIDI from the lowest string, exactly as golem's
/// `library/catalog/tunings.json` on 2026-11-14 (a test compares them with the file when present).
pub static TUNINGS: [&[u8]; 20] = [
    &[40, 45, 50, 55, 59, 64],
    &[39, 44, 49, 54, 58, 63],
    &[38, 43, 48, 53, 57, 62],
    &[38, 45, 50, 55, 59, 64],
    &[38, 45, 50, 55, 57, 62],
    &[36, 43, 48, 53, 57, 62],
    &[35, 40, 45, 50, 55, 59, 64],
    &[33, 40, 45, 50, 55, 59, 64],
    &[31, 38, 43, 48, 53, 57, 62],
    &[30, 35, 40, 45, 50, 55, 59, 64],
    &[28, 35, 40, 45, 50, 55, 59, 64],
    &[28, 33, 40, 45, 50, 55, 59, 64],
    &[26, 35, 38, 45, 50, 55, 59, 64],
    &[28, 33, 38, 43],
    &[27, 32, 37, 42],
    &[26, 31, 36, 41],
    &[26, 33, 38, 43],
    &[23, 28, 33, 38, 43],
    &[28, 33, 38, 43, 48],
    &[21, 28, 33, 38, 43],
];

/// The read-only meters (README §3.5), in this order.
pub static METERS: [&str; 23] = [
    "freq", "note", "cents", "confidence", "string", "state", "seq", "s1_cents", "s2_cents", "s3_cents", "s4_cents",
    "s5_cents", "s6_cents", "s7_cents", "s8_cents", "s1_conf", "s2_conf", "s3_conf", "s4_conf", "s5_conf", "s6_conf",
    "s7_conf", "s8_conf",
];
const M_FREQ: usize = 0;
const M_NOTE: usize = 1;
const M_CENTS: usize = 2;
const M_CONF: usize = 3;
const M_STRING: usize = 4;
const M_STATE: usize = 5;
const M_SEQ: usize = 6;
const M_SCENTS: usize = 7;
const M_SCONF: usize = 15;

/// `state` values.
pub const STATE_SILENCE: f32 = 0.0;
pub const STATE_SETTLING: f32 = 1.0;
pub const STATE_SETTLED: f32 = 2.0;
pub const STATE_STRUM: f32 = 3.0;

/// The decimated rate's target (Hz), as gfx-dsp `detect`.
const FD_TARGET: f32 = 12000.0;
/// Publishing hop (s).
const HOP_S: f64 = 0.01;
/// History of the decimated input, replayed into a track started late (samples at fd).
const HIST: usize = 4096;
/// The settled rule's memory (hops).
const READINGS: usize = 128;
/// Mute ramp (s).
const RAMP_S: f32 = 0.01;

/// One `precision`'s constants (README §2.4).
#[derive(Clone, Copy)]
struct Precision {
    /// window grows from `p_min` periods to `p_max`, clamped to [t_lo, t_hi] s
    p_min: f64,
    p_max: f64,
    t_lo: f64,
    t_hi: f64,
    /// settled: the readings of the last max(span, T / 2) span ≤ tol ¢
    span: f64,
    tol: f64,
}

const FAST: Precision = Precision { p_min: 8.0, p_max: 24.0, t_lo: 0.08, t_hi: 0.6, span: 0.15, tol: 0.2 };
const STROBE: Precision = Precision { p_min: 8.0, p_max: 48.0, t_lo: 0.3, t_hi: 1.2, span: 0.3, tol: 0.07 };

impl Precision {
    fn t_max(&self, f: f64) -> f64 {
        (self.p_max / f).clamp(self.t_lo, self.t_hi)
    }
    fn t_min(&self, f: f64) -> f64 {
        (self.p_min / f).min(self.t_max(f))
    }
}

/// A reading's target (README §2.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    /// 1 = the lowest string; 0 = chromatic
    pub string: u8,
    /// the string's 2nd partial (12th-fret harmonic)
    pub harmonic: bool,
    /// MIDI note (concert pitch): the string's note, + 12 for its harmonic
    pub note: i32,
    /// the frequency the cents are measured against (Hz), offset included
    pub hz: f64,
}

impl Target {
    fn key(&self) -> (u8, bool, i32) {
        (self.string, self.harmonic, self.note)
    }
}

/// The parameters as plain values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub tuning: usize,
    pub mode: usize,
    pub precision: usize,
    pub reference: f32,
    pub mute: bool,
    pub bypass: bool,
    pub transpose: i32,
    pub offsets: [f32; 8],
    pub strings: [u8; 8],
    pub threshold: f32,
    pub always_on: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            tuning: 0,
            mode: 0,
            precision: 0,
            reference: 440.0,
            mute: false,
            bypass: false,
            transpose: 0,
            offsets: [0.0; 8],
            strings: [40, 45, 50, 55, 59, 64, 0, 0],
            threshold: -65.0,
            always_on: false,
        }
    }
}

impl Settings {
    /// The open strings, MIDI from the lowest (empty = chromatic). A named option and Custom with the
    /// same notes give the same list (README §3.1).
    pub fn open_strings(&self) -> ([u8; 8], usize) {
        let mut out = [0u8; 8];
        let src: &[u8] = match self.tuning {
            0 => &[],
            1 => {
                let n = self.strings.iter().position(|&m| m == 0).unwrap_or(8);
                &self.strings[..n]
            }
            t => TUNINGS[(t - 2).min(TUNINGS.len() - 1)],
        };
        out[..src.len()].copy_from_slice(src);
        (out, src.len())
    }
}

/// What the analysis did (tests and the bench): onsets, track starts and why.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Counters {
    pub onsets: u32,
    pub starts: u32,
    /// restarts because the coarse reading left the wide filter (170 ¢)
    pub far: u32,
    /// hops whose estimate the capture guard held (35 ¢)
    pub guard: u32,
    /// open strings found under a harmonic attribution (the strobe moved down)
    pub octave_fixes: u32,
    pub grids: u32,
}

/// One poly string's reading.
#[derive(Clone, Copy, Debug, Default)]
struct PolyString {
    grid_done: bool,
    hz: f64,
    power: f64,
    edge: bool,
    /// the reading is due a new grid search (first reading, or the peak was lost)
    want_grid: bool,
}

pub struct Tuner {
    s: Settings,
    sr: f32,
    channels: usize,
    // audio: the mute ramp (0 = muted, 1 = open)
    ramp: f32,
    ramp_step: f32,
    // front end
    hp: Svf,
    aa: [Svf; 2],
    dec: usize,
    dphase: usize,
    fd: f32,
    lp4: [Svf; 2],
    d4phase: usize,
    short: PitchDetector,
    long: PitchDetector,
    fast: PeakFollower,
    slow: PeakFollower,
    hist: Vec<f32>,
    /// decimated samples since reset: the analysis clock
    nd: u64,
    hop_len: usize,
    hop_pos: usize,
    hops: u64,
    running: bool,
    // onsets
    armed: bool,
    onset: u64,
    sounding: bool,
    // targets
    open: [u8; 8],
    n_strings: usize,
    targets: [f64; 8],
    clean: [bool; 8],
    t_poly: [f64; 8],
    /// a shared string's colliding foreign partials within 150 ¢: (string, partial order), count
    collide: [[(usize, u32); 4]; 8],
    n_collide: [usize; 8],
    targets_dirty: bool,
    // coarse and attribution
    clarity: f32,
    coarse_hz: f64,
    cur: Option<Target>,
    pend: Option<(u8, bool, i32)>,
    pend_n: u32,
    /// this note's YIN reads it an octave high: the open string, not its harmonic
    octave_fix: bool,
    // the mono track
    main: Zoom,
    probe: Zoom,
    track_on: bool,
    probe_on: bool,
    probe_decided: bool,
    /// the window's earliest instant, s after the track's first sample
    t_from: f64,
    far_n: u32,
    readings: [f64; READINGS],
    valid: [bool; READINGS],
    sigma_c: f64,
    fine: Option<f64>,
    // poly
    poly: [Zoom; 8],
    ps: [PolyString; 8],
    poly_on: bool,
    poly_decided: bool,
    showing_poly: bool,
    strum_peak: f32,
    level_peak: f32,
    pub counters: Counters,
    // published
    meters: [f32; 23],
    seq: u32,
    shown_cents: f32,
    shown_key: Option<(u8, bool, i32)>,
}

impl Default for Tuner {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn midi_hz(reference: f64, m: f64) -> f64 {
    reference * ((m - 69.0) / 12.0).exp2()
}

#[inline]
fn db(x: f64) -> f64 {
    10.0 * x.max(1e-30).log10()
}

impl Tuner {
    pub fn new() -> Self {
        Self {
            s: Settings::default(),
            sr: 48000.0,
            channels: 1,
            ramp: 1.0,
            ramp_step: 1.0 / 480.0,
            hp: Svf::new(),
            aa: [Svf::new(), Svf::new()],
            dec: 4,
            dphase: 0,
            fd: 12000.0,
            lp4: [Svf::new(), Svf::new()],
            d4phase: 0,
            short: PitchDetector::default(),
            long: PitchDetector::default(),
            fast: PeakFollower::new(0.5, 20.0),
            slow: PeakFollower::new(10.0, 400.0),
            hist: Vec::new(),
            nd: 0,
            hop_len: 480,
            hop_pos: 0,
            hops: 0,
            running: true,
            armed: true,
            onset: 0,
            sounding: false,
            open: [0; 8],
            n_strings: 0,
            targets: [0.0; 8],
            clean: [false; 8],
            t_poly: [0.25; 8],
            collide: [[(0, 0); 4]; 8],
            n_collide: [0; 8],
            targets_dirty: true,
            clarity: 0.0,
            coarse_hz: 0.0,
            cur: None,
            pend: None,
            pend_n: 0,
            octave_fix: false,
            main: Zoom::new(),
            probe: Zoom::new(),
            track_on: false,
            probe_on: false,
            probe_decided: false,
            t_from: 0.0,
            far_n: 0,
            readings: [0.0; READINGS],
            valid: [false; READINGS],
            sigma_c: 0.0,
            fine: None,
            poly: Default::default(),
            ps: [PolyString::default(); 8],
            poly_on: false,
            poly_decided: false,
            showing_poly: false,
            strum_peak: -200.0,
            level_peak: -200.0,
            counters: Counters::default(),
            meters: [0.0; 23],
            seq: 0,
            shown_cents: 0.0,
            shown_key: None,
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn set(&mut self, s: Settings) {
        let old = self.s;
        self.s = s;
        if old.tuning != s.tuning
            || old.reference != s.reference
            || old.transpose != s.transpose
            || old.offsets != s.offsets
            || (s.tuning == 1 && old.strings != s.strings)
            || old.mode != s.mode
        {
            self.targets_dirty = true;
        }
        if old.threshold != s.threshold {
            self.short.set_floor(s.threshold - 10.0);
            self.long.set_floor(s.threshold - 10.0);
        }
    }

    /// The meters as last published (README §3.5, [`METERS`] order).
    pub fn meters(&self) -> &[f32; 23] {
        &self.meters
    }

    /// This hop's strobe reading (Hz) and its error bar (¢), when the strobe reads.
    pub fn fine(&self) -> Option<(f64, f64)> {
        self.fine.map(|f| (f, self.sigma_c))
    }

    /// The decimated analysis rate (Hz).
    pub fn analysis_rate(&self) -> f32 {
        self.fd
    }

    /// The open strings' targets (Hz), whether each is clean, and its poly window (s).
    pub fn string_table(&self) -> Vec<(f64, bool, f64)> {
        (0..self.n_strings).map(|i| (self.targets[i], self.clean[i], self.t_poly[i])).collect()
    }

    fn reset_analysis(&mut self) {
        self.hp.reset();
        self.aa.iter_mut().for_each(Svf::reset);
        self.lp4.iter_mut().for_each(Svf::reset);
        self.dphase = 0;
        self.d4phase = 0;
        self.short.reset();
        self.long.reset();
        self.fast.reset();
        self.slow.reset();
        self.hist.iter_mut().for_each(|v| *v = 0.0);
        self.nd = 0;
        self.hop_pos = 0;
        self.armed = true;
        self.onset = 0;
        self.sounding = false;
        self.end_note();
        self.cur = None;
        self.targets_dirty = true;
        self.meters = [0.0; 23];
        self.meters[M_SEQ] = self.seq as f32;
    }

    /// Forgets the note being read (silence, a new onset).
    fn end_note(&mut self) {
        self.track_on = false;
        self.probe_on = false;
        self.probe_decided = false;
        self.octave_fix = false;
        self.far_n = 0;
        self.valid = [false; READINGS];
        self.fine = None;
        self.pend = None;
        self.pend_n = 0;
        self.poly_decided = false;
        self.showing_poly = false;
        self.ps = [PolyString::default(); 8];
        self.level_peak = -200.0;
    }

    fn precision(&self) -> Precision {
        if self.s.precision == 1 {
            STROBE
        } else {
            FAST
        }
    }

    /// Targets, the clean / shared table and the poly windows (README §2.5); restarts the poly tracks.
    fn update_targets(&mut self) {
        self.targets_dirty = false;
        let (open, n) = self.s.open_strings();
        self.open = open;
        self.n_strings = n;
        let r = self.s.reference as f64;
        for i in 0..8 {
            self.targets[i] = if i < n {
                midi_hz(r, (open[i] as i32 + self.s.transpose) as f64) * (self.s.offsets[i] as f64 / 1200.0).exp2()
            } else {
                0.0
            };
        }
        let mut t_clean_max: f64 = 0.0;
        for i in 0..n {
            let fi = self.targets[i];
            // the nearest foreign partial (orders 1 – 8), in Hz once both strings are 30 ¢ out toward
            // each other (60 ¢ closer)
            let (mut best, mut delta) = (f64::MAX, f64::MAX);
            self.n_collide[i] = 0;
            for l in 0..n {
                if l == i {
                    continue;
                }
                for j in 1..=8u32 {
                    let c = cents(j as f64 * self.targets[l] / fi);
                    let d = c.abs();
                    best = best.min(d);
                    delta = delta.min(fi * (((d - 60.0).max(0.0) / 1200.0 * c.signum()).exp2() - 1.0).abs());
                    if d < 150.0 && self.n_collide[i] < 4 {
                        self.collide[i][self.n_collide[i]] = (l, j);
                        self.n_collide[i] += 1;
                    }
                }
            }
            self.clean[i] = best >= 150.0;
            if self.clean[i] {
                // 5 / Δ, not the design's 4 / Δ: at 4 / Δ that partial sits on the window's main-lobe edge
                // and pulls 1.5 ¢; at 5 / Δ, 0.003 ¢ (measured, README "As coded")
                self.t_poly[i] = (5.0 / delta).clamp(0.25, 1.0);
                t_clean_max = t_clean_max.max(self.t_poly[i]);
            }
        }
        for i in 0..n {
            if !self.clean[i] {
                self.t_poly[i] = t_clean_max.max(0.25);
            }
        }
        self.poly_on = n > 0 && self.s.mode != 1;
        for i in 0..n {
            self.poly[i].start(self.targets[i]);
        }
        self.ps = [PolyString::default(); 8];
        self.poly_decided = false;
        self.showing_poly = false;
    }

    /// The target of a frequency (README §2.3), or None outside every string's ±100 ¢.
    pub fn attribute(&self, f: f64) -> Option<Target> {
        if self.n_strings == 0 {
            let r = self.s.reference as f64;
            let n = (69.0 + 12.0 * (f / r).log2()).round();
            return Some(Target { string: 0, harmonic: false, note: n as i32, hz: midi_hz(r, n) });
        }
        let mut fund = (f64::MAX, 0usize);
        let mut harm = (f64::MAX, 0usize);
        for i in 0..self.n_strings {
            let c1 = cents(f / self.targets[i]).abs();
            let c2 = cents(f / (2.0 * self.targets[i])).abs();
            if c1 < fund.0 {
                fund = (c1, i);
            }
            if c2 < harm.0 {
                harm = (c2, i);
            }
        }
        // ties: a fundamental within 30 ¢ of another string's 2nd partial wins
        let tie = cents(2.0 * self.targets[harm.1] / self.targets[fund.1]).abs() <= 30.0;
        let (d, i, h) = if harm.0 < fund.0 && !tie { (harm.0, harm.1, true) } else { (fund.0, fund.1, false) };
        if d > 100.0 {
            return None;
        }
        let note = self.open[i] as i32 + self.s.transpose + if h { 12 } else { 0 };
        Some(Target { string: i as u8 + 1, harmonic: h, note, hz: self.targets[i] * if h { 2.0 } else { 1.0 } })
    }

    /// The target for a frequency: the strings' target, else chromatic (string 0).
    fn target_of(&self, f: f64) -> Target {
        self.attribute(f).unwrap_or_else(|| {
            let r = self.s.reference as f64;
            let n = (69.0 + 12.0 * (f / r).log2()).round();
            Target { string: 0, harmonic: false, note: n as i32, hz: midi_hz(r, n) }
        })
    }

    /// The coarse reading of this hop: (Hz, clarity), the detector's window lying inside the note.
    fn coarse(&self) -> Option<(f64, f32)> {
        let since = (self.nd - self.onset) as f32 / self.fd;
        type Pick = (f64, f32, [(f32, f32); 2]);
        let pick = |d: &PitchDetector| -> Option<Pick> {
            let hz = d.hz()?;
            if since < d.window_s() + 1.0 / hz {
                return None;
            }
            Some((hz as f64, d.clarity(), d.candidates()))
        };
        // the long window only below the short one's range (above it, it reads sub-harmonics), or when
        // the short one is barely voiced on a frequency that is no harmonic of the long one's clear
        // reading (a low B with a weak fundamental makes the short window voice a partial cluster)
        let low = self.short.period_hz() < 200.0;
        let long = if low { pick(&self.long) } else { None };
        let spurious = |s: &Pick, l: &Pick| {
            let k = (s.0 / l.0).round().max(1.0);
            s.1 < 0.9 && 1.0 - l.1 < 0.5 * (1.0 - s.1) && cents(s.0 / (k * l.0)).abs() > 50.0
        };
        let (mut hz, clarity, cands) = match (pick(&self.short), long) {
            (Some(s), Some(l)) if spurious(&s, &l) => l,
            // a barely voiced short reading waits for the long window's first answer (B0's 33 ms
            // period inside it), so that a wrong note is not flashed
            (Some(s), None) if s.1 < 0.9 && low && since < self.long.window_s() + 1.0 / 30.0 => return None,
            (Some(s), _) => s,
            (None, l) => l?,
        };
        if self.n_strings > 0 {
            // octave errors: two candidates an octave apart, only one of them on a string (or its 2×)
            let other = cands[1].0 as f64;
            if other > 0.0 && (cents(hz / other).abs() - 1200.0).abs() < 60.0 {
                // (a margin of 20 ¢, so that a string 100 ¢ out does not flip between the two)
                let near = |f: f64, lim: f64| self.attribute(f).is_some_and(|t| cents(f / t.hz).abs() <= lim);
                if !near(hz, 100.0) && near(other, 80.0) {
                    hz = other;
                }
            }
            if self.octave_fix {
                if let Some(t) = self.attribute(hz) {
                    if t.harmonic {
                        hz *= 0.5;
                    }
                }
            }
        }
        Some((hz, clarity))
    }

    /// Starts the mono track on `f`, replaying the input since the onset (or as far as the history
    /// holds) so that a late coarse reading costs no time.
    fn start_track(&mut self, f: f64) {
        let n0 = self.onset.max(self.nd.saturating_sub(HIST as u64 - 1));
        self.counters.starts += 1;
        self.main.start(f);
        for n in n0..self.nd {
            let v = self.hist[(n as usize) % HIST];
            self.main.push(v);
        }
        // the window never reaches back before the onset + max(3 periods, 25 ms) + 2 periods (the
        // wide filter), nor before 2 periods of the track
        let since_onset = (n0 - self.onset) as f64 / self.fd as f64;
        let hold = (3.0 / f).max(0.025) + 2.0 / f;
        self.t_from = (hold - since_onset).max(2.0 / f);
        self.track_on = true;
        self.probe_on = false;
        self.probe_decided = false;
        self.far_n = 0;
        self.valid = [false; READINGS];
        self.fine = None;
    }

    fn start_probe(&mut self, f: f64) {
        let n0 = self.onset.max(self.nd.saturating_sub(HIST as u64 - 1));
        self.probe.start(f);
        for n in n0..self.nd {
            let v = self.hist[(n as usize) % HIST];
            self.probe.push(v);
        }
        self.probe_on = true;
    }

    /// One decimated sample through the analysis.
    #[inline]
    fn analyse_sample(&mut self, y: f32) {
        self.short.push_decimated(y);
        let l = self.lp4[0].process(y);
        let l = self.lp4[1].process(l);
        self.d4phase += 1;
        if self.d4phase == 4 {
            self.d4phase = 0;
            self.long.push_decimated(l);
        }
        let fa = self.fast.next(y);
        let sl = self.slow.next(y);
        // onset: the fast envelope 6 dB over the slow one, above the threshold; re-armed under 3.5 dB
        let thr = 10f32.powf(0.05 * self.s.threshold);
        if self.armed {
            if fa > 2.0 * sl && fa > thr && (!self.sounding || self.nd >= self.onset + (0.05 * self.fd) as u64) {
                self.armed = false;
                self.onset = self.nd;
                self.end_note();
                self.cur = None;
                self.counters.onsets += 1;
            }
        } else if fa < 1.5 * sl {
            self.armed = true;
        }
        self.hist[(self.nd as usize) % HIST] = y;
        self.nd += 1;
        if self.track_on {
            self.main.push(y);
        }
        if self.probe_on {
            self.probe.push(y);
        }
        if self.poly_on {
            for i in 0..self.n_strings {
                self.poly[i].push(y);
            }
        }
    }

    /// The work of one 10 ms hop: coarse, attribution, strobe, poly, the meters.
    fn hop(&mut self) {
        self.hops += 1;
        self.seq = (self.seq + 1) % 1024;
        self.meters[M_SEQ] = self.seq as f32;
        if self.targets_dirty {
            self.update_targets();
        }
        let level = 20.0 * self.slow.value().max(1e-10).log10();
        if level < self.s.threshold {
            if self.sounding {
                self.sounding = false;
                self.end_note();
                self.cur = None;
            }
            self.publish_silence();
            return;
        }
        if !self.sounding {
            self.sounding = true;
            // a note that grew in without an onset starts here
            if self.nd > self.onset + (0.05 * self.fd) as u64 {
                self.onset = self.nd;
                self.end_note();
            }
        }
        self.level_peak = self.level_peak.max(level);
        let prec = self.precision();
        let coarse = self.coarse();
        if let Some((hz, clarity)) = coarse {
            self.clarity = clarity;
            // the mono track follows what is heard: a restart when the coarse reading leaves the wide
            // filter's flat part (170 ¢) two hops running
            if !self.track_on {
                self.start_track(hz);
            } else if cents(hz / self.main.fc()).abs() > 170.0 {
                self.far_n += 1;
                if self.far_n >= 2 {
                    self.counters.far += 1;
                    self.start_track(hz);
                }
            } else {
                self.far_n = 0;
            }
            // the track's coarse reading (a reading 170 ¢ away is another note, or a glitch, until the
            // restart): the capture guard's centre, and the reading shown before the strobe's
            if cents(hz / self.main.fc()).abs() <= 170.0 {
                self.coarse_hz = hz;
            }
            // note hysteresis: a new target wins 2 hops running (chromatic: and > 60 ¢ from the old)
            let t = self.target_of(hz);
            match self.cur {
                None => self.cur = Some(t),
                Some(c) if c.key() == t.key() => {
                    self.cur = Some(Target { hz: t.hz, ..c });
                    self.pend = None;
                    self.pend_n = 0;
                }
                Some(c) => {
                    let far = c.string != 0 || t.string != 0 || cents(hz / c.hz).abs() > 60.0;
                    if far {
                        if self.pend == Some(t.key()) {
                            self.pend_n += 1;
                        } else {
                            self.pend = Some(t.key());
                            self.pend_n = 1;
                        }
                        if self.pend_n >= 2 {
                            self.cur = Some(t);
                            self.pend = None;
                            self.pend_n = 0;
                        }
                    }
                }
            }
        }
        // the strobe
        self.fine = None;
        if self.track_on {
            let fc = self.main.fc();
            let avail = self.main.elapsed() - self.t_from;
            let t = avail.min(prec.t_max(fc));
            if t >= prec.t_min(fc) {
                let from = (self.t_from * self.main.fb()).ceil() as u64;
                // capture guard: the estimate may not leave ±35 ¢ around the coarse reading; held at
                // the edge (a louder neighbour pulled it), the hop shows the coarse reading only
                let (lo, hi) = (self.coarse_hz * (-35.0f64 / 1200.0).exp2(), self.coarse_hz * (35.0f64 / 1200.0).exp2());
                self.main.set_capture(lo, hi);
                if let Some(e) = self.main.estimate(t, from) {
                    if e.clamped {
                        self.counters.guard += 1;
                    } else if e.hz.is_finite() && e.hz > 0.0 {
                        self.fine = Some(e.hz);
                        self.sigma_c = cents((e.hz + e.sigma_hz) / e.hz).abs();
                    }
                }
            }
            self.harmonic_probe();
        }
        // readings history (the settled rule)
        let k = (self.hops as usize) % READINGS;
        self.valid[k] = self.fine.is_some();
        self.readings[k] = self.fine.map_or(0.0, |f| cents(f / 440.0));
        let settled = self.fine.is_some() && self.track_on && {
            let fc = self.main.fc();
            let span = prec.span.max(0.5 * prec.t_max(fc));
            let n = ((span / HOP_S).round() as usize).min(READINGS - 1);
            let (mut lo, mut hi, mut ok) = (f64::MAX, f64::MIN, true);
            for j in 0..=n {
                let q = (self.hops as usize + READINGS - j) % READINGS;
                if !self.valid[q] {
                    ok = false;
                    break;
                }
                lo = lo.min(self.readings[q]);
                hi = hi.max(self.readings[q]);
            }
            ok && hi - lo <= prec.tol
        };
        self.poly_hop(level);
        self.publish(settled, level);
    }

    /// Open string or 12th-fret harmonic (README §2.3): when the target is a string's 2nd partial, a
    /// probe at the fundamental runs beside the track; harmonic when its band is ≥ 20 dB under.
    fn harmonic_probe(&mut self) {
        let Some(t) = self.cur else { return };
        if !t.harmonic || self.probe_decided || self.octave_fix {
            return;
        }
        let f1 = self.main.fc() * 0.5;
        if !self.probe_on {
            self.start_probe(f1);
        }
        // one Hann frame of 4 fundamental periods in both bands, past the hold-off
        let frame = 4.0 / f1;
        let since = (self.nd - self.onset) as f64 / self.fd as f64;
        if since < (3.0 / f1).max(0.025) + 2.0 / f1 + frame {
            return;
        }
        let p_main = self.main.frame_power(frame, self.main.delta());
        let p_probe = self.probe.frame_power(frame, 0.5 * self.main.delta());
        self.probe_decided = true;
        if db(p_probe) - db(p_main) > -20.0 {
            // the fundamental is there: the open string, read an octave high; the strobe moves down
            self.octave_fix = true;
            self.counters.octave_fixes += 1;
            std::mem::swap(&mut self.main, &mut self.probe);
            self.probe_on = false;
            let since_track = self.main.elapsed();
            let hold = (3.0 / f1).max(0.025) + 2.0 / f1;
            self.t_from = (hold - (since - since_track)).max(2.0 / f1);
            self.valid = [false; READINGS];
            self.fine = None;
            self.coarse_hz *= 0.5;
            let s = t.string as usize - 1;
            self.cur = Some(Target { string: t.string, harmonic: false, note: t.note - 12, hz: self.targets[s] });
        } else {
            self.probe_on = false;
        }
    }

    /// Poly: grid searches, refinements every 50 ms, the strum decision (README §2.5).
    fn poly_hop(&mut self, level: f32) {
        if !self.poly_on || self.n_strings == 0 {
            self.showing_poly = false;
            return;
        }
        let since = (self.nd - self.onset) as f64 / self.fd as f64;
        // the windows start 30 ms after the first onset (the strum's spread) + 2 periods (the filter)
        let hold = 0.03;
        // one grid search per hop (the heavy part), each string's as soon as its own window is full,
        // and the 50 ms refreshes staggered over the hops: no block carries them all
        let mut budget = 1;
        let (fd, nd, onset) = (self.fd as f64, self.nd, self.onset);
        for i in 0..self.n_strings {
            let refresh = (self.hops + i as u64).is_multiple_of(5);
            let fi = self.targets[i];
            if since < hold + 2.0 / fi + self.t_poly[i] {
                continue;
            }
            let z = &mut self.poly[i];
            let from_s = hold + 2.0 / fi;
            // the window's earliest instant, as a ring index of this track
            let track_start = nd as f64 / fd - z.elapsed();
            let from_idx = ((onset as f64 / fd + from_s - track_start) * z.fb()).max(0.0).ceil() as u64;
            let t = self.t_poly[i].min(since - from_s);
            let st = &mut self.ps[i];
            if !st.grid_done || st.want_grid {
                if budget == 0 {
                    continue;
                }
                budget -= 1;
                self.counters.grids += 1;
                if let Some((hz, p, edge)) = z.grid_peak(t, from_idx, 60.0, 2.5) {
                    st.grid_done = true;
                    st.want_grid = false;
                    st.edge = edge;
                    st.hz = hz;
                    st.power = p;
                    if !edge {
                        if let Some((h, p)) = z.refine(t, from_idx, hz) {
                            st.hz = h;
                            st.power = p;
                        }
                    }
                }
            } else if refresh && !st.edge {
                if let Some((h, p)) = z.refine(t, from_idx, st.hz) {
                    if cents(h / fi).abs() > 60.0 {
                        st.want_grid = true;
                    } else {
                        st.hz = h;
                        st.power = p;
                    }
                }
            } else if refresh && st.edge {
                st.want_grid = true;
            }
        }
        if !self.poly_decided {
            let all_clean_done = (0..self.n_strings).all(|i| !self.clean[i] || self.ps[i].grid_done);
            if !all_clean_done {
                return;
            }
            self.poly_decided = true;
            let n_clean = (0..self.n_strings).filter(|&i| self.clean[i]).count();
            let lit = (0..self.n_strings).filter(|&i| self.clean[i] && self.lit(i)).count();
            let strum = n_clean > 0 && lit >= n_clean.clamp(2, 3);
            self.showing_poly = self.s.mode == 2 || (self.s.mode == 0 && strum);
            self.strum_peak = self.level_peak;
        } else if self.showing_poly && level < self.strum_peak - 40.0 {
            self.showing_poly = false;
        }
    }

    /// Amplitude (dBFS) of a poly string's peak.
    fn poly_db(&self, i: usize) -> f64 {
        db(4.0 * self.ps[i].power)
    }

    /// A poly string shows a peak: inside the grid, within 30 dB of the loudest, above the threshold.
    fn lit(&self, i: usize) -> bool {
        let st = &self.ps[i];
        if !st.grid_done || st.edge {
            return false;
        }
        let loudest = (0..self.n_strings).filter(|&j| self.ps[j].grid_done).map(|j| self.poly_db(j)).fold(-400.0, f64::max);
        let p = self.poly_db(i);
        p >= loudest - 30.0 && p >= self.s.threshold as f64
    }

    fn publish_silence(&mut self) {
        for (k, m) in self.meters.iter_mut().enumerate() {
            if k != M_SEQ {
                *m = 0.0;
            }
        }
        self.shown_key = None;
    }

    fn publish(&mut self, settled: bool, level: f32) {
        match self.cur.filter(|_| self.track_on) {
            Some(t) => self.publish_mono(t, settled, level),
            // a strum gives the coarse stage a poor reading: the strings are shown without one
            None if self.showing_poly => {
                self.publish_silence();
                self.meters[M_STATE] = STATE_STRUM;
            }
            None => {
                self.publish_silence();
                return;
            }
        }
        self.publish_strings();
    }

    fn publish_mono(&mut self, t: Target, settled: bool, level: f32) {
        let f = self.fine.unwrap_or(self.coarse_hz);
        let c = cents(f / t.hz).clamp(-100.0, 100.0) as f32;
        // dead band: the needle moves by more than σ / 2 or 0.05 ¢
        let fine = self.fine.is_some();
        let band = if fine { (0.5 * self.sigma_c).max(0.05) as f32 } else { 0.05 };
        if self.shown_key != Some(t.key()) || (c - self.shown_cents).abs() > band || c.abs() >= 100.0 {
            self.shown_cents = c;
            self.shown_key = Some(t.key());
        }
        let level_term = ((level - self.s.threshold) / 12.0).clamp(0.0, 1.0);
        let strobe_term = if fine {
            let s = self.sigma_c.max(1e-9);
            if s <= 0.1 {
                1.0
            } else {
                (1.0 - (s / 0.1).ln() / 30f64.ln()).max(0.0) as f32
            }
        } else {
            1.0
        };
        self.meters[M_FREQ] = f as f32;
        self.meters[M_NOTE] = t.note.clamp(0, 127) as f32;
        self.meters[M_CENTS] = self.shown_cents;
        self.meters[M_CONF] = (self.clarity * level_term * strobe_term).clamp(0.0, 1.0);
        self.meters[M_STRING] = t.string as f32;
        self.meters[M_STATE] = if self.showing_poly {
            STATE_STRUM
        } else if settled {
            STATE_SETTLED
        } else {
            STATE_SETTLING
        };
    }

    /// The strings' columns (0 when not showing a strum).
    fn publish_strings(&mut self) {
        for i in 0..8 {
            let (mut sc, mut cf) = (0.0, 0.0);
            if self.showing_poly && i < self.n_strings && self.ps[i].grid_done {
                let st = self.ps[i];
                let c = cents(st.hz / self.targets[i]);
                if st.edge {
                    sc = if c < 0.0 { -100.0 } else { 100.0 };
                } else if self.lit(i) {
                    sc = c.clamp(-100.0, 100.0) as f32;
                    let lt = ((self.poly_db(i) as f32 - self.s.threshold) / 12.0).clamp(0.0, 1.0);
                    let base = if self.clean[i] || self.separated(i) { 1.0 } else { 0.5 };
                    cf = base * lt;
                }
            }
            self.meters[M_SCENTS + i] = sc;
            self.meters[M_SCONF + i] = cf;
        }
    }

    /// A shared string whose peak lies more than 1.5 main lobes (6 / T) from every colliding partial,
    /// those strings having just been read: the peak is its own (README §2.5 says one main lobe, to set
    /// on the bench; one lobe left 2 ¢ of pull on synthetic strums).
    fn separated(&self, i: usize) -> bool {
        (0..self.n_collide[i]).all(|c| {
            let (l, k) = self.collide[i][c];
            self.lit(l) && (self.ps[i].hz - k as f64 * self.ps[l].hz).abs() > 6.0 / self.t_poly[i]
        })
    }
}

impl Effect for Tuner {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate;
        self.channels = channels.clamp(1, 2);
        self.dec = (sample_rate / FD_TARGET).round().max(1.0) as usize;
        self.fd = sample_rate / self.dec as f32;
        let fd = self.fd;
        self.hp.set(SvfMode::HighPass, 18.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, sample_rate);
        // the anti-alias low-pass of gfx-dsp `detect`
        let fc = (0.2 * fd).min(2500.0);
        for s in self.aa.iter_mut() {
            s.set(SvfMode::LowPass, fc, std::f32::consts::FRAC_1_SQRT_2, 0.0, sample_rate);
        }
        // ÷4 for the long detector: a 4th-order Butterworth at 600 Hz
        self.lp4[0].set(SvfMode::LowPass, 600.0, 0.541_196, 0.0, fd);
        self.lp4[1].set(SvfMode::LowPass, 600.0, 1.306_563, 0.0, fd);
        self.short.prepare_decimated(fd, 60.0, 1800.0, false);
        self.short.set_hop((HOP_S as f32 * fd).round() as usize);
        self.long.prepare_decimated(fd / 4.0, 25.0, 150.0, true);
        self.long.set_hop((HOP_S as f32 * fd / 4.0).round() as usize);
        self.short.set_floor(self.s.threshold - 10.0);
        self.long.set_floor(self.s.threshold - 10.0);
        self.fast.prepare(fd);
        self.slow.prepare(fd);
        self.hist = vec![0.0; HIST];
        self.main.prepare(fd);
        self.probe.prepare(fd);
        for z in self.poly.iter_mut() {
            z.prepare(fd);
        }
        self.hop_len = (HOP_S * sample_rate as f64).round() as usize;
        self.ramp_step = 1.0 / (RAMP_S * sample_rate);
        self.reset();
    }

    fn reset(&mut self) {
        self.counters = Counters::default();
        self.ramp = if self.s.mute && !self.s.bypass { 0.0 } else { 1.0 };
        self.seq = 0;
        self.hops = 0;
        self.running = !self.s.bypass || self.s.always_on;
        self.reset_analysis();
        self.shown_key = None;
    }

    fn set_param(&mut self, id: &str, value: f32) {
        let mut s = self.s;
        let b = value >= 0.5;
        let i = value.round();
        match id {
            "tuning" => s.tuning = i.clamp(0.0, 21.0) as usize,
            "mode" => s.mode = i.clamp(0.0, 2.0) as usize,
            "precision" => s.precision = i.clamp(0.0, 1.0) as usize,
            "reference" => s.reference = value.clamp(410.0, 470.0),
            "mute" => s.mute = b,
            "bypass" => s.bypass = b,
            "transpose" => s.transpose = i.clamp(-12.0, 12.0) as i32,
            "threshold" => s.threshold = value.clamp(-90.0, -30.0),
            "always_on" => s.always_on = b,
            _ => {
                if let Some(n) = id.strip_prefix("offset_").and_then(|n| n.parse::<usize>().ok()).filter(|n| (1..=8).contains(n)) {
                    s.offsets[n - 1] = value.clamp(-50.0, 50.0);
                } else if let Some(n) = id.strip_prefix("string_").and_then(|n| n.parse::<usize>().ok()).filter(|n| (1..=8).contains(n)) {
                    s.strings[n - 1] = i.clamp(0.0, 96.0) as u8;
                } else {
                    return;
                }
            }
        }
        self.set(s);
    }

    fn meter(&self, id: &str) -> Option<f32> {
        METERS.iter().position(|m| *m == id).map(|k| self.meters[k])
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        let n = buffers.first().map_or(0, |b| b.len());
        let ch = buffers.len().min(2);
        let engaged = !self.s.bypass;
        let run = engaged || self.s.always_on;
        if run != self.running {
            self.running = run;
            if !run {
                self.reset_analysis();
            }
        }
        let target = if self.s.mute && engaged { 0.0 } else { 1.0 };
        for i in 0..n {
            if self.running {
                let x = if ch == 2 { 0.5 * (buffers[0][i] + buffers[1][i]) } else { buffers[0][i] };
                let h = self.hp.process(x);
                let a = self.aa[0].process(h);
                let y = self.aa[1].process(a);
                self.dphase += 1;
                if self.dphase >= self.dec {
                    self.dphase = 0;
                    self.analyse_sample(y);
                }
                self.hop_pos += 1;
                if self.hop_pos >= self.hop_len {
                    self.hop_pos = 0;
                    self.hop();
                }
            }
            // the mute ramp: a raised cosine; at g = 1 the buffer is left untouched (bit-exact)
            if self.ramp != target {
                self.ramp = if target > self.ramp { (self.ramp + self.ramp_step).min(1.0) } else { (self.ramp - self.ramp_step).max(0.0) };
            }
            if self.ramp < 1.0 {
                let g = 0.5 - 0.5 * (std::f32::consts::PI * self.ramp).cos();
                for b in buffers.iter_mut() {
                    b[i] *= g;
                }
            }
        }
    }
}
