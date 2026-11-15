// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-grains — golem's granular processor and granular delay (design: README.md).
//!
//! - **RECORD** (gfx-dsp `grain::MipBuffer`): a rolling buffer of 2 × 10 s + 0.5 s with its half-rate
//!   copy, always recording the input plus the feedback. **HOLD**: the last `length` copied out at 8
//!   frames per sample after a freeze (copy-on-freeze), with its own half-rate copy.
//! - **Grains** (≤ 128, gfx-dsp `GrainPool`): drawn once at their start from twelve seeded variates in a
//!   fixed order (position, size, pitch, detune, direction, pan, chance, four re-draws, one spare),
//!   read band-limited (gfx-dsp `KernelBank`, the octave switch; the unity path copies samples), with a
//!   window of exact phase (flat → triangle → Hann, `skew`, C¹ edges).
//! - **Four schedulers** (`mode`): Cloud (free onsets, Poisson ↔ metronomic), Rhythm (host grid,
//!   clusters on whole steps), Strum (the recent onsets of gfx-dsp `onset::Pick`), Phrase (the last
//!   phrase cut into slices, looper crossfades at the seams).
//! - **Layers**: the live cloud and up to two holds (a hold releasing under the live cloud or under a
//!   new hold); a third ends the oldest in 50 ms. While two sound, each is capped at 48 grains.
//! - **Level**: each layer's gain sits between the incoherent and coherent bounds of its cloud's
//!   statistics, at the point a 400 ms power ratio measures (pinned to the coherent bound on the unity
//!   path).
//! - **Wet** → low / high cut → × mix over a unity dry (`kill_dry`); → DC blocker → soft clip →
//!   × `feedback` into RECORD.
//!
//! Zero latency; everything per sample (tick of 32 samples from reset for the per-tick work), so the
//! output does not depend on the host's block size. Real-time safe after `prepare`; deterministic.

#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

#[cfg(feature = "plugin")]
mod plugin;

#[cfg(test)]
mod tests;

use gfx_dsp::cuts::Cuts;
use gfx_dsp::detect::SCALES;
use gfx_dsp::grain::{GrainPool, KernelBank, MipBuffer, Window, WindowOsc, WindowShape};
use gfx_dsp::lfo::Rng;
use gfx_dsp::onset::{Level, Pick, PickSettings, Source, Strum};
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::transport::{Sync, TransportInfo};
use gfx_dsp::{Effect, ParamInfo, ParamKind};

pub const NP: usize = 32;

const fn int(id: &'static str, name: &'static str, min: f32, max: f32, default: f32) -> ParamInfo {
    ParamInfo { id, name, min, max, default, kind: ParamKind::Int, exp: false }
}

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::boolean("freeze", "Freeze", false),
    int("grains", "Grains", 1.0, 64.0, 8.0).with_exp(),
    ParamInfo::float("pos", "Grain Position", 0.0, 1.0, 0.5).with_exp(),
    ParamInfo::float("size", "Size", 2.0, 2000.0, 120.0).with_exp(),
    ParamInfo::float("pitch", "Pitch", -24.0, 24.0, 0.0).with_exp(),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 50.0).with_exp(),
    ParamInfo::enumeration("mode", "Mode", 4, 0),
    ParamInfo::float("length", "Length", 50.0, 10000.0, 1000.0),
    ParamInfo::enumeration("length_sync", "Length Sync", 17, 0),
    ParamInfo::float("spray", "Spray", 0.0, 100.0, 10.0).with_exp(),
    ParamInfo::float("scan", "Scan", -200.0, 200.0, 0.0).with_exp(),
    ParamInfo::float("regular", "Regularity", 0.0, 100.0, 0.0),
    ParamInfo::float("chance", "Chance", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::float("shape", "Shape", 0.0, 100.0, 100.0),
    ParamInfo::float("skew", "Skew", -100.0, 100.0, 0.0),
    ParamInfo::float("reverse", "Reverse", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("pitch_rand", "Pitch Random", 0.0, 24.0, 0.0).with_exp(),
    ParamInfo::enumeration("scale", "Scale", 10, 0),
    ParamInfo::float("detune", "Detune", 0.0, 50.0, 0.0),
    ParamInfo::float("glide", "Glide", -24.0, 24.0, 0.0),
    ParamInfo::float("spread", "Spread", 0.0, 100.0, 50.0),
    ParamInfo::enumeration("sync", "Sync", 17, 0),
    int("seed", "Seed", 0.0, 999.0, 0.0),
    ParamInfo::float("skip", "Attack Skip", 0.0, 100.0, 0.0),
    ParamInfo::float("live", "Live Level", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::float("decay", "Decay", 0.05, 10.0, 1.0),
    ParamInfo::float("feedback", "Feedback", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("low_cut", "Low Cut", 20.0, 1000.0, 20.0),
    ParamInfo::float("high_cut", "High Cut", 1000.0, 20000.0, 20000.0).with_exp(),
    ParamInfo::boolean("kill_dry", "Kill Dry", false),
    ParamInfo::boolean("trails", "Trails", true),
    ParamInfo::boolean("bypass", "Bypass", false),
];

pub mod p {
    pub const FREEZE: usize = 0;
    pub const GRAINS: usize = 1;
    pub const POS: usize = 2;
    pub const SIZE: usize = 3;
    pub const PITCH: usize = 4;
    pub const MIX: usize = 5;
    pub const MODE: usize = 6;
    pub const LENGTH: usize = 7;
    pub const LENGTH_SYNC: usize = 8;
    pub const SPRAY: usize = 9;
    pub const SCAN: usize = 10;
    pub const REGULAR: usize = 11;
    pub const CHANCE: usize = 12;
    pub const SHAPE: usize = 13;
    pub const SKEW: usize = 14;
    pub const REVERSE: usize = 15;
    pub const PITCH_RAND: usize = 16;
    pub const SCALE: usize = 17;
    pub const DETUNE: usize = 18;
    pub const GLIDE: usize = 19;
    pub const SPREAD: usize = 20;
    pub const SYNC: usize = 21;
    pub const SEED: usize = 22;
    pub const SKIP: usize = 23;
    pub const LIVE: usize = 24;
    pub const DECAY: usize = 25;
    pub const FEEDBACK: usize = 26;
    pub const LOW_CUT: usize = 27;
    pub const HIGH_CUT: usize = 28;
    pub const KILL_DRY: usize = 29;
    pub const TRAILS: usize = 30;
    pub const BYPASS: usize = 31;
}

/// Mode labels (enum order, final; append only).
pub const MODES: [&str; 4] = ["Cloud", "Rhythm", "Strum", "Phrase"];
/// Scale labels (enum order, final; append only).
pub const SCALE_NAMES: [&str; 10] = ["Off", "Chromatic", "Octaves", "Fifths", "Major Triad", "Minor Triad", "Major", "Minor", "Major Pent", "Minor Pent"];
/// The read-only meters (`Effect::meter`): voices sounding, and 0 Live · 1 Held · 2 Releasing.
pub const METERS: [&str; 2] = ["active", "state"];

const CLOUD: usize = 0;
const RHYTHM: usize = 1;
const STRUM: usize = 2;
const PHRASE: usize = 3;

/// The longest `length` (s) and the RECORD span (2 × that + a margin).
pub const LMAX_S: f64 = 10.0;
const RECORD_S: f64 = 2.0 * LMAX_S + 0.5;
/// Voices.
pub const POOL: usize = 128;
/// Per-layer cap while two layers sound (and while a third ends, its 50 ms).
pub const CAP_TWO: f64 = 48.0;
pub const CAP_THREE: f64 = 32.0;
/// The per-tick work's grid (samples from reset).
pub const TICK: u64 = 32;
/// Onset map block (frames).
const BLOCK: u64 = 32;
/// Live layer + three hold slots (two sounding, one ending).
const NL: usize = 4;
/// Frames kept clear of a level's ends (kernel half-length, half-band delay).
const MARGIN: f64 = 80.0;
/// Minimum read distances (README §2.1).
const DMIN0_MS: f64 = 1.5;
const DMIN1_MS: f64 = 2.5;
/// The longest grain (s): `size` 2 s with +25 % jitter.
const MAX_GRAIN_S: f64 = 2.5;
/// Copy-on-freeze speed (frames per sample).
const COPY_RATE: usize = 8;
/// A strum's window (ms): onsets closer than this are one (gfx-dsp `onset::Strum`).
const STRUM_MS: f32 = 60.0;
/// The weakest onset Strum mode plays from (dB of rise; the module's default sensitivity).
const STRUM_RISE_DB: f32 = 7.5;
/// Rise bytes: 0.25 dB per step, 0 = no mark.
const RISE_SCALE: f32 = 4.0;
/// Onsets remembered (ring).
const ONSETS: usize = 64;
/// Gain bounds of the level normalisation.
const G_MAX: f64 = 31.6;

#[inline]
fn db_to_gain(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// Reflects x into [lo, hi] (a triangle fold, never a clamp: README §2.2). None when the range is empty.
#[inline]
fn reflect(x: f64, lo: f64, hi: f64) -> Option<f64> {
    if hi.partial_cmp(&lo).is_none_or(|o| o == std::cmp::Ordering::Less) {
        return None;
    }
    let w = hi - lo;
    if w < 1e-9 {
        return Some(lo);
    }
    let mut y = (x - lo).rem_euclid(2.0 * w);
    if y > w {
        y = 2.0 * w - y;
    }
    Some(lo + y)
}

/// Source span of a grain: Σ of its ratio over `len` samples (exponential glide).
#[inline]
fn span(r0: f64, mul: f64, len: f64) -> f64 {
    if mul == 1.0 {
        r0 * len
    } else {
        r0 * (mul.powf(len) - 1.0) / mul.ln()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Law {
    /// amplitude-complementary raised cosine (contiguous slices: exact unity)
    #[default]
    Comp,
    /// equal power (a jump: wrap, shuffle, reverse, pitch, rest)
    Power,
}

#[derive(Clone, Copy, Debug, Default)]
struct Grain {
    layer: u8,
    /// reads HOLD (else RECORD)
    hold: bool,
    t_on: f64,
    inv_len: f64,
    len: f64,
    /// read position (level-0 frames, the buffer's own coordinates) at the next sample
    pos: f64,
    /// signed ratio per sample
    step: f64,
    /// glide factor per sample (1 = none)
    mul: f64,
    unity: bool,
    level: u8,
    kernel: u8,
    plevel: u8,
    pkernel: u8,
    /// samples left of a kernel / level crossfade
    xf: u8,
    win: WindowShape,
    osc: WindowOsc,
    /// Phrase slices: fades of `x` samples in and out, the plain part ends at `len − x`
    slice: bool,
    x: f64,
    law_in: Law,
    law_out: Law,
    gl: f32,
    gr: f32,
    /// the layer's normalisation gain when the grain started (fixed for its life: a density change
    /// crossfades through the grains' lives, never steps)
    amp: f32,
    dead: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum State {
    #[default]
    Free,
    Live,
    Held,
    Releasing,
    Ending,
}

/// The statistics of a layer's cloud (README §2.5), recomputed when their inputs change.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct StatKey {
    shape: f32,
    skew: f32,
    size: f64,
    n_grid: f64,
    m: f64,
    chance: f32,
    irr: f32,
    mode: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct Stats {
    n: f64,
    ew2: f64,
    /// the coherent bound of A / B
    hi: f64,
}

/// One layer's sums over the current tick (the grains are rendered a tick ahead, grain by grain:
/// none reads a sample of its own tick, README §2.1).
#[derive(Clone, Copy, Debug)]
struct TickBuf {
    l: [f32; TICK as usize],
    r: [f32; TICK as usize],
    /// the layer's mid before its gain, and Σ (w·mid)² (the level measurement)
    m: [f32; TICK as usize],
    b: [f32; TICK as usize],
    /// the mid with the grains' gains (the mono output)
    o: [f32; TICK as usize],
}

impl Default for TickBuf {
    fn default() -> Self {
        Self { l: [0.0; TICK as usize], r: [0.0; TICK as usize], m: [0.0; TICK as usize], b: [0.0; TICK as usize], o: [0.0; TICK as usize] }
    }
}

/// Renders samples `k0 .. k1` of the tick starting at `t0` of one grain into its layer's sums.
/// Returns true once the grain has ended.
#[allow(clippy::too_many_arguments)]
fn render_grain(g: &mut Grain, k0: usize, k1: usize, t0: f64, rec: &MipBuffer, hold: &MipBuffer, bank: &KernelBank, win: &Window, mono: bool, tb: &mut TickBuf) -> bool {
    const N: usize = TICK as usize;
    let mut w = [0.0f32; N];
    let mut end = k1;
    let mut dead = false;
    for k in k0..k1 {
        let t = t0 + k as f64;
        let phi = (t - g.t_on) * g.inv_len;
        if phi >= 1.0 {
            end = k;
            dead = true;
            break;
        }
        w[k] = if g.slice {
            let tau = t - g.t_on;
            let plain = g.len - g.x;
            if tau < g.x {
                let u = (tau / g.x) as f32;
                match g.law_in {
                    Law::Comp => 0.5 - 0.5 * (std::f32::consts::PI * u).cos(),
                    Law::Power => (std::f32::consts::FRAC_PI_2 * u).sin(),
                }
            } else if tau >= plain {
                let u = ((tau - plain) / g.x) as f32;
                match g.law_out {
                    Law::Comp => 0.5 + 0.5 * (std::f32::consts::PI * u).cos(),
                    Law::Power => (std::f32::consts::FRAC_PI_2 * u).cos(),
                }
            } else {
                1.0
            }
        } else {
            let h = if g.win.wh > 0.0 { g.win.wh * g.osc.next(&g.win, phi) } else { 0.0 };
            h + win.value_ft(&g.win, phi)
        };
    }
    if end <= k0 {
        return dead;
    }
    let buf = if g.hold { hold } else { rec };
    let mut v = [[0.0f32; 2]; N];
    if g.unity {
        let mut p = g.pos as u64;
        if g.step > 0.0 {
            for o in v[k0..end].iter_mut() {
                *o = buf.read_int(p);
                p += 1;
            }
        } else {
            for o in v[k0..end].iter_mut() {
                *o = buf.read_int(p);
                p = p.wrapping_sub(1);
            }
        }
        g.pos += g.step * (end - k0) as f64;
    } else if g.xf > 0 {
        // a kernel or level change: the old and the new read crossfaded over this tick
        let (mut pp, mut ps) = (g.pos, g.step);
        buf.read_run(bank, g.plevel as usize, g.pkernel as usize, &mut pp, &mut ps, g.mul, &mut v[k0..end]);
        let mut nv = [[0.0f32; 2]; N];
        buf.read_run(bank, g.level as usize, g.kernel as usize, &mut g.pos, &mut g.step, g.mul, &mut nv[k0..end]);
        for k in k0..end {
            let a = (k as f32 + 0.5) / N as f32;
            v[k] = [v[k][0] + a * (nv[k][0] - v[k][0]), v[k][1] + a * (nv[k][1] - v[k][1])];
        }
        g.xf = 0;
    } else {
        buf.read_run(bank, g.level as usize, g.kernel as usize, &mut g.pos, &mut g.step, g.mul, &mut v[k0..end]);
    }
    if mono {
        for k in k0..end {
            let wm = w[k] * v[k][0];
            tb.m[k] += wm;
            tb.b[k] += wm * wm;
            tb.o[k] += g.amp * wm;
        }
    } else {
        for k in k0..end {
            let m = 0.5 * (v[k][0] + v[k][1]);
            let sd = 0.5 * (v[k][0] - v[k][1]);
            let wm = w[k] * m;
            tb.m[k] += wm;
            tb.b[k] += wm * wm;
            let wa = w[k] * g.amp;
            tb.l[k] += wa * (m * g.gl + sd);
            tb.r[k] += wa * (m * g.gr - sd);
        }
    }
    dead
}

#[derive(Clone, Copy, Debug, Default)]
struct Onset {
    at: u64,
    rise: f32,
}

#[derive(Clone, Debug)]
struct Layer {
    state: State,
    rng: Rng,
    // ---- scheduling
    t_next: f64,
    phase: f64,
    started: bool,
    strum_k: u64,
    /// Phrase: cycle start, slice count and length, the last slice index, its grain id
    c0: f64,
    slices: usize,
    slice_t: f64,
    slice_j: i64,
    /// the next slice's variates and the law of its fade-in (decided with the previous slice)
    pending: Option<[f64; 12]>,
    next_law: Law,
    phrase_l: f64,
    scan_off: f64,
    /// the grains of the current Rhythm step (identical ones are played once)
    step_keys: [(f64, f64, f32, bool); 8],
    n_step_keys: usize,
    // ---- hold
    engaged_at: u64,
    /// HOLD region start (absolute, even) and the field [f0, f1)
    h0: u64,
    f0: f64,
    f1: f64,
    copied: bool,
    end_by: u64,
    gain: f64,
    kill: f32,
    map: Vec<u8>,
    map_b0: u64,
    peak_lv: u8,
    onsets: [Onset; ONSETS],
    n_onsets: usize,
    // ---- level
    key: Option<StatKey>,
    stats: Stats,
    pinned: bool,
    a_t: f64,
    b_t: f64,
    a_s: f64,
    b_s: f64,
    ratio: f64,
    g: f64,
    /// no statistics yet
    fresh: bool,
    count: usize,
}

impl Layer {
    fn new(seed: u64) -> Self {
        Self {
            state: State::Free,
            rng: Rng::new(seed),
            t_next: 0.0,
            phase: 0.0,
            started: false,
            strum_k: 0,
            c0: 0.0,
            slices: 1,
            slice_t: 1.0,
            slice_j: -1,
            pending: None,
            next_law: Law::Power,
            phrase_l: 1.0,
            scan_off: 0.0,
            step_keys: [(0.0, 0.0, 0.0, false); 8],
            n_step_keys: 0,
            engaged_at: 0,
            h0: 0,
            f0: 0.0,
            f1: 0.0,
            copied: false,
            end_by: u64::MAX,
            gain: 1.0,
            kill: 1.0,
            map: Vec::new(),
            map_b0: 0,
            peak_lv: 0,
            onsets: [Onset::default(); ONSETS],
            n_onsets: 0,
            key: None,
            stats: Stats::default(),
            pinned: false,
            a_t: 0.0,
            b_t: 0.0,
            a_s: 0.0,
            b_s: 0.0,
            ratio: 1.0,
            g: 0.0,
            fresh: true,
            count: 0,
        }
    }

    fn restart_schedule(&mut self, now: u64) {
        self.t_next = now as f64;
        self.phase = 0.0;
        self.started = false;
        self.slice_j = -1;
        self.pending = None;
        self.next_law = Law::Power;
        self.scan_off = 0.0;
    }

    /// Spawning, or still sounding its last grains.
    fn sounding(&self) -> bool {
        matches!(self.state, State::Live | State::Held | State::Releasing | State::Ending) || self.count > 0
    }
}

/// Settings read once per block (sample-rate scaled).
#[derive(Clone, Copy, Debug, Default)]
struct Set {
    mode: usize,
    grains: f64,
    pos: f64,
    size: f64,
    pitch: f64,
    l: f64,
    spray: f64,
    scan: f64,
    irr: f64,
    chance: f64,
    shape: f32,
    skew: f32,
    reverse: f64,
    pitch_rand: f64,
    detune: f64,
    glide: f64,
    spread: f64,
    sync_beats: Option<f64>,
    len_beats: Option<f64>,
    skip: f64,
    decay_k: f64,
    trails: bool,
    bypass: bool,
}

pub struct Grains {
    v: [f32; NP],
    sr: f32,
    nch: usize,
    rec: MipBuffer,
    hold: MipBuffer,
    bank: KernelBank,
    win: Window,
    pool: GrainPool<Grain>,
    layers: [Layer; NL],
    tb: [TickBuf; NL],
    now: u64,
    /// HOLD: the layer whose grains read it, the layer waiting for it, the copy in progress
    hold_owner: Option<usize>,
    copy_wait: Option<usize>,
    copy_pos: usize,
    copy_len: usize,
    copy_layer: Option<usize>,
    freeze_on: bool,
    engage_pending: bool,
    // ---- onsets
    level: Level,
    pick: Pick,
    strum: Strum,
    /// the pick being graded: (frame, max rise, samples left, accepted)
    grading: Option<(u64, f32, u32, bool)>,
    map_rise: Vec<u8>,
    map_lv: Vec<u8>,
    onsets: [Onset; ONSETS],
    n_onsets: usize,
    onset_w: usize,
    // ---- interval sets
    members: [f32; 64],
    n_members: usize,
    // ---- output
    mix: OnePole,
    live: OnePole,
    fb: OnePole,
    kd: OnePole,
    byp: OnePole,
    wet_on: LinearRamp,
    scan_s: OnePole,
    cuts: [Cuts; 2],
    dc: [(f64, f64); 2],
    dc_k: f64,
    transport: TransportInfo,
    set: Set,
    seed: u64,
    /// every grain started: (layer, onset, absolute start, length, ratio) — tests only
    #[cfg(test)]
    pub(crate) trace: Vec<(u8, f64, f64, f64, f64)>,
}

impl Default for Grains {
    fn default() -> Self {
        Self::new()
    }
}

fn layer_seed(seed: u64, li: usize) -> u64 {
    0x6772_616E_u64.wrapping_mul(seed + 1).wrapping_add(li as u64)
}

impl Grains {
    pub fn new() -> Self {
        let mut v = [0.0; NP];
        for (i, p) in PARAMS.iter().enumerate() {
            v[i] = p.default;
        }
        let mut g = Self {
            v,
            sr: 48000.0,
            nch: 2,
            rec: MipBuffer::default(),
            hold: MipBuffer::default(),
            bank: KernelBank::new(),
            win: Window::new(),
            pool: GrainPool::new(POOL),
            layers: std::array::from_fn(|i| Layer::new(layer_seed(0, i))),
            tb: [TickBuf::default(); NL],
            now: 0,
            hold_owner: None,
            copy_wait: None,
            copy_pos: 0,
            copy_len: 0,
            copy_layer: None,
            freeze_on: false,
            engage_pending: false,
            level: Level::default(),
            pick: Pick::default(),
            strum: Strum::default(),
            grading: None,
            map_rise: Vec::new(),
            map_lv: Vec::new(),
            onsets: [Onset::default(); ONSETS],
            n_onsets: 0,
            onset_w: 0,
            members: [0.0; 64],
            n_members: 0,
            mix: OnePole::new(20.0, 0.5, 1e-6),
            live: OnePole::new(20.0, 1.0, 1e-6),
            fb: OnePole::new(20.0, 0.0, 1e-6),
            kd: OnePole::new(20.0, 0.0, 1e-6),
            byp: OnePole::new(10.0, 0.0, 1e-6),
            wet_on: LinearRamp::new(20.0, 1.0),
            scan_s: OnePole::new(20.0, 0.0, 1e-6),
            cuts: [Cuts::new(20.0, 1000.0, 1000.0, 20000.0), Cuts::new(20.0, 1000.0, 1000.0, 20000.0)],
            dc: [(0.0, 0.0); 2],
            dc_k: 1.0,
            transport: TransportInfo::default(),
            set: Set::default(),
            seed: 0,
            #[cfg(test)]
            trace: Vec::new(),
        };
        g.update_members();
        g
    }

    /// Memory held by the buffers (bytes).
    pub fn buffer_bytes(&self) -> usize {
        self.rec.bytes() + self.hold.bytes() + self.bank.bytes() + self.map_rise.len() * 2 + self.layers.iter().map(|l| l.map.len()).sum::<usize>()
    }

    /// 0 Live · 1 Held · 2 Releasing.
    pub fn state(&self) -> u32 {
        if self.layers[1..].iter().any(|l| l.state == State::Held) {
            1
        } else if self.layers[1..].iter().any(|l| l.state != State::Free) {
            2
        } else {
            0
        }
    }

    /// Voices sounding.
    pub fn active(&self) -> usize {
        self.pool.len()
    }

    /// Voices dropped (pool full) since reset.
    pub fn dropped(&self) -> u64 {
        self.pool.dropped()
    }

    /// The live layer's and the hold layers' current normalisation gains (tests).
    pub fn layer_gains(&self) -> [f64; NL] {
        std::array::from_fn(|i| self.layers[i].g)
    }

    /// The hold field of the newest held or releasing layer, in frames from reset (tests).
    pub fn hold_field(&self) -> Option<(f64, f64)> {
        self.layers[1..].iter().filter(|l| l.state != State::Free).max_by_key(|l| l.engaged_at).map(|l| (l.f0, l.f1))
    }

    /// Onsets marked so far (frames from reset, rise dB), newest last (tests).
    pub fn onsets(&self) -> Vec<(u64, f32)> {
        let n = self.n_onsets.min(ONSETS);
        (0..n).map(|k| self.onsets[(self.onset_w + ONSETS - n + k) % ONSETS]).map(|o| (o.at, o.rise)).collect()
    }

    fn update_members(&mut self) {
        let range = self.v[p::PITCH_RAND] as f64;
        let scale = self.v[p::SCALE].round() as usize;
        self.n_members = 0;
        if scale == 0 {
            return;
        }
        let set: &[i32] = match scale {
            1 => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            2 => &[0],
            3 => &[0, 7],
            4 => &[0, 4, 7],
            5 => &[0, 3, 7],
            6 => SCALES[0],
            7 => SCALES[1],
            8 => SCALES[8],
            _ => SCALES[9],
        };
        for st in -24i32..=24 {
            let pc = st.rem_euclid(12);
            if (st as f64).abs() <= range + 1e-9 && set.contains(&pc) && self.n_members < 64 {
                self.members[self.n_members] = st as f32;
                self.n_members += 1;
            }
        }
    }

    fn read_set(&mut self) {
        let v = &self.v;
        let sr = self.sr as f64;
        let ls = Sync::from_index(v[p::LENGTH_SYNC].round() as usize).beats();
        let l = match ls {
            Some(b) => self.transport.note_samples(b, self.sr).min(LMAX_S * sr),
            None => v[p::LENGTH] as f64 * 0.001 * sr,
        };
        self.set = Set {
            mode: v[p::MODE].round().clamp(0.0, 3.0) as usize,
            grains: v[p::GRAINS].round().clamp(1.0, 64.0) as f64,
            pos: v[p::POS] as f64,
            size: (v[p::SIZE] as f64 * 0.001 * sr).max(2.0),
            pitch: v[p::PITCH] as f64,
            l: l.max(1.0),
            spray: v[p::SPRAY] as f64 * 0.01,
            scan: 0.0,
            irr: 1.0 - v[p::REGULAR] as f64 * 0.01,
            chance: v[p::CHANCE] as f64 * 0.01,
            shape: v[p::SHAPE] * 0.01,
            skew: v[p::SKEW] * 0.01,
            reverse: v[p::REVERSE] as f64 * 0.01,
            pitch_rand: v[p::PITCH_RAND] as f64,
            detune: v[p::DETUNE] as f64 * 0.01,
            glide: v[p::GLIDE] as f64,
            spread: v[p::SPREAD] as f64 * 0.01,
            sync_beats: Sync::from_index(v[p::SYNC].round() as usize).beats(),
            len_beats: ls,
            skip: v[p::SKIP] as f64 * 0.01,
            decay_k: db_to_gain(-60.0 / (v[p::DECAY].max(0.05) as f64 * sr)),
            trails: v[p::TRAILS] > 0.5,
            bypass: v[p::BYPASS] > 0.5,
        };
    }

    fn set_targets(&mut self) {
        let v = self.v;
        self.mix.set_target(v[p::MIX] * 0.01);
        self.live.set_target(v[p::LIVE] * 0.01);
        self.fb.set_target(v[p::FEEDBACK] * 0.01);
        self.kd.set_target(if v[p::KILL_DRY] > 0.5 { 1.0 } else { 0.0 });
        self.byp.set_target(if v[p::BYPASS] > 0.5 { 1.0 } else { 0.0 });
        let trails_off_byp = v[p::BYPASS] > 0.5 && v[p::TRAILS] <= 0.5;
        self.wet_on.set_target(if trails_off_byp { 0.0 } else { 1.0 });
        self.scan_s.set_target(v[p::SCAN] * 0.01);
    }

    fn set_cuts(&mut self) {
        let v = self.v;
        for c in &mut self.cuts {
            c.low.set_freq(v[p::LOW_CUT], self.now);
            c.high.set_freq(v[p::HIGH_CUT], self.now);
        }
    }

    fn reseed(&mut self) {
        for (i, l) in self.layers.iter_mut().enumerate() {
            l.rng = Rng::new(layer_seed(self.seed, i));
        }
    }

    // ------------------------------------------------------------------------------------------
    // onsets

    /// The onset map and marks (gfx-dsp `onset`: level, `Pick` at its most sensitive, the strum
    /// window). The detector runs while Strum or `skip` needs it; a block it did not see stays
    /// unmarked, its level byte 0 (unknown).
    fn onset_sample(&mut self, xd: f32, detect: bool) {
        let t = self.now;
        let b = (t / BLOCK) as usize & (self.map_lv.len() - 1);
        if t.is_multiple_of(BLOCK) {
            self.map_rise[b] = 0;
            self.map_lv[b] = 0;
        }
        if !detect {
            if let Some((at, mx, _, acc)) = self.grading.take() {
                self.mark(at, mx, acc);
            }
            return;
        }
        self.strum.tick();
        let e_db = self.level.next(xd, -60.0);
        let floor = self.level.floor_db(30.0, -60.0);
        let lv = (e_db + 128.0).clamp(1.0, 255.0) as u8;
        if lv > self.map_lv[b] {
            self.map_lv[b] = lv;
        }
        let fired = self.pick.next(xd, e_db > floor);
        let rise = self.pick.rise_db();
        if let Some((at, mx, left, acc)) = self.grading {
            let mx = mx.max(rise);
            if left == 0 || fired {
                self.mark(at, mx, acc);
                self.grading = None;
            } else {
                self.grading = Some((at, mx, left - 1, acc));
            }
        }
        if fired {
            let lag = (Pick::LAG_MS * 0.001 * self.sr) as u64;
            let at = t.saturating_sub(lag);
            let acc = self.strum.decide(Source::Pick, STRUM_MS, false);
            self.grading = Some((at, rise, (0.005 * self.sr) as u32, acc));
        }
    }

    fn mark(&mut self, at: u64, rise: f32, accepted: bool) {
        let byte = (rise * RISE_SCALE).clamp(1.0, 255.0) as u8;
        let b = (at / BLOCK) as usize & (self.map_rise.len() - 1);
        self.map_rise[b] = self.map_rise[b].max(byte);
        if accepted {
            self.onsets[self.onset_w] = Onset { at, rise };
            self.onset_w = (self.onset_w + 1) % ONSETS;
            self.n_onsets += 1;
        } else if self.n_onsets > 0 {
            let k = (self.onset_w + ONSETS - 1) % ONSETS;
            self.onsets[k].rise = self.onsets[k].rise.max(rise);
        }
    }

    // ------------------------------------------------------------------------------------------
    // hold

    fn engage(&mut self) -> bool {
        let sr = self.sr as f64;
        let now = self.now;
        let written = self.rec.written();
        if (written as f64) < 0.1 * sr + 2.0 * MARGIN {
            return false;
        }
        // a third layer ends the oldest
        let sounding = |l: &Layer| matches!(l.state, State::Held | State::Releasing);
        if self.layers[1..].iter().filter(|l| sounding(l)).count() >= 2 {
            let oldest = (1..NL).filter(|&i| sounding(&self.layers[i])).min_by_key(|&i| self.layers[i].engaged_at).unwrap();
            self.layers[oldest].state = State::Ending;
        }
        let slot = match (1..NL).find(|&i| self.layers[i].state == State::Free) {
            Some(s) => s,
            None => {
                // every slot busy (one ending): drop the ending one's grains now
                let s = (1..NL).find(|&i| self.layers[i].state == State::Ending).unwrap_or(1);
                self.free_layer(s);
                s
            }
        };
        let s = self.set;
        let preroll = 0.03 * sr + 256.0;
        let lh = s.l.min(written as f64 - preroll.min(written as f64 * 0.5) - 2.0 * MARGIN).max(0.05 * sr);
        let e = now as f64;
        let h0 = ((e - lh - preroll).max(0.0) as u64) & !1;
        let mut f0 = (e - lh).max(h0 as f64 + MARGIN);
        let mut f1 = e;
        // the field: what rings after the last counted attack (README §2.6)
        let skip_on = s.skip > 0.0;
        let thr = ((12.0 - 9.0 * s.skip) as f32 * RISE_SCALE) as u8;
        let nb = self.map_rise.len();
        let attack_at = |from: f64, to: f64, rise: &[u8]| -> Option<u64> {
            // the newest block in [from, to) with a counted attack
            let (b0, b1) = ((from.max(0.0) as u64) / BLOCK, (to.max(0.0) as u64) / BLOCK);
            (b0..=b1).rev().find(|&b| rise[b as usize & (nb - 1)] >= thr.max(1) && b * BLOCK < to as u64).map(|b| b * BLOCK)
        };
        if skip_on {
            if let Some(a) = attack_at(f0, f1, &self.map_rise) {
                let after = a as f64 + 0.04 * sr;
                if f1 - after >= 0.1 * sr {
                    f0 = after;
                } else {
                    let end = a as f64 - 0.005 * sr;
                    let start = match attack_at(f0, end, &self.map_rise) {
                        Some(a2) => (a2 as f64 + 0.04 * sr).max(f0),
                        None => f0,
                    };
                    if end - start >= 0.02 * sr {
                        f0 = start;
                        f1 = end;
                    }
                }
            }
        }
        let li = slot;
        let rng = self.layers[li].rng.clone();
        let map = std::mem::take(&mut self.layers[li].map);
        let mut l = Layer::new(0);
        l.rng = rng;
        l.map = map;
        l.state = State::Held;
        l.engaged_at = now;
        l.h0 = h0;
        l.f0 = f0;
        l.f1 = f1;
        l.copied = false;
        l.restart_schedule(now);
        // the map and the onsets of the region
        let b0 = h0 / BLOCK;
        let b1 = now / BLOCK + 1;
        l.map_b0 = b0;
        let nblk = ((b1 - b0) as usize).min(l.map.len() / 2);
        let mut peak = 0u8;
        for k in 0..nblk {
            let src = (b0 as usize + k) & (nb - 1);
            l.map[2 * k] = self.map_rise[src];
            l.map[2 * k + 1] = self.map_lv[src];
            let at = (b0 + k as u64) * BLOCK;
            if at as f64 >= f0 && (at as f64) < f1 {
                peak = peak.max(self.map_lv[src]);
            }
        }
        l.peak_lv = peak;
        l.n_onsets = 0;
        for o in self.onsets_vec() {
            if o.at as f64 >= f0 && (o.at as f64) < f1 && l.n_onsets < ONSETS {
                l.onsets[l.n_onsets] = o;
                l.n_onsets += 1;
            }
        }
        self.layers[li] = l;
        // copy-on-freeze: now, or once the layer reading HOLD has gone
        let busy = self.hold_owner.is_some_and(|o| o != li && self.layers[o].state != State::Free);
        if busy {
            let o = self.hold_owner.unwrap();
            if let Some(w) = self.copy_wait {
                // a layer was already waiting: it gives up its turn and stays on RECORD (it is the
                // oldest unless ended above; its own end bound applies)
                let _ = w;
            }
            self.copy_wait = Some(li);
            let n0 = self.rec.frames() as f64;
            let lh_f = now as f64 - h0 as f64;
            let deadline = h0 as f64 + n0 - lh_f / COPY_RATE as f64 - (MAX_GRAIN_S + 0.5) * sr - 0.05 * sr;
            let d = deadline.max(now as f64) as u64;
            self.layers[o].end_by = self.layers[o].end_by.min(d);
            // and the waiting layer itself never outlives its RECORD region
            self.layers[li].end_by = (h0 as f64 + n0 - (MAX_GRAIN_S + 0.5) * sr) as u64;
        } else {
            self.start_copy(li);
        }
        self.tick_level(li);
        true
    }

    fn onsets_vec(&self) -> impl Iterator<Item = Onset> + '_ {
        let n = self.n_onsets.min(ONSETS);
        (0..n).map(move |k| self.onsets[(self.onset_w + ONSETS - n + k) % ONSETS])
    }

    fn start_copy(&mut self, li: usize) {
        self.hold_owner = Some(li);
        self.copy_layer = Some(li);
        self.copy_wait = None;
        self.copy_pos = 0;
        let n = (self.now - self.layers[li].h0) as usize;
        self.copy_len = n.div_ceil(COPY_RATE) * COPY_RATE;
        self.layers[li].copied = false;
    }

    fn copy_step(&mut self) {
        let Some(li) = self.copy_layer else { return };
        if self.layers[li].state == State::Free {
            self.copy_layer = None;
            self.hold_owner = None;
            return;
        }
        let h0 = self.layers[li].h0;
        let k = self.copy_pos;
        let n = COPY_RATE.min(self.copy_len - k);
        self.hold.copy_l0_from(&self.rec, h0 + k as u64, k, n);
        self.hold.copy_l1_from(&self.rec, h0 / 2 + k as u64 / 2, k / 2, n / 2);
        self.copy_pos += n;
        if self.copy_pos >= self.copy_len {
            self.copy_layer = None;
            self.layers[li].copied = true;
            // its grains still on RECORD end within the longest grain: well inside the region's life
            self.layers[li].end_by = u64::MAX;
        }
    }

    fn free_layer(&mut self, li: usize) {
        self.pool.retain(|g| g.layer as usize != li);
        let l = &mut self.layers[li];
        l.state = State::Free;
        l.count = 0;
        if self.hold_owner == Some(li) {
            self.hold_owner = None;
            if self.copy_layer == Some(li) {
                self.copy_layer = None;
            }
        }
        if self.copy_wait == Some(li) {
            self.copy_wait = None;
        }
    }

    // ------------------------------------------------------------------------------------------
    // spawning

    #[inline]
    fn draw(rng: &mut Rng) -> [f64; 12] {
        std::array::from_fn(|_| rng.next_f64())
    }

    /// The pitch offset (st) for a variate.
    #[inline]
    fn interval(&self, u: f64) -> f64 {
        if self.set.pitch_rand <= 0.0 {
            return 0.0;
        }
        if self.n_members == 0 {
            self.set.pitch_rand * (2.0 * u - 1.0)
        } else {
            self.members[((u * self.n_members as f64) as usize).min(self.n_members - 1)] as f64
        }
    }

    /// Builds and adds one grain from its variates; `a_want` is the wanted start (absolute frame),
    /// reflected into the readable range. False when it could not start (no room, pool full).
    fn spawn(&mut self, li: usize, t_on: f64, u: &[f64; 12], len: f64, a_want: f64, slice: Option<(f64, Law, Law)>) -> bool {
        let s = self.set;
        let sr = self.sr as f64;
        let hold_layer = li > 0;
        let s0 = (s.pitch + self.interval(u[2]) + 100.0 * s.detune * (2.0 * u[3] - 1.0) * 0.01).clamp(-24.0, 24.0);
        let s1 = (s0 + s.glide).clamp(-24.0, 24.0);
        let r0 = (s0 / 12.0).exp2();
        let mut len = len.clamp(2.0, if slice.is_some() { f64::MAX } else { MAX_GRAIN_S * sr });
        let reverse = u[4] < s.reverse;
        let theta = s.spread * (2.0 * u[5] - 1.0);
        let unity = s0 == 0.0 && s1 == 0.0;
        let mut mul = if s1 == s0 { 1.0 } else { ((s1 - s0) / 12.0 / len).exp2() };
        let mut sp = span(r0, mul, len);
        let rmax = r0.max(r0 * mul.powf(len));
        let lay = &self.layers[li];
        // the readable range of the start
        let (lo, hi) = if hold_layer {
            let half = 0.5 * (lay.f1 - lay.f0);
            if slice.is_none() && sp > half {
                len *= half / sp;
                mul = if s1 == s0 { 1.0 } else { ((s1 - s0) / 12.0 / len).exp2() };
                sp = span(r0, mul, len);
            }
            let (a, b) = (lay.f0 - if slice.is_some() { 0.03 * sr } else { 0.0 }, lay.f1);
            let (a, b) = (a.max(lay.h0 as f64 + MARGIN), b - MARGIN);
            if reverse {
                (a + sp, b)
            } else {
                (a, b - sp)
            }
        } else {
            let dmin = (if rmax >= 1.9 { DMIN1_MS } else { DMIN0_MS }) * 0.001 * sr;
            let oldest = (self.now as f64 + MAX_GRAIN_S * sr - self.rec.frames() as f64).max(0.0) + MARGIN;
            if reverse {
                (oldest + sp, t_on - dmin)
            } else {
                (oldest, (t_on - dmin).min(t_on + len - sp - dmin))
            }
        };
        let Some(mut a0) = reflect(a_want, lo, hi) else { return false };
        let now = self.now as f64;
        let step0 = if reverse { -r0 } else { r0 };
        // position at the first rendered sample (now ≥ t_on)
        let el = now - t_on;
        let mut pos_abs;
        if unity {
            if reverse {
                let k = (a0 + t_on).round();
                a0 = k - t_on;
                pos_abs = k - now;
            } else {
                let d = (t_on - a0).round();
                a0 = t_on - d;
                pos_abs = now - d;
            }
            if a0 < lo - 1.0 || a0 > hi + 1.0 {
                return false;
            }
        } else {
            pos_abs = a0 + step0 * el;
        }
        // which buffer
        let use_hold = hold_layer && self.layers[li].copied;
        if use_hold {
            pos_abs -= self.layers[li].h0 as f64;
        }
        let r_first = if mul > 1.0 { r0 * mul.powf(el + TICK as f64) } else { r0 * mul.powf(el) };
        let (level, kernel) = KernelBank::choose(r_first.min(4.0));
        let guard = (0.002 * sr).min(len * 0.25) / len;
        let win = WindowShape::new(s.shape, s.skew, guard);
        let (gl, gr) = if theta == 0.0 {
            (1.0, 1.0)
        } else {
            let a = std::f64::consts::FRAC_PI_4;
            ((std::f64::consts::SQRT_2 * (a * (1.0 + theta)).cos()) as f32, (std::f64::consts::SQRT_2 * (a * (1.0 - theta)).cos()) as f32)
        };
        let (slice_on, x, law_in, law_out) = match slice {
            Some((x, a, b)) => (true, x, a, b),
            None => (false, 0.0, Law::Power, Law::Power),
        };
        let g = Grain {
            layer: li as u8,
            hold: use_hold,
            t_on,
            inv_len: 1.0 / len,
            len,
            pos: pos_abs,
            step: step0 * if mul != 1.0 { mul.powf(el) } else { 1.0 },
            mul,
            unity,
            level: level as u8,
            kernel: kernel as u8,
            plevel: level as u8,
            pkernel: kernel as u8,
            xf: 0,
            win,
            osc: WindowOsc::new(&win, el / len, 1.0 / len),
            slice: slice_on,
            x,
            law_in,
            law_out,
            gl,
            gr,
            amp: self.layers[li].g as f32,
            dead: false,
        };
        if self.pool.push(g) {
            self.layers[li].count += 1;
            #[cfg(test)]
            self.trace.push((li as u8, t_on, a0, len, if reverse { -r0 } else { r0 }));
            // the rest of the current tick, at once
            let t0 = (self.now - self.now % TICK) as f64;
            let k0 = (self.now % TICK) as usize;
            let mono = self.nch == 1;
            let last = self.pool.len() - 1;
            let gm = &mut self.pool.as_mut_slice()[last];
            if render_grain(gm, k0, TICK as usize, t0, &self.rec, &self.hold, &self.bank, &self.win, mono, &mut self.tb[li]) {
                gm.dead = true;
            }
            true
        } else {
            false
        }
    }

    /// A layer's grain length: `size`, or in a hold at most what fits half its field at the base
    /// pitch (README §2.6: "grains fit the field"); the onset rate follows, so the overlap is kept.
    fn size_of(&self, li: usize) -> f64 {
        let s = self.set;
        if li == 0 || s.mode == PHRASE {
            return s.size;
        }
        let l = &self.layers[li];
        let fit = 0.5 * (l.f1 - l.f0) / (s.pitch / 12.0).exp2();
        s.size.min(fit).max(2.0)
    }

    /// The cap on a layer's density while two layers sound.
    fn cap(&self) -> f64 {
        // the live layer counts while it spawns or still has grains; an ending layer (≤ 50 ms) too
        let sounding = self.layers.iter().filter(|l| l.sounding() && (l.state != State::Free || l.count > 0)).count();
        match sounding {
            0 | 1 => 64.0,
            2 => CAP_TWO,
            _ => CAP_THREE,
        }
    }

    /// The layer's L and its position reference: (L, end) — live: (L, now), hold: (field, f1).
    fn frame_of(&self, li: usize) -> (f64, f64) {
        if li == 0 {
            (self.set.l, self.now as f64)
        } else {
            let l = &self.layers[li];
            (l.f1 - l.f0, l.f1)
        }
    }

    /// A Cloud / Rhythm grain's wanted start (absolute) from its variates.
    fn cloud_start(&self, li: usize, u0: f64, quant: Option<f64>) -> f64 {
        let s = self.set;
        let (l, end) = self.frame_of(li);
        let off = self.layers[li].scan_off.rem_euclid(l);
        let mut c = s.pos * l - off;
        if c < 0.0 {
            c += l;
        }
        let mut d = c + s.spray * l * (2.0 * u0 - 1.0);
        if let Some(step) = quant {
            d = (d / step).round().max(1.0) * step;
        }
        end - d
    }

    /// Hold rejection (`skip`): a span touching an attack (5 ms before its mark to 40 ms after) or a
    /// block 30 dB under the hold's peak.
    fn rejected(&self, li: usize, a0: f64, sp: f64, reverse: bool) -> bool {
        let l = &self.layers[li];
        let sr = self.sr as f64;
        let (a, b) = if reverse { (a0 - sp, a0) } else { (a0, a0 + sp) };
        let thr = ((12.0 - 9.0 * self.set.skip) as f32 * RISE_SCALE).max(1.0) as u8;
        let floor = l.peak_lv.saturating_sub(30);
        let nblk = l.map.len() / 2;
        if nblk == 0 {
            return false;
        }
        let blk = |f: f64| -> usize { ((f.max(0.0) as u64 / BLOCK).saturating_sub(l.map_b0) as usize).min(nblk - 1) };
        // attacks: marks within [a − 40 ms, b + 5 ms]
        for k in blk(a - 0.04 * sr)..=blk(b + 0.005 * sr) {
            if l.map[2 * k] >= thr {
                return true;
            }
        }
        // near-silence (a level byte of 0 is a block the detector did not see)
        for k in blk(a)..=blk(b) {
            let lv = l.map[2 * k + 1];
            if lv != 0 && lv < floor {
                return true;
            }
        }
        false
    }

    /// One Cloud / Rhythm / Strum grain from fresh variates.
    fn grain_from(&mut self, li: usize, t_on: f64, u: &[f64; 12], want: impl Fn(&Self, f64) -> f64) -> bool {
        let s = self.set;
        if u[6] >= s.chance {
            return false;
        }
        let len = self.size_of(li) * (1.0 + 0.25 * (2.0 * u[1] - 1.0) * s.spray);
        let mut a = want(self, u[0]);
        if li > 0 && s.skip > 0.0 && (s.mode == CLOUD || s.mode == RHYTHM) {
            // the rule: re-draw the position with the spare variates, up to four times
            let r0 = ((s.pitch + self.interval(u[2])) / 12.0).exp2();
            let sp = (r0 * len).min(0.5 * (self.layers[li].f1 - self.layers[li].f0));
            let reverse = u[4] < s.reverse;
            let mut k = 0;
            let (lo, hi) = (self.layers[li].f0 + MARGIN, self.layers[li].f1 - MARGIN);
            loop {
                let a_r = reflect(a, if reverse { lo + sp } else { lo }, if reverse { hi } else { hi - sp }).unwrap_or(a);
                if !self.rejected(li, a_r, sp, reverse) {
                    break;
                }
                if k == 4 {
                    return false;
                }
                a = want(self, u[7 + k]);
                k += 1;
            }
        }
        self.spawn(li, t_on, u, len, a, None)
    }

    /// Spawns what the layer's scheduler asks for at this sample.
    fn schedule(&mut self, li: usize, i_block: usize) {
        let s = self.set;
        let sr = self.sr as f64;
        let now = self.now as f64;
        let cap = self.cap();
        // scan: the position centre moves at `scan` × real time
        let scan = self.scan_s.value() as f64;
        if scan != 0.0 {
            let (l, _) = self.frame_of(li);
            let lay = &mut self.layers[li];
            lay.scan_off = (lay.scan_off + scan).rem_euclid(l.max(1.0));
        }
        match s.mode {
            CLOUD => {
                let grains = s.grains.min(cap);
                let mean = self.size_of(li) / grains;
                let mut guard = 0;
                while self.layers[li].t_next <= now && guard < 16 {
                    guard += 1;
                    let t_on = self.layers[li].t_next;
                    let u = Self::draw(&mut self.layers[li].rng);
                    let iv = mean * ((1.0 - s.irr) + s.irr * -(1.0 - u[11]).ln());
                    self.layers[li].t_next = t_on + iv.max(1e-3);
                    self.grain_from(li, t_on, &u, |g, u0| g.cloud_start(li, u0, None));
                }
                if guard == 16 {
                    self.layers[li].t_next = self.layers[li].t_next.max(now);
                }
            }
            RHYTHM | STRUM => {
                let step = match s.sync_beats {
                    Some(b) => self.transport.note_samples(b, self.sr),
                    None => {
                        if s.mode == RHYTHM {
                            s.size
                        } else {
                            0.5 * s.size
                        }
                    }
                };
                let Some(t_on) = self.grid(li, i_block, step, s.sync_beats) else { return };
                if s.mode == RHYTHM {
                    self.layers[li].n_step_keys = 0;
                    let n = s.grains.min(cap) as usize;
                    for _ in 0..n {
                        let u = Self::draw(&mut self.layers[li].rng);
                        // grains that would be identical (position, ratio, pan, direction) play once
                        let a = self.cloud_start(li, u[0], Some(step));
                        let key = (a, s.pitch + self.interval(u[2]) + 100.0 * s.detune * (2.0 * u[3] - 1.0) * 0.01, (s.spread * (2.0 * u[5] - 1.0)) as f32, u[4] < s.reverse);
                        let lay = &self.layers[li];
                        if lay.step_keys[..lay.n_step_keys].contains(&key) {
                            continue;
                        }
                        if lay.n_step_keys < 8 {
                            let lay = &mut self.layers[li];
                            lay.step_keys[lay.n_step_keys] = key;
                            lay.n_step_keys += 1;
                        }
                        self.grain_from(li, t_on, &u, |g, u0| g.cloud_start(li, u0, Some(step)));
                    }
                } else {
                    let u = Self::draw(&mut self.layers[li].rng);
                    let (l, end) = self.frame_of(li);
                    let from = end - s.pos * l;
                    let list: [Onset; ONSETS];
                    let n;
                    if li == 0 {
                        let mut arr = [Onset::default(); ONSETS];
                        let mut k = 0;
                        for o in self.onsets_vec() {
                            arr[k] = o;
                            k += 1;
                        }
                        list = arr;
                        n = k;
                    } else {
                        list = self.layers[li].onsets;
                        n = self.layers[li].n_onsets;
                    }
                    // newest first, those in range and strong enough
                    let mut cand = [0u64; ONSETS];
                    let mut m = 0;
                    for k in (0..n).rev() {
                        let o = list[k];
                        if (o.at as f64) >= from && (o.at as f64) < end && o.rise >= STRUM_RISE_DB {
                            cand[m] = o.at;
                            m += 1;
                        }
                    }
                    let m = m.min(s.grains as usize);
                    if m == 0 {
                        return;
                    }
                    let k = (self.layers[li].strum_k % m as u64) as usize;
                    self.layers[li].strum_k += 1;
                    let at = cand[k] as f64 - 0.005 * sr + s.spray * 0.05 * sr * (2.0 * u[0] - 1.0);
                    self.grain_from(li, t_on, &u, |_, _| at);
                }
            }
            _ => self.phrase(li, i_block),
        }
    }

    /// A Rhythm / Strum step at this sample: its exact onset time.
    fn grid(&mut self, li: usize, i_block: usize, step: f64, beats: Option<f64>) -> Option<f64> {
        let now = self.now as f64;
        let synced = beats.and_then(|b| self.transport.phase_at(i_block, b, 0.0, self.sr));
        let lay = &mut self.layers[li];
        let (ph, wrapped) = match synced {
            Some(ph) => {
                let w = !lay.started || ph < lay.phase;
                (ph, w)
            }
            None => {
                let mut ph = lay.phase + 1.0 / step.max(1.0);
                if !lay.started {
                    ph = 0.0;
                }
                let w = !lay.started || ph >= 1.0;
                (ph.fract(), w)
            }
        };
        lay.phase = ph;
        let first = !lay.started;
        lay.started = true;
        if wrapped {
            Some(if first { now } else { now - (ph * step).clamp(0.0, 1.0 - 1e-9) })
        } else {
            None
        }
    }

    /// Phrase: the slice clock and its slice grains (README §2.7).
    fn phrase(&mut self, li: usize, i_block: usize) {
        let s = self.set;
        let sr = self.sr as f64;
        let now = self.now as f64;
        let synced = if li == 0 { s.len_beats.and_then(|b| self.transport.phase_at(i_block, b, 0.0, self.sr)) } else { None };
        let (l_now, _) = self.frame_of(li);
        let lay = &mut self.layers[li];
        let first = !lay.started;
        let (ph, wrapped) = if first {
            lay.phrase_l = l_now;
            lay.slices = s.grains as usize;
            lay.slice_t = l_now / lay.slices as f64;
            (synced.unwrap_or(0.0), true)
        } else {
            match synced {
                Some(ph) => (ph, ph < lay.phase),
                None => {
                    let ph = lay.phase + 1.0 / lay.phrase_l;
                    (ph.fract(), ph >= 1.0)
                }
            }
        };
        lay.started = true;
        lay.phase = ph;
        if wrapped {
            lay.phrase_l = l_now;
            lay.slices = s.grains as usize;
            lay.slice_t = l_now / lay.slices as f64;
        }
        let j = ((ph * lay.slices as f64).floor() as i64).min(lay.slices as i64 - 1);
        if !wrapped && j == lay.slice_j {
            return;
        }
        let since = (ph * lay.slices as f64 - j as f64) * lay.slice_t;
        let t_on = if first || since >= 1.0 { now } else { now - since.max(0.0) };
        if wrapped {
            lay.c0 = t_on - j as f64 * lay.slice_t;
        }
        lay.slice_j = j;
        let (t, n, c0, lp) = (lay.slice_t, lay.slices, lay.c0, lay.phrase_l);
        let x = (0.5 * t).min(0.03 * sr);
        // this slice's variates (drawn one slice ahead) and the next one's: the seam between them is
        // decided now, before either is rendered
        let u = match self.layers[li].pending.take() {
            Some(u) => u,
            None => Self::draw(&mut self.layers[li].rng),
        };
        let un = Self::draw(&mut self.layers[li].rng);
        self.layers[li].pending = Some(un);
        let sigma_of = |u: &[f64; 12], j: usize| if u[11] < s.spray { ((u[0] * n as f64) as usize).min(n - 1) } else { j % n };
        let unity_of = |g: &Self, u: &[f64; 12]| s.pitch + g.interval(u[2]) + s.detune * (2.0 * u[3] - 1.0) == 0.0 && s.glide == 0.0 && u[4] >= s.reverse;
        let j = j as usize;
        let sigma = sigma_of(&u, j);
        // the next slice: (j + 1) mod n live (a new cycle re-captures the phrase: a delay of L), j + 1
        // in a hold (its wrap returns to the phrase's start: a jump)
        let jn = if li == 0 { (j + 1) % n } else { j + 1 };
        let sigma_n = sigma_of(&un, (j + 1) % n);
        let contiguous = unity_of(self, &u) && unity_of(self, &un) && un[6] < s.chance && j as i64 - sigma as i64 == jn as i64 - sigma_n as i64;
        let law_in = self.layers[li].next_law;
        let law_out = if contiguous { Law::Comp } else { Law::Power };
        self.layers[li].next_law = law_out;
        let start = if li == 0 { c0 - lp + sigma as f64 * t } else { self.layers[li].f0 + sigma as f64 * t };
        let reverse = u[4] < s.reverse;
        let a = if reverse { start + t } else { start - x };
        if u[6] >= s.chance {
            // a rest: the previous slice has already faded out on its own
            self.layers[li].next_law = Law::Power;
            return;
        }
        self.spawn(li, t_on, &u, t + x, a, Some((x, law_in, law_out)));
    }

    // ------------------------------------------------------------------------------------------
    // level

    fn tick_level(&mut self, li: usize) {
        let s = self.set;
        let sr = self.sr as f64;
        let cap = self.cap();
        let size = self.size_of(li);
        let lay = &mut self.layers[li];
        if lay.state == State::Free {
            return;
        }
        let lay = &self.layers[li];
        let step = match s.sync_beats {
            Some(b) => self.transport.note_samples(b, self.sr),
            None => {
                if s.mode == RHYTHM {
                    s.size
                } else {
                    0.5 * s.size
                }
            }
        };
        let (n_grid, m) = match s.mode {
            CLOUD => (s.grains.min(cap), 1.0),
            RHYTHM => {
                let identical = s.spray == 0.0 && s.pitch_rand == 0.0 && s.detune == 0.0 && s.spread == 0.0 && (s.reverse == 0.0 || s.reverse >= 1.0) && s.scan == 0.0;
                (size / step, if identical { 1.0 } else { s.grains.min(cap) })
            }
            STRUM => (size / step, 1.0),
            _ => (1.0, 1.0),
        };
        let key = StatKey { shape: s.shape, skew: s.skew, size, n_grid, m, chance: s.chance as f32, irr: if s.mode == CLOUD { s.irr as f32 } else { 0.0 }, mode: s.mode };
        if lay.key != Some(key) {
            let guard = (0.002 * sr).min(size * 0.25) / size;
            let ws = WindowShape::new(s.shape, s.skew, guard);
            let (ew, ew2) = self.win.moments(&ws);
            let c = s.chance;
            let a2 = key.irr as f64 * key.irr as f64;
            let vper = self.win.periodic_var(&ws, n_grid);
            let n = c * m * n_grid;
            let v = c * c * m * m * ((1.0 - a2) * vper + a2 * n_grid * ew2) + c * (1.0 - c) * m * n_grid * ew2;
            let base = n * ew2;
            let hi = if base > 0.0 { ((n * n * ew * ew + v) / base).max(1.0) } else { 1.0 };
            let first = lay.key.is_none();
            let lay = &mut self.layers[li];
            lay.key = Some(key);
            lay.stats = Stats { n, ew2, hi };
            if first {
                // after a reset: the draft's model, c = exp(−σ / 2 ms)
                let sigma_ms = s.spray * s.l / sr * 1000.0 / 3f64.sqrt();
                let c = (-sigma_ms / 2.0).exp();
                lay.ratio = 1.0 + c * (hi - 1.0);
            }
        }
        let lay = &mut self.layers[li];
        let k = 1.0 - (-(TICK as f64) / (0.4 * sr)).exp();
        lay.a_s += k * (lay.a_t - lay.a_s);
        lay.b_s += k * (lay.b_t - lay.b_s);
        lay.a_t = 0.0;
        lay.b_t = 0.0;
        let hi = lay.stats.hi;
        // pinned when the layer's grains provably read the same samples (README §2.5)
        lay.pinned = s.mode != STRUM && s.pitch == 0.0 && s.pitch_rand == 0.0 && s.detune == 0.0 && s.glide == 0.0 && s.spray == 0.0 && self.v[p::SCAN] == 0.0;
        if lay.pinned {
            lay.ratio = hi;
        } else if lay.b_s > 1e-9 * TICK as f64 {
            lay.ratio = (lay.a_s / lay.b_s).clamp(1.0, hi);
        } else {
            lay.ratio = lay.ratio.clamp(1.0, hi);
        }
        let target = if s.mode == PHRASE {
            1.0
        } else {
            let d = lay.stats.n * lay.stats.ew2 * lay.ratio;
            if d > 0.0 {
                (1.0 / d.sqrt()).min(G_MAX)
            } else {
                0.0
            }
        };
        lay.fresh = false;
        lay.g = target;
    }

    // ------------------------------------------------------------------------------------------
    // per tick

    fn tick(&mut self) {
        // kernels follow ρ(t) (per tick, with a one-tick crossfade at a change)
        for g in self.pool.as_mut_slice() {
            if g.unity {
                continue;
            }
            let r_end = g.step.abs() * if g.mul != 1.0 { g.mul.powf(TICK as f64) } else { 1.0 };
            let r = if g.mul > 1.0 { r_end } else { g.step.abs() };
            let (lv, k) = KernelBank::choose(r.min(4.0));
            if lv as u8 != g.level || k as u8 != g.kernel {
                g.plevel = g.level;
                g.pkernel = g.kernel;
                g.level = lv as u8;
                g.kernel = k as u8;
                g.xf = TICK as u8;
            }
        }
        for li in 0..NL {
            self.tick_level(li);
        }
    }

    /// Clears the tick's sums, drops the grains that ended, renders every grain over the tick.
    fn render_tick(&mut self) {
        let layers = &mut self.layers;
        self.pool.retain(|g| {
            if g.dead {
                layers[g.layer as usize].count -= 1;
            }
            !g.dead
        });
        self.tb = [TickBuf::default(); NL];
        let t0 = self.now as f64;
        let mono = self.nch == 1;
        let (rec, hold, bank, win, tb) = (&self.rec, &self.hold, &self.bank, &self.win, &mut self.tb);
        for g in self.pool.as_mut_slice() {
            if render_grain(g, 0, TICK as usize, t0, rec, hold, bank, win, mono, &mut tb[g.layer as usize]) {
                g.dead = true;
            }
        }
    }

    /// Freeze edges, layer lifecycles (per sample).
    fn lifecycle(&mut self) {
        let s = self.set;
        let want = self.v[p::FREEZE] > 0.5 && !(s.bypass && s.trails);
        if want && !self.freeze_on {
            self.freeze_on = true;
            self.engage_pending = true;
        } else if !want && self.freeze_on {
            self.freeze_on = false;
            self.engage_pending = false;
            for i in 1..NL {
                if self.layers[i].state == State::Held {
                    self.layers[i].state = State::Releasing;
                }
            }
        }
        if self.engage_pending && self.engage() {
            self.engage_pending = false;
        }
        let now = self.now;
        let kill_step = 1.0 / (0.05 * self.sr);
        for i in 1..NL {
            let l = &mut self.layers[i];
            match l.state {
                State::Releasing => {
                    l.gain *= s.decay_k;
                    if l.gain < 1e-4 {
                        l.state = State::Ending;
                    }
                }
                State::Ending => {
                    l.kill -= kill_step;
                    if l.kill <= 0.0 {
                        l.kill = 0.0;
                        self.free_layer(i);
                        continue;
                    }
                }
                _ => {}
            }
            let l = &mut self.layers[i];
            if l.state != State::Free && l.state != State::Ending && now >= l.end_by {
                l.state = State::Ending;
            }
        }
        // a waiting copy starts once HOLD is free
        if let Some(w) = self.copy_wait {
            let free = self.hold_owner.is_none_or(|o| self.layers[o].state == State::Free);
            if free {
                self.start_copy(w);
            }
        }
        // the live layer
        let live_ok = !self.freeze_on && !(s.bypass && s.trails) && !(self.live.target() == 0.0 && self.live.value() == 0.0);
        let l = &mut self.layers[0];
        if live_ok && l.state != State::Live {
            l.state = State::Live;
            l.restart_schedule(now);
        } else if !live_ok && l.state == State::Live {
            l.state = State::Free;
        }
    }
}

impl Effect for Grains {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate;
        self.nch = channels.clamp(1, 2);
        let sr = sample_rate as f64;
        self.dc_k = 1.0 - 2.0 * std::f64::consts::PI * 10.0 / sr;
        self.rec = MipBuffer::new(self.nch, (RECORD_S * sr) as usize);
        self.hold = MipBuffer::new(self.nch, ((LMAX_S + 0.03) * sr) as usize + 1024);
        let nb = self.rec.frames() / BLOCK as usize;
        self.map_rise = vec![0; nb];
        self.map_lv = vec![0; nb];
        let hb = (self.hold.frames() / BLOCK as usize + 4) * 2;
        for l in &mut self.layers {
            l.map = vec![0; hb];
        }
        for s in [&mut self.mix, &mut self.live, &mut self.fb, &mut self.kd, &mut self.byp, &mut self.scan_s] {
            s.prepare(sample_rate);
        }
        self.wet_on.prepare(sample_rate);
        for c in &mut self.cuts {
            c.prepare(sample_rate);
        }
        self.level.prepare(sample_rate);
        self.pick.prepare(sample_rate);
        self.pick.set(PickSettings { rise_db: Pick::rise_for(1.0), ..PickSettings::default() });
        self.strum.prepare(sample_rate);
        self.reset();
    }

    fn reset(&mut self) {
        self.now = 0;
        self.read_set();
        self.set_targets();
        for s in [&mut self.mix, &mut self.live, &mut self.fb, &mut self.kd, &mut self.byp, &mut self.scan_s] {
            s.settle();
        }
        self.wet_on.settle();
        self.set_cuts();
        for c in &mut self.cuts {
            c.reset();
        }
        self.dc = [(0.0, 0.0); 2];
        self.rec.reset();
        self.hold.reset();
        self.pool.clear();
        self.seed = self.v[p::SEED].round() as u64;
        for (i, l) in self.layers.iter_mut().enumerate() {
            let map = std::mem::take(&mut l.map);
            *l = Layer::new(layer_seed(self.seed, i));
            l.map = map;
        }
        self.tb = [TickBuf::default(); NL];
        self.hold_owner = None;
        self.copy_wait = None;
        self.copy_layer = None;
        self.freeze_on = false;
        self.engage_pending = false;
        self.level.reset();
        self.pick.reset();
        self.strum.reset();
        self.grading = None;
        self.n_onsets = 0;
        self.onset_w = 0;
        self.update_members();
        #[cfg(test)]
        self.trace.clear();
    }

    fn set_param(&mut self, id: &str, value: f32) {
        if let Some(i) = PARAMS.iter().position(|p| p.id == id) {
            let pi = &PARAMS[i];
            let value = if matches!(pi.kind, ParamKind::Int | ParamKind::Enum(_)) { value.round() } else { value };
            self.v[i] = value.clamp(pi.min, pi.max);
            self.set_targets();
            match i {
                p::PITCH_RAND | p::SCALE => self.update_members(),
                p::LOW_CUT | p::HIGH_CUT => self.set_cuts(),
                p::SEED => {
                    let s = self.v[p::SEED].round() as u64;
                    if s != self.seed {
                        self.seed = s;
                        self.reseed();
                    }
                }
                p::MODE => {
                    let now = self.now;
                    for l in &mut self.layers {
                        l.restart_schedule(now);
                    }
                }
                _ => {}
            }
        }
    }

    fn set_transport(&mut self, transport: TransportInfo) {
        self.transport = transport;
    }

    fn meter(&self, id: &str) -> Option<f32> {
        match id {
            "active" => Some(self.active() as f32),
            "state" => Some(self.state() as f32),
            _ => None,
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(self.nch);
        if nch == 0 {
            return;
        }
        let n = buffers[0].len();
        self.read_set();
        let mono = self.nch == 1;
        for i in 0..n {
            let mut x = [0.0f32; 2];
            for c in 0..nch {
                x[c] = buffers[c][i];
            }
            let xd = if mono { x[0] } else { 0.5 * (x[0] + x[1]) };
            // ---- smoothed controls
            let mix = self.mix.next();
            let live = self.live.next() as f64;
            let fb = self.fb.next();
            let kd = self.kd.next();
            let byp = self.byp.next();
            let wet_on = self.wet_on.next();
            self.scan_s.next();
            self.set.scan = self.scan_s.value() as f64;

            let detect = self.set.mode == STRUM || self.set.skip > 0.0;
            self.onset_sample(xd, detect);
            self.lifecycle();
            if self.now.is_multiple_of(TICK) {
                self.tick();
                self.render_tick();
            }
            for li in 0..NL {
                let st = self.layers[li].state;
                if st == State::Live || st == State::Held || st == State::Releasing {
                    self.schedule(li, i);
                }
            }
            if self.copy_layer.is_some() {
                self.copy_step();
            }

            let k = (self.now % TICK) as usize;
            // ---- layers
            let mut wet = [0.0f32; 2];
            for li in 0..NL {
                let lay = &mut self.layers[li];
                if lay.state == State::Free && lay.count == 0 {
                    continue;
                }
                let a = &self.tb[li];
                lay.a_t += (a.m[k] as f64) * (a.m[k] as f64);
                lay.b_t += a.b[k] as f64;
                let lg = if li == 0 { live } else { lay.gain * lay.kill as f64 };
                let gg = lg as f32;
                if mono {
                    wet[0] += gg * a.o[k];
                } else {
                    wet[0] += gg * a.l[k];
                    wet[1] += gg * a.r[k];
                }
            }
            // ---- output and feedback
            let dry_g = 1.0 - kd * (1.0 - byp);
            let mut rec_in = [0.0f32; 2];
            let clock = self.now;
            let rec_g = if self.set.trails { 1.0 - byp } else { 1.0 };
            let cuts_off = self.cuts[0].low.bypassed() && self.cuts[0].high.bypassed() && self.cuts[1].low.bypassed() && self.cuts[1].high.bypassed();
            let fb_on = fb != 0.0;
            for c in 0..(if mono { 1 } else { 2 }) {
                let w = if cuts_off { wet[c] } else { self.cuts[c].process(wet[c], clock) };
                rec_in[c] = x[c] * rec_g;
                if fb_on {
                    let (x1, y1) = self.dc[c];
                    let y = w as f64 - x1 + self.dc_k * y1;
                    self.dc[c] = (w as f64, y);
                    let yc = y as f32;
                    let (kn, ceil) = (0.707_946, 1.412_538);
                    let ay = yc.abs();
                    let clipped = if ay > kn { yc.signum() * (kn + (ceil - kn) * ((ay - kn) / (ceil - kn)).tanh()) } else { yc };
                    rec_in[c] += fb * clipped;
                } else {
                    self.dc[c] = (0.0, 0.0);
                }
                if c < nch {
                    buffers[c][i] = x[c] * dry_g + mix * w * wet_on;
                }
            }
            if mono {
                self.rec.push(&rec_in[..1]);
            } else {
                self.rec.push(&rec_in);
            }
            self.now += 1;
        }
    }
}
