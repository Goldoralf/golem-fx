// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-spectralcomp — golem's spectral compressor (design: README.md). Stands for golem's
//! `spectral-comp` block (its ten catalogue ids kept; `thresh`, `slope`, `up` and `attack` converted
//! once, README §3.4).
//!
//! - **Detector** (both modes, README §2.2): periodic Hann STFT (gfx-dsp `stft`), N = `window` × the
//!   rate family (1 / 2 / 4), hop N / 8. Bin power P_k = c |X_k|², c = 2 / (N Σ w²); band power E_k
//!   over [k − d_k, k + u_k], u_k = max(2, k (2^(r/2) − 1)), d_k = max(2, k (1 − 2^(−r/2))), r = `res`
//!   / 12 octaves, by f64 prefix sums with fractional edges; L_k = 10 log10(E_k / 0.5) (0 dB = a
//!   full-scale sine). Stereo: each channel's band power moves toward the louder by `link`.
//! - **Follower** (§2.4): one per bin in dB, α = 1 − e^(−H / (τ fs)), attack when rising; instant
//!   for the first N samples after a reset (and for the first frame after a switch).
//! - **Static curve** (§2.3, `curve`): T_k = `thresh` + `slope` · log2(f_k / `center`), downward and
//!   guarded upward with a quadratic knee, `range` and `lift_range`.
//! - **Studio** (§2.5): Y_k = X_k · 10^(g_k / 20) in the same STFT, inverse, Hann, overlap-add / 3.0;
//!   a frame runs inside the host block at the sample that completes its hop: latency N − 1.
//! - **Live** (§2.6, `live`): the same gains as two-thirds-octave targets on 27 minimum-phase
//!   third-octave bells (ACGE) and two shelves beyond the end bells, zero latency.
//! - **Worst block** (§2.9, Review change 7): a frame runs in one piece while 2 × N ≤ 16,384 /
//!   family (two channels counted whatever the layout); above, its work is cut into units spread over
//!   the hop (`stft::due`): +H of latency in Studio, +H of detection lag in Live.
//! - **Output** (§2.8): static auto make-up (pink at −18 dBFS RMS, 0 dB under a key), linear `mix`
//!   with the dry aligned, `delta` (wet − dry), `out` after the mix, `bypass` keeping the latency;
//!   meters `gr`, `lift_gr` and eight signed octave curves `dyn_*`.
//!
//! `mode` (and `window` in Studio) are set-up switches: 10 ms out, switch, silence while the new
//! engine fills its window and settles (N + four solver periods), 10 ms in. `window` in
//! Live changes at the next frame without a fade (no latency moves). `source` crossfades the gains
//! over 20 ms. Real-time safe, deterministic, block-size independent.

#![allow(clippy::needless_range_loop)]

pub mod curve;
pub mod live;

#[cfg(feature = "plugin")]
pub mod plugin;

#[cfg(test)]
mod tests;

use curve::{band_edges, band_factors, db_to_lin, level_of, makeup_db, makeup_part, Curve};
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::geq::{Plan, MAX_POINTS};
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::stft::{self, as_complex, as_complex_ref, Tables, C32, OLA_AMP_HOP8};
use gfx_dsp::{Effect, ParamInfo};
use live::{point_bins, Cascade, Slot, P_FLOOR};

pub const NP: usize = 21;

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::float("thresh", "Threshold", -80.0, 0.0, -36.0).with_exp(),
    ParamInfo::float("down", "Down", 1.0, 20.0, 4.0),
    ParamInfo::float("up", "Up", 1.0, 20.0, 1.0),
    ParamInfo::float("slope", "Slope", -24.0, 24.0, 0.0).with_exp(),
    ParamInfo::float("attack", "Attack", 1.0, 1000.0, 150.0),
    ParamInfo::enumeration("mode", "Mode", 2, 0),
    ParamInfo::float("center", "Center", 100.0, 8000.0, 420.0),
    ParamInfo::float("release", "Release", 5.0, 2000.0, 300.0),
    ParamInfo::enumeration("window", "Window", 4, 0),
    ParamInfo::float("res", "Resolution", 0.0, 12.0, 2.0),
    ParamInfo::float("knee", "Knee", 0.0, 24.0, 6.0),
    ParamInfo::float("range", "Range", 0.0, 60.0, 24.0),
    ParamInfo::float("lift_range", "Lift Range", 0.0, 24.0, 12.0),
    ParamInfo::float("guard", "Noise Guard", -96.0, -36.0, -72.0),
    ParamInfo::enumeration("source", "Source", 2, 0),
    ParamInfo::boolean("auto_gain", "Auto Gain", true),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::float("out", "Output", -24.0, 24.0, 0.0).with_exp(),
    ParamInfo::float("link", "Stereo Link", 0.0, 100.0, 100.0),
    ParamInfo::boolean("delta", "Delta", false),
    ParamInfo::boolean("bypass", "Bypass", false),
];

/// Parameter indices.
pub mod p {
    pub const THRESH: usize = 0;
    pub const DOWN: usize = 1;
    pub const UP: usize = 2;
    pub const SLOPE: usize = 3;
    pub const ATTACK: usize = 4;
    pub const MODE: usize = 5;
    pub const CENTER: usize = 6;
    pub const RELEASE: usize = 7;
    pub const WINDOW: usize = 8;
    pub const RES: usize = 9;
    pub const KNEE: usize = 10;
    pub const RANGE: usize = 11;
    pub const LIFT_RANGE: usize = 12;
    pub const GUARD: usize = 13;
    pub const SOURCE: usize = 14;
    pub const AUTO_GAIN: usize = 15;
    pub const MIX: usize = 16;
    pub const OUT: usize = 17;
    pub const LINK: usize = 18;
    pub const DELTA: usize = 19;
    pub const BYPASS: usize = 20;
}

/// `mode` option labels (enum order, final: append only).
pub const MODES: [&str; 2] = ["Live", "Studio"];
pub const LIVE: usize = 0;
pub const STUDIO: usize = 1;
/// `window` options (enum order, final: the catalogue's three, then 512 appended), samples at 48 kHz.
pub const WINDOWS: [usize; 4] = [1024, 2048, 4096, 512];
/// `source` option labels (enum order, final; gfx-gate's).
pub const SOURCES: [&str; 2] = ["Main", "Key"];
pub const MAIN: usize = 0;
pub const KEY_SRC: usize = 1;
/// The read-only meters, in the plugin's order.
pub const METERS: [&str; 10] = ["gr", "lift_gr", "dyn_63", "dyn_125", "dyn_250", "dyn_500", "dyn_1k", "dyn_2k", "dyn_4k", "dyn_8k"];
/// The meters' octave centres (Hz): 1 kHz × 2^k, k = −4 … 3.
pub const OCTAVES: [f64; 8] = [62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0];

const MAX_CH: usize = 2;
/// Signal slots: the two main channels and the key.
const KEY: usize = 2;
const NSIG: usize = 3;
/// Hop N / 8, both modes (README §2.5, Review change 9).
pub const HOP_DIV: usize = 8;
/// Above this many channel-points (× the rate family) a frame is cut into units over the hop.
const ONE_PIECE_POINTS: usize = 16384;
const SETUP_FADE_MS: f32 = 10.0;
/// The curve's set-and-forget values (`down`, `up`, `center`, `knee`, `range`, `lift_range`,
/// `guard`, `res`) glide to a new value at the frame rate with this time constant.
const CURVE_GLIDE_S: f32 = 0.02;
/// The parameters that glide, in [`SpectralComp::glide`]'s order.
const GLIDE: [usize; 8] = [p::DOWN, p::UP, p::CENTER, p::KNEE, p::RANGE, p::LIFT_RANGE, p::GUARD, p::RES];
const SOURCE_XFADE_S: f32 = 0.02;
const MAKEUP_MS: f32 = 50.0;
const METER_HOLD_S: f32 = 0.05;
const MAX_PASSES: usize = 24;
/// Bins (or samples) per unit of a per-bin pass, and design points per unit of the targets: small
/// enough that the units spread over the hop keep every 64-sample block light (README §2.9).
const PIECE: usize = 256;
const POINTS_PER_UNIT: usize = 14;

/// Number of pieces of at most [`PIECE`] covering `len` items.
#[inline]
fn pieces(len: usize) -> usize {
    len.div_ceil(PIECE)
}

/// The item range of piece `u` out of `len`.
#[inline]
fn piece(len: usize, u: usize) -> (usize, usize) {
    ((u * PIECE).min(len), ((u + 1) * PIECE).min(len))
}

/// The rate family: × 1 at 44.1 / 48 kHz, × 2 at 88.2 / 96, × 4 at 176.4 / 192.
pub fn family(sample_rate: f32) -> usize {
    if sample_rate < 60000.0 {
        1
    } else if sample_rate < 120000.0 {
        2
    } else {
        4
    }
}

#[inline]
fn idx(v: f32, n: usize) -> usize {
    (v.round().max(0.0) as usize).min(n - 1)
}

#[inline]
fn db_gain(db: f32) -> f32 {
    if db == 0.0 {
        1.0
    } else {
        (db * (std::f32::consts::LN_10 / 20.0)).exp()
    }
}

/// What one STFT size needs, rebuilt on a `window` change (no allocation).
#[derive(Clone, Debug)]
struct Layout {
    n: usize,
    h: usize,
    m: usize,
    nb: usize,
    /// index of the size in the stft grid
    si: usize,
    /// power calibration 2 / (N Σ w²)
    c: f32,
    bin: f32,
    /// log2 f_k (bin 0 takes bin 1's)
    log2f: Vec<f32>,
    /// each bin's meter octave (`u8::MAX` = none)
    oct: Vec<u8>,
    /// the periodic Hann window
    win: Vec<f32>,
    /// each design point's bins (inclusive)
    pts: [(u32, u32); MAX_POINTS],
    np: usize,
    /// the frame is cut into units over the hop
    split: bool,
}

impl Layout {
    fn new(cap_nb: usize) -> Self {
        Self { n: 0, h: 1, m: 0, nb: 0, si: 0, c: 0.0, bin: 0.0, log2f: vec![0.0; cap_nb], oct: vec![u8::MAX; cap_nb], win: vec![0.0; 2 * cap_nb], pts: [(1, 1); MAX_POINTS], np: 0, split: false }
    }

    fn set(&mut self, n: usize, sr: f32, nch: usize, fam: usize, plan: &Plan, t: &Tables) {
        self.n = n;
        self.h = n / HOP_DIV;
        self.m = n / 2;
        self.nb = n / 2 + 1;
        self.si = (0..stft::NUM_SIZES).find(|&i| stft::size(i) == n).expect("a grid size");
        let sp = &t.plans[self.si];
        for i in 0..n {
            self.win[i] = sp.hann(t, i);
        }
        // Σ w² of the periodic Hann = 3N / 8
        self.c = (2.0 / (n as f64 * 0.375 * n as f64)) as f32;
        self.bin = sr / n as f32;
        for k in 0..self.nb {
            let f = (k.max(1) as f64) * sr as f64 / n as f64;
            self.log2f[k] = f.log2() as f32;
            let kf = k as f64 * sr as f64 / n as f64;
            self.oct[k] = OCTAVES.iter().position(|&fc| kf >= fc / std::f64::consts::SQRT_2 && kf < fc * std::f64::consts::SQRT_2).map_or(u8::MAX, |o| o as u8);
        }
        self.np = plan.points().len();
        point_bins(plan.points(), n, sr as f64, &mut self.pts);
        // counted on two channels whatever the layout: mono and stereo then behave alike, and the
        // reported latency never depends on the channel count
        let _ = nch;
        self.split = MAX_CH * n > ONE_PIECE_POINTS / fam;
    }
}

/// One signal's analysis: its input ring, FFT buffer, bin powers and their prefix sum.
struct Sig {
    ring: Vec<f32>,
    buf: Vec<f32>,
    p: Vec<f32>,
    s: Vec<f64>,
}

/// One main channel: its spectrum (Studio), gains, overlap-add and Live cascade.
struct Chan {
    x: Vec<C32>,
    gout: Vec<f32>,
    gfrom: Vec<f32>,
    glin: Vec<f32>,
    ola: Vec<f32>,
    casc: Cascade,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Fwd,
    Post,
    Prefix,
    Detect,
    Sums,
    Targets,
    Apply,
    Inv,
    Ola,
    Makeup,
    Finish,
}

#[derive(Clone, Copy, Debug)]
struct Pass {
    kind: Kind,
    arg: u8,
    units: u16,
}

/// What a frame reads, fixed at its start.
#[derive(Clone, Copy, Debug)]
struct Snap {
    curve: Curve,
    fu: f32,
    fd: f32,
    aa: f32,
    ar: f32,
    instant: bool,
    link: f32,
    key: bool,
    studio: bool,
    nsolve: usize,
    xf: f32,
    res: f32,
}

/// The frame in progress.
struct Frame {
    passes: [Pass; MAX_PASSES],
    np: usize,
    pi: usize,
    pu: usize,
    total: usize,
    done: usize,
    end: u64,
    pending: bool,
    snap: Snap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Setup {
    Idle,
    Out,
    /// silent after the switch: the new engine fills its window and settles
    Wait,
    In,
}

pub struct SpectralComp {
    v: [f32; NP],
    sr: f32,
    fam: usize,
    nch: usize,
    clock: u64,
    tables: Option<Box<Tables>>,
    tmp: Vec<C32>,
    scratch: Vec<C32>,
    ring_mask: usize,
    // smoothers
    thresh: ExpSmoother,
    slope: ExpSmoother,
    mix: ExpSmoother,
    out: ExpSmoother,
    link: OnePole,
    makeup: OnePole,
    delta_g: LinearRamp,
    bypass_g: LinearRamp,
    setup_g: LinearRamp,
    setup: Setup,
    setup_wait: usize,
    /// the gliding curve values (GLIDE order)
    glide: [f32; 8],
    // the engine in use
    mode: usize,
    window: usize,
    source: usize,
    lay: Layout,
    since: usize,
    instant_frames: usize,
    sig: [Sig; NSIG],
    ch: [Chan; MAX_CH],
    lbar: [Vec<f32>; NSIG],
    /// the Live targets' weights per bin, per solver: P_k + floor and P_k · 10^(g_k / 10)
    sp: [Vec<f64>; MAX_CH],
    sg: [Vec<f64>; MAX_CH],
    /// each solver's G_p before the [¼ ½ ¼] smoothing
    gp: [[f64; MAX_POINTS]; MAX_CH],
    fr: Frame,
    // Live
    plan: Option<Box<Plan>>,
    slots: [Slot; MAX_CH],
    shared: bool,
    tick_len: u32,
    flat_hold: u32,
    // `source` crossfade of the gains
    xf: f32,
    xstep: f32,
    // auto make-up
    mk_for: Option<(Curve, f32, usize)>,
    mk_sum: (f64, f64),
    mk_db: f32,
    // meters
    acc: [[f64; 4]; 8],
    m_dyn: [f32; 8],
    m_dyn_age: [u32; 8],
    m_gr: f32,
    m_gr_age: u32,
    m_lift: f32,
    m_lift_age: u32,
    hold_len: u32,
}

impl SpectralComp {
    pub fn new() -> Self {
        let v: [f32; NP] = core::array::from_fn(|i| PARAMS[i].default);
        let snap = Snap {
            curve: Curve { thresh: -36.0, down: 4.0, up: 1.0, slope: 0.0, center: 420.0, knee: 6.0, range: 24.0, lift_range: 12.0, guard: -72.0 },
            fu: 0.0,
            fd: 0.0,
            aa: 1.0,
            ar: 1.0,
            instant: true,
            link: 1.0,
            key: false,
            studio: false,
            nsolve: 1,
            xf: 1.0,
            res: 2.0,
        };
        let mut s = Self {
            v,
            sr: 48000.0,
            fam: 1,
            nch: 1,
            clock: 0,
            tables: None,
            tmp: Vec::new(),
            scratch: Vec::new(),
            ring_mask: 0,
            thresh: ExpSmoother::new(-80.0, 0.0, v[p::THRESH]),
            slope: ExpSmoother::new(-24.0, 24.0, v[p::SLOPE]),
            mix: ExpSmoother::new(0.0, 100.0, v[p::MIX]),
            out: ExpSmoother::new(-24.0, 24.0, v[p::OUT]),
            link: OnePole::new(20.0, v[p::LINK], 1e-4),
            makeup: OnePole::new(MAKEUP_MS, 0.0, 1e-5),
            delta_g: LinearRamp::new(10.0, 0.0),
            bypass_g: LinearRamp::new(10.0, 1.0),
            setup_g: LinearRamp::new(SETUP_FADE_MS, 1.0),
            setup: Setup::Idle,
            setup_wait: 0,
            glide: core::array::from_fn(|i| PARAMS[GLIDE[i]].default),
            mode: LIVE,
            window: 0,
            source: MAIN,
            lay: Layout::new(0),
            since: 0,
            instant_frames: HOP_DIV,
            sig: core::array::from_fn(|_| Sig { ring: Vec::new(), buf: Vec::new(), p: Vec::new(), s: Vec::new() }),
            ch: core::array::from_fn(|_| Chan { x: Vec::new(), gout: Vec::new(), gfrom: Vec::new(), glin: Vec::new(), ola: Vec::new(), casc: Cascade::default() }),
            lbar: core::array::from_fn(|_| Vec::new()),
            sp: core::array::from_fn(|_| Vec::new()),
            sg: core::array::from_fn(|_| Vec::new()),
            gp: [[0.0; MAX_POINTS]; MAX_CH],
            fr: Frame { passes: [Pass { kind: Kind::Finish, arg: 0, units: 1 }; MAX_PASSES], np: 0, pi: 0, pu: 0, total: 0, done: 0, end: 0, pending: false, snap },
            plan: None,
            slots: [Slot::new(), Slot::new()],
            shared: false,
            tick_len: 64,
            flat_hold: 38,
            xf: 1.0,
            xstep: 1.0,
            mk_for: None,
            mk_sum: (0.0, 0.0),
            mk_db: 0.0,
            acc: [[0.0; 4]; 8],
            m_dyn: [0.0; 8],
            m_dyn_age: [0; 8],
            m_gr: 0.0,
            m_gr_age: 0,
            m_lift: 0.0,
            m_lift_age: 0,
            hold_len: 2400,
        };
        s.prepare(48000.0, 512, 1);
        s.reset();
        s
    }

    // ---- accessors (meters, tests, the bench) ----

    /// The STFT size in use.
    pub fn n(&self) -> usize {
        self.lay.n
    }
    /// The hop in use.
    pub fn hop(&self) -> usize {
        self.lay.h
    }
    /// The frame is cut into units over the hop (README §2.9).
    pub fn is_split(&self) -> bool {
        self.lay.split
    }
    /// The engine in use (`LIVE` / `STUDIO`).
    pub fn mode_in_use(&self) -> usize {
        self.mode
    }
    /// Bin spacing (Hz).
    pub fn bin_hz(&self) -> f32 {
        self.lay.bin
    }
    /// Channel `c`'s smoothed band levels (dB, per bin); `c` = 2 is the key.
    pub fn levels(&self, c: usize) -> &[f32] {
        &self.lbar[c][..self.lay.nb]
    }
    /// Channel `c`'s gains (dB, per bin) from the last frame.
    pub fn gains(&self, c: usize) -> &[f32] {
        &self.ch[c].gout[..self.lay.nb]
    }
    /// The make-up in use (dB, after its smoothing).
    pub fn makeup_db(&self) -> f32 {
        self.makeup.value()
    }
    /// The make-up the static curve asks for (dB, auto gain on, no key).
    pub fn makeup_target_db(&self) -> f32 {
        self.mk_db
    }
    /// Live: solver `s`'s section gains and its largest (tests).
    pub fn live_slot(&self, s: usize) -> &Slot {
        &self.slots[s]
    }
    /// The Live plan in use.
    pub fn live_plan(&self) -> &Plan {
        self.plan.as_deref().expect("prepared")
    }
    /// `gr`: the largest octave cut (dB, ≥ 0), held 50 ms.
    pub fn gr_db(&self) -> f32 {
        self.m_gr
    }
    /// `lift_gr`: the largest octave lift (dB, ≥ 0), held 50 ms.
    pub fn lift_db(&self) -> f32 {
        self.m_lift
    }
    /// `dyn_*`: the signed octave gain changes (dB), held 50 ms.
    pub fn dyn_db(&self) -> [f32; 8] {
        self.m_dyn
    }

    fn n_for(&self, w: usize) -> usize {
        WINDOWS[w] * self.fam
    }

    fn relayout(&mut self) {
        let n = self.n_for(self.window);
        let plan = self.plan.as_deref().expect("prepared");
        let t = self.tables.as_deref().expect("prepared");
        self.lay.set(n, self.sr, self.nch, self.fam, plan, t);
    }

    /// Latency of the engine in use: Live 0; Studio N − 1, + H when the frame is cut over the hop.
    fn latency_now(&self) -> usize {
        if self.mode == STUDIO {
            self.lay.n - 1 + if self.lay.split { self.lay.h } else { 0 }
        } else {
            0
        }
    }

    fn want_mode(&self) -> usize {
        idx(self.v[p::MODE], 2)
    }
    fn want_window(&self) -> usize {
        idx(self.v[p::WINDOW], 4)
    }
    fn want_source(&self) -> usize {
        idx(self.v[p::SOURCE], 2)
    }

    fn curve_now(&self) -> Curve {
        let g = &self.glide;
        Curve { thresh: self.thresh.value(), down: g[0], up: g[1], slope: self.slope.value(), center: g[2], knee: g[3], range: g[4], lift_range: g[5], guard: g[6] }
    }

    /// The gliding values one frame on (or settled).
    fn glide_step(&mut self, settle: bool) {
        let a = 1.0 - (-(self.lay.h as f32) / (CURVE_GLIDE_S * self.sr)).exp();
        for (k, &i) in GLIDE.iter().enumerate() {
            let pi = &PARAMS[i];
            let t = self.v[i].clamp(pi.min, pi.max);
            let g = &mut self.glide[k];
            if settle || (t - *g).abs() <= 1e-5 * (pi.max - pi.min) {
                *g = t;
            } else {
                *g += a * (t - *g);
            }
        }
    }

    /// The make-up the settings ask for now (dB): 0 with auto gain off or under a key.
    fn makeup_want(&self) -> f32 {
        if self.v[p::AUTO_GAIN] < 0.5 || self.source == KEY_SRC {
            0.0
        } else {
            self.mk_db
        }
    }

    /// The whole make-up sum at once (reset, prepare).
    fn makeup_now(&mut self) {
        let c = self.curve_now();
        let res = self.glide[7];
        let (w, g) = makeup_part(&c, self.lay.n, self.sr, res, 0, self.lay.nb);
        self.mk_db = makeup_db(w, g);
        self.mk_for = Some((c, res, self.lay.n));
    }

    /// Clears the analysis and synthesis state of the engine (switch, reset); the rings keep their
    /// history.
    fn clear_engine(&mut self) {
        for c in self.ch.iter_mut() {
            c.ola.fill(0.0);
            c.gout.fill(0.0);
            c.gfrom.fill(0.0);
            c.glin.fill(1.0);
            c.casc.reset();
        }
        for l in self.lbar.iter_mut() {
            l.fill(curve::FLOOR_DB);
        }
        for s in self.slots.iter_mut() {
            s.reset();
        }
        self.fr.pending = false;
        self.since = 0;
        self.xf = 1.0;
    }

    /// Applies a `mode` / `window` switch at the silent point of a set-up fade.
    fn apply_setup(&mut self) {
        self.mode = self.want_mode();
        self.window = self.want_window();
        self.relayout();
        self.clear_engine();
        self.instant_frames = 1;
    }

    // ---- the frame ----

    fn push_pass(&mut self, kind: Kind, arg: usize, units: usize) {
        let f = &mut self.fr;
        f.passes[f.np] = Pass { kind, arg: arg as u8, units: units as u16 };
        f.np += 1;
        f.total += units;
    }

    /// Starts the frame whose last sample is `end` (the sample just pushed).
    fn start_frame(&mut self, end: u64) {
        // a `window` change in Live: at this frame, no fade (no latency moves)
        if self.mode == LIVE && self.setup == Setup::Idle && self.want_window() != self.window {
            self.window = self.want_window();
            self.relayout();
            self.since = 0;
            self.instant_frames = self.instant_frames.max(1);
        }
        // `source`: crossfade the gains over 20 ms, the new detector reads instantly
        let src = self.want_source();
        if src != self.source {
            self.source = src;
            for c in self.ch.iter_mut() {
                let nb = self.lay.nb;
                let (from, out) = (&mut c.gfrom, &c.gout);
                from[..nb].copy_from_slice(&out[..nb]);
            }
            self.xf = 0.0;
            self.xstep = (self.lay.h as f32 / (SOURCE_XFADE_S * self.sr)).min(1.0);
            self.instant_frames = self.instant_frames.max(1);
        }
        if self.xf < 1.0 {
            self.xf = (self.xf + self.xstep).min(1.0);
        }
        let (h, nb, m, n) = (self.lay.h, self.lay.nb, self.lay.m, self.lay.n);
        let fs = self.sr;
        let hf = h as f32;
        let tau = |ms: f32| 1.0 - (-hf / (ms.max(0.1) * 0.001 * fs)).exp();
        self.glide_step(false);
        let curve = self.curve_now();
        let res = self.glide[7];
        let (fu, fd) = band_factors(res);
        let key = self.source == KEY_SRC;
        let studio = self.mode == STUDIO;
        let link = (self.link.value() * 0.01).clamp(0.0, 1.0);
        let nch = self.nch;
        let nsolve = if nch == 2 && !key && link < 1.0 { 2 } else { 1 };
        self.shared = nch == 2 && nsolve == 1;
        if self.shared && !studio {
            // the second channel follows the first solver
            let s0 = self.slots[0].clone();
            self.slots[1] = s0;
        }
        let makeup = self.mk_for != Some((curve, res, n));
        let instant = self.instant_frames > 0;
        self.instant_frames = self.instant_frames.saturating_sub(1);
        self.fr.snap = Snap {
            curve,
            fu,
            fd,
            aa: tau(self.v[p::ATTACK].clamp(1.0, 1000.0)),
            ar: tau(self.v[p::RELEASE].clamp(5.0, 2000.0)),
            instant,
            link,
            key,
            studio,
            nsolve,
            xf: self.xf,
            res,
        };
        // the passes
        self.fr.np = 0;
        self.fr.total = 0;
        let post = pieces(nb);
        // an FFT runs whole on its pass's first unit; the pass counts as many units as its cost in
        // pieces, so the schedule over the hop leaves room after it
        let fft_units = (n / (4 * PIECE)).max(1);
        let dets: &[usize] = if key { &[KEY] } else if nch == 2 { &[0, 1] } else { &[0] };
        let dets_arr = [dets[0], *dets.get(1).unwrap_or(&usize::MAX)];
        let ndet = dets.len();
        let mut fwd = [usize::MAX; NSIG];
        let mut nf = 0;
        for &d in &dets_arr[..ndet] {
            fwd[nf] = d;
            nf += 1;
        }
        if studio && key {
            for c in 0..nch {
                fwd[nf] = c;
                nf += 1;
            }
        }
        for &s in &fwd[..nf] {
            self.push_pass(Kind::Fwd, s, fft_units);
            self.push_pass(Kind::Post, s, post);
        }
        for &d in &dets_arr[..ndet] {
            self.push_pass(Kind::Prefix, d, post);
        }
        self.push_pass(Kind::Detect, 0, post);
        self.push_pass(Kind::Sums, 0, post);
        if studio {
            for c in 0..nch {
                self.push_pass(Kind::Apply, c, pieces(m));
                self.push_pass(Kind::Inv, c, fft_units);
                self.push_pass(Kind::Ola, c, pieces(n));
            }
        } else {
            self.push_pass(Kind::Targets, 0, self.lay.np.div_ceil(POINTS_PER_UNIT));
        }
        if makeup {
            self.push_pass(Kind::Makeup, 0, post);
        }
        self.push_pass(Kind::Finish, 0, 1);
        self.fr.pi = 0;
        self.fr.pu = 0;
        self.fr.done = 0;
        self.fr.end = end;
        self.fr.pending = true;
    }

    /// Runs the frame's units until `upto` are done.
    fn run_units(&mut self, upto: usize) {
        while self.fr.pending && self.fr.done < upto {
            let pass = self.fr.passes[self.fr.pi];
            let u = self.fr.pu;
            self.run_unit(pass.kind, pass.arg as usize, u, pass.units as usize);
            self.fr.done += 1;
            self.fr.pu += 1;
            if self.fr.pu >= pass.units as usize {
                self.fr.pu = 0;
                self.fr.pi += 1;
                if self.fr.pi >= self.fr.np {
                    self.fr.pending = false;
                }
            }
        }
    }

    fn run_unit(&mut self, kind: Kind, arg: usize, u: usize, units: usize) {
        let nb = self.lay.nb;
        match kind {
            Kind::Detect => return self.detect(piece(nb, u)),
            Kind::Sums => return self.sums(piece(nb, u), u == 0),
            Kind::Targets => return self.targets(u, units),
            Kind::Finish => return self.finish(),
            _ => {}
        }
        let t = self.tables.as_deref().expect("prepared");
        let lay = &self.lay;
        let plan = &t.plans[lay.si];
        let (n, nb, m) = (lay.n, lay.nb, lay.m);
        let snap = self.fr.snap;
        let end = self.fr.end;
        let mask = self.ring_mask;
        match kind {
            Kind::Fwd | Kind::Inv if u > 0 => {}
            Kind::Fwd => {
                let sg = &mut self.sig[arg];
                let start = end.wrapping_sub(n as u64 - 1) as usize;
                let win = &lay.win[..n];
                for i in 0..n {
                    sg.buf[i] = sg.ring[start.wrapping_add(i) & mask] * win[i];
                }
                stft::cfft_unit(plan, t, as_complex(&mut sg.buf[..n]), &mut self.tmp, &mut self.scratch, 0);
            }
            Kind::Post => {
                let (k0, k1) = piece(nb, u);
                let c = lay.c;
                let keep = snap.studio && arg < MAX_CH;
                let sg = &mut self.sig[arg];
                let pw = &mut sg.p;
                let xs = &mut self.ch[arg.min(MAX_CH - 1)].x;
                stft::post_twiddle(plan, t, as_complex_ref(&sg.buf[..n]), k0, k1, |k, x| {
                    pw[k] = c * x.norm_sqr();
                    if keep {
                        xs[k] = x;
                    }
                });
            }
            Kind::Prefix => {
                // in order, piece by piece
                let (k0, k1) = piece(nb, u);
                let sg = &mut self.sig[arg];
                if u == 0 {
                    sg.s[0] = 0.0;
                }
                for i in k0..k1 {
                    sg.s[i + 1] = sg.s[i] + sg.p[i] as f64;
                }
            }
            Kind::Apply => {
                let (k0, k1) = piece(m, u);
                let c = &mut self.ch[arg];
                let gl = &c.glin;
                // DC and Nyquist take their neighbour's gain
                let g = |k: usize| gl[k.clamp(1, m - 1)];
                let z = as_complex(&mut self.sig[arg].buf[..n]);
                for k in k0..k1 {
                    let yk = c.x[k] * g(k);
                    let ym = c.x[m - k] * g(m - k);
                    z[k] = stft::pre_twiddle(plan, t, k, yk, ym);
                }
            }
            Kind::Inv => {
                stft::cfft_unit(plan, t, as_complex(&mut self.sig[arg].buf[..n]), &mut self.tmp, &mut self.scratch, 0);
            }
            Kind::Ola => {
                let (r0, r1) = piece(n, u);
                let k = 1.0 / (n as f32 * OLA_AMP_HOP8);
                let start = end.wrapping_sub(n as u64 - 1) as usize;
                let ola = &mut self.ch[arg].ola;
                let win = &lay.win[..n];
                stft::read_time(plan, as_complex_ref(&self.sig[arg].buf[..n]), r0, r1, |r, v| {
                    let y = v * k * win[r];
                    ola[start.wrapping_add(r) & mask] += if y.is_finite() { y } else { 0.0 };
                });
            }
            Kind::Makeup => {
                let (k0, k1) = piece(nb, u);
                if u == 0 {
                    self.mk_sum = (0.0, 0.0);
                }
                let (w, g) = makeup_part(&snap.curve, n, self.sr, snap.res, k0, k1);
                self.mk_sum.0 += w;
                self.mk_sum.1 += g;
                if u + 1 == units {
                    self.mk_db = makeup_db(self.mk_sum.0, self.mk_sum.1);
                    self.mk_for = Some((snap.curve, snap.res, n));
                }
            }
            Kind::Detect | Kind::Sums | Kind::Targets | Kind::Finish => {}
        }
    }

    /// Bins [k0, k1): band power, link, follower, curve; each channel's gains. In four loops (the
    /// band sums, the levels, the follower and curve, the linear gains) so the second and fourth
    /// vectorise; the analysed signals' FFT buffers (free by now) hold the levels.
    fn detect(&mut self, (k0, k1): (usize, usize)) {
        let snap = self.fr.snap;
        let nch = self.nch;
        let nb = self.lay.nb;
        let nbf = nb as f32;
        let bin = self.lay.bin;
        let pc = snap.curve.prepared();
        let t0 = snap.curve.t0();
        let slope = snap.curve.slope;
        let key = snap.key;
        let ndet = if key { 1 } else { nch };
        let (fu, fd) = (snap.fu, snap.fd);
        let (aa, ar, instant) = (snap.aa, snap.ar, snap.instant);
        let link = snap.link as f64;
        let xf = snap.xf;
        let pos = |y: f32| {
            let x = (y + 0.5).clamp(0.0, nbf);
            let i = (x as usize).min(nb - 1);
            (i, (x - i as f32) as f64)
        };
        let [s0, s1, s2] = &mut self.sig;
        let sa = if key { s2 } else { s0 };
        let Sig { s: sas, p: sap, buf: ba, .. } = sa;
        let Sig { s: sbs, p: sbp, buf: bb, .. } = s1;
        // band powers / 0.5, linked
        for k in k0..k1 {
            let (lo, hi) = band_edges(k, fu, fd);
            let ((il, fl), (ih, fh)) = (pos(lo), pos(hi));
            let mut ea = (sas[ih] + fh * sap[ih] as f64) - (sas[il] + fl * sap[il] as f64);
            if ndet == 2 {
                let mut eb = (sbs[ih] + fh * sbp[ih] as f64) - (sbs[il] + fl * sbp[il] as f64);
                if link > 0.0 {
                    let mx = ea.max(eb);
                    if link >= 1.0 {
                        ea = mx;
                        eb = mx;
                    } else {
                        ea += link * (mx - ea);
                        eb += link * (mx - eb);
                    }
                }
                bb[k] = (eb / curve::FULL_SCALE_POWER) as f32;
            }
            ba[k] = (ea / curve::FULL_SCALE_POWER) as f32;
        }
        // levels
        for v in ba[k0..k1].iter_mut() {
            *v = level_of(*v);
        }
        if ndet == 2 {
            for v in bb[k0..k1].iter_mut() {
                *v = level_of(*v);
            }
        }
        // follower and curve, per detector: its raw gains replace its levels
        let log2f = &self.lay.log2f[..nb];
        let [l0, l1, l2] = &mut self.lbar;
        let la = if key { l2 } else { l0 };
        let run = |lev: &mut [f32], lbar: &mut [f32]| {
            for k in k0..k1 {
                let l = lev[k];
                let lb = if instant {
                    l
                } else {
                    let d = l - lbar[k];
                    lbar[k] + if d > 0.0 { aa } else { ar } * d
                };
                lbar[k] = lb;
                lev[k] = pc.gain(lb, t0 + slope * log2f[k], k as f32 * bin);
            }
        };
        run(ba, la);
        if ndet == 2 {
            run(bb, l1);
        }
        // each channel's gains (a key drives both), crossfaded after a `source` change
        let [c0, c1] = &mut self.ch;
        for (c, ch) in [c0, c1].into_iter().enumerate().take(nch) {
            let raw: &[f32] = if c == 1 && ndet == 2 { bb } else { ba };
            for k in k0..k1 {
                let g = raw[k];
                ch.gout[k] = if xf < 1.0 { ch.gfrom[k] + xf * (g - ch.gfrom[k]) } else { g };
            }
            let (go, gl) = (&ch.gout[k0..k1], &mut ch.glin[k0..k1]);
            for (l, &g) in gl.iter_mut().zip(go) {
                *l = db_to_lin(g);
            }
        }
    }

    /// Bins [k0, k1): the Live targets' weights (P_k + floor, and P_k · 10^(g_k / 10)) and the
    /// meters' octave sums. The targets sum their windows directly: a prefix difference would be
    /// rounding noise where the weights sit at the floor under a loud band (a keyed tone).
    fn sums(&mut self, (k0, k1): (usize, usize), first: bool) {
        let snap = self.fr.snap;
        let nch = self.nch;
        let live = !snap.studio;
        if first {
            self.acc = [[0.0; 4]; 8];
        }
        let [s0, s1, s2] = &self.sig;
        let weight = |s: usize, k: usize| -> f64 {
            let p = if snap.key {
                s2.p[k]
            } else if nch == 2 && snap.nsolve == 1 {
                s0.p[k].max(s1.p[k])
            } else if s == 0 {
                s0.p[k]
            } else {
                s1.p[k]
            };
            p as f64 + P_FLOOR
        };
        for k in k0..k1 {
            if live {
                for s in 0..snap.nsolve {
                    let w = weight(s, k);
                    let gl = self.ch[s].glin[k] as f64;
                    self.sp[s][k] = w;
                    self.sg[s][k] = w * gl * gl;
                }
            }
            let o = self.lay.oct[k];
            if (o as usize) < 8 {
                let w = weight(0, k);
                let g = self.ch[0].gout[k];
                let gl = self.ch[0].glin[k] as f64;
                let p2 = w * gl * gl;
                let a = &mut self.acc[o as usize];
                a[0] += w;
                a[1] += p2;
                a[2] += if g < 0.0 { p2 } else { w };
                a[3] += if g > 0.0 { p2 } else { w };
            }
        }
    }

    /// Live: each solver's targets at the design points (the target rule): unit `u` sums the windows
    /// of its points; the last one smooths [¼ ½ ¼] and installs them for the ramp.
    fn targets(&mut self, u: usize, units: usize) {
        let snap = self.fr.snap;
        let np = self.lay.np;
        let (p0, p1) = ((u * POINTS_PER_UNIT).min(np), ((u + 1) * POINTS_PER_UNIT).min(np));
        for s in 0..snap.nsolve {
            for i in p0..p1 {
                let (lo, hi) = self.lay.pts[i];
                let (lo, hi) = (lo as usize, hi as usize + 1);
                let num: f64 = self.sg[s][lo..hi].iter().sum();
                let den: f64 = self.sp[s][lo..hi].iter().sum();
                self.gp[s][i] = if num == den { 0.0 } else { 10.0 * (num.max(1e-300) / den.max(1e-300)).log10() };
            }
        }
        if u + 1 < units {
            return;
        }
        for s in 0..snap.nsolve {
            let g = &self.gp[s];
            let mut t = [0.0f64; MAX_POINTS];
            for i in 0..np {
                t[i] = 0.25 * g[i.saturating_sub(1)] + 0.5 * g[i] + 0.25 * g[(i + 1).min(np - 1)];
            }
            self.slots[s].install(&t, self.clock, self.lay.h);
        }
        if self.shared {
            let s0 = self.slots[0].clone();
            self.slots[1] = s0;
        }
    }

    /// The frame's end: the meters (held 50 ms).
    fn finish(&mut self) {
        let h = self.lay.h as u32;
        let hold = self.hold_len;
        let (mut gr, mut lift) = (0.0f32, 0.0f32);
        for o in 0..8 {
            let a = self.acc[o];
            let (d, cut, up) = if a[0] > 0.0 {
                ((10.0 * (a[1] / a[0]).log10()) as f32, (-10.0 * (a[2] / a[0]).log10()) as f32, (10.0 * (a[3] / a[0]).log10()) as f32)
            } else {
                (0.0, 0.0, 0.0)
            };
            gr = gr.max(cut);
            lift = lift.max(up);
            if d.abs() >= self.m_dyn[o].abs() || self.m_dyn_age[o] >= hold {
                self.m_dyn[o] = d;
                self.m_dyn_age[o] = 0;
            } else {
                self.m_dyn_age[o] += h;
            }
        }
        if gr >= self.m_gr || self.m_gr_age >= hold {
            self.m_gr = gr;
            self.m_gr_age = 0;
        } else {
            self.m_gr_age += h;
        }
        if lift >= self.m_lift || self.m_lift_age >= hold {
            self.m_lift = lift;
            self.m_lift_age = 0;
        } else {
            self.m_lift_age += h;
        }
    }

    /// One Live control tick at sample `now`: each solver's correction, the channels' coefficients.
    fn live_tick(&mut self, now: u64) {
        let plan = self.plan.as_deref().expect("prepared");
        let (len, hop, hold) = (self.tick_len, self.lay.h, self.flat_hold);
        let ns = if self.shared || self.nch == 1 { 1 } else { 2 };
        for s in 0..ns {
            self.slots[s].tick(plan, now, len, hop, hold);
        }
        if self.shared {
            let s0 = self.slots[0].clone();
            self.slots[1] = s0;
        }
        for c in 0..self.nch {
            let slot = &self.slots[c];
            let n = slot.n;
            if slot.restart {
                let casc = &mut self.ch[c].casc;
                casc.reset();
                casc.set(&slot.zero, &slot.zero_shelf, n);
                casc.ramp(&slot.designs, &slot.shelf, n, len);
            } else if slot.changed && slot.engaged {
                self.ch[c].casc.ramp(&slot.designs, &slot.shelf, n, len);
            }
        }
    }

    pub fn process_with_key(&mut self, buffers: &mut [&mut [f32]], key: Option<&[&[f32]]>) {
        let nch = buffers.len().min(self.nch);
        if nch == 0 {
            return;
        }
        let n = buffers[0].len();
        let key = key.filter(|k| !k.is_empty());
        let nk = key.map_or(0, |k| k.len().min(2));
        let mask = self.ring_mask;
        for i in 0..n {
            self.thresh.next();
            self.slope.next();
            let mix = (self.mix.next() * 0.01).clamp(0.0, 1.0);
            let out_db = self.out.next();
            self.link.next();
            self.makeup.set_target(self.makeup_want());
            let mk = db_gain(self.makeup.next());
            let outg = db_gain(out_db);
            // smoothstep crossfades (no slope corners)
            let ss = |v: f32| v * v * (3.0 - 2.0 * v);
            let dl = ss(self.delta_g.next());
            let byp = ss(self.bypass_g.next());
            // set-up switch: fade out, switch at silence, wait for the new engine (a full window
            // and four solver periods), fade in; smoothstep fades (no slope corners)
            let lin = self.setup_g.next();
            let sg = lin * lin * (3.0 - 2.0 * lin);
            match self.setup {
                Setup::Out if self.setup_g.is_settled() => {
                    self.apply_setup();
                    self.setup = Setup::Wait;
                    self.setup_wait = self.lay.n + 4 * self.tick_len as usize;
                }
                Setup::Wait => {
                    self.setup_wait = self.setup_wait.saturating_sub(1);
                    if self.setup_wait == 0 {
                        self.setup = Setup::In;
                        self.setup_g.set_target(1.0);
                    }
                }
                Setup::In if self.setup_g.is_settled() => self.setup = Setup::Idle,
                _ => {}
            }

            // input
            let now = self.clock;
            let pos = now as usize & mask;
            let mut x = [0.0f32; MAX_CH];
            for c in 0..nch {
                let s = buffers[c][i];
                x[c] = if s.is_finite() { s } else { 0.0 };
                self.sig[c].ring[pos] = x[c];
            }
            let kv = match key {
                Some(k) => {
                    let a = k[0].get(i).copied().unwrap_or(0.0);
                    let v = if nk == 2 { 0.5 * (a + k[1].get(i).copied().unwrap_or(0.0)) } else { a };
                    if v.is_finite() {
                        v
                    } else {
                        0.0
                    }
                }
                None => 0.0,
            };
            self.sig[KEY].ring[pos] = kv;

            // the frame
            self.since += 1;
            if self.since >= self.lay.h {
                self.since = 0;
                self.start_frame(now);
            }
            if self.fr.pending {
                let upto = if self.lay.split { stft::due((now - self.fr.end) as usize, self.fr.total, self.lay.h) } else { self.fr.total };
                self.run_units(upto);
            }

            // Live's control tick
            let live = self.mode == LIVE;
            if live && now.is_multiple_of(self.tick_len as u64) {
                self.live_tick(now);
            }
            if live {
                let ns = if self.nch == 2 { 2 } else { 1 };
                for s in 0..ns {
                    self.slots[s].advance();
                }
            }

            // output
            let d = self.latency_now();
            for c in 0..nch {
                let (dry, wet) = if live {
                    let slot = &self.slots[c];
                    let xv = x[c];
                    let w = if slot.engaged {
                        let y = self.ch[c].casc.process(xv);
                        if slot.m >= 1.0 {
                            y
                        } else {
                            xv + (y - xv) * slot.m
                        }
                    } else {
                        xv
                    };
                    (xv, w)
                } else {
                    let ti = now.wrapping_sub(d as u64) as usize & mask;
                    let ola = &mut self.ch[c].ola;
                    let w = ola[ti];
                    ola[ti] = 0.0;
                    (self.sig[c].ring[ti], w)
                };
                let wet = if mk == 1.0 { wet } else { wet * mk };
                let mut y = if mix >= 1.0 { wet } else { dry + mix * (wet - dry) };
                if dl > 0.0 {
                    y -= dl * dry;
                }
                if outg != 1.0 {
                    y *= outg;
                }
                if byp < 1.0 {
                    y = dry + byp * (y - dry);
                }
                if sg < 1.0 {
                    y *= sg;
                }
                buffers[c][i] = y;
            }
            self.clock += 1;
        }
    }
}

impl Default for SpectralComp {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for SpectralComp {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate;
        self.fam = family(sample_rate);
        self.nch = channels.clamp(1, MAX_CH);
        if self.tables.is_none() {
            self.tables = Some(Box::new(Tables::new()));
        }
        let t = self.tables.as_deref().expect("built");
        self.tmp = vec![C32::default(); t.tmp_len];
        self.scratch = vec![C32::default(); t.scratch_len];
        let nmax = 4096 * self.fam;
        let nbmax = nmax / 2 + 1;
        let ring = (2 * nmax).next_power_of_two();
        self.ring_mask = ring - 1;
        for s in self.sig.iter_mut() {
            s.ring = vec![0.0; ring];
            s.buf = vec![0.0; nmax];
            s.p = vec![0.0; nbmax];
            s.s = vec![0.0; nbmax + 1];
        }
        for c in self.ch.iter_mut() {
            c.x = vec![C32::default(); nbmax];
            c.gout = vec![0.0; nbmax];
            c.gfrom = vec![0.0; nbmax];
            c.glin = vec![1.0; nbmax];
            c.ola = vec![0.0; ring];
        }
        for l in self.lbar.iter_mut() {
            *l = vec![curve::FLOOR_DB; nbmax];
        }
        for s in self.sp.iter_mut().chain(self.sg.iter_mut()) {
            *s = vec![0.0; nbmax + 1];
        }
        self.lay = Layout::new(nbmax);
        let plan = live::plan(sample_rate as f64);
        for s in self.slots.iter_mut() {
            s.prepare(&plan, sample_rate);
        }
        self.plan = Some(Box::new(plan));
        let sr = sample_rate as f64;
        self.tick_len = ((sr * live::TICK_S).round() as u32).max(1) * live::TICKS_PER_SOLVE;
        self.flat_hold = ((live::FLAT_HOLD_S * sr) / self.tick_len as f64).ceil() as u32;
        self.hold_len = (METER_HOLD_S * sample_rate) as u32;
        for s in [&mut self.thresh, &mut self.slope, &mut self.mix, &mut self.out] {
            s.prepare(sample_rate);
        }
        self.link.prepare(sample_rate);
        self.makeup.prepare(sample_rate);
        for r in [&mut self.delta_g, &mut self.bypass_g, &mut self.setup_g] {
            r.prepare(sample_rate);
        }
        self.mode = self.want_mode();
        self.window = self.want_window();
        self.relayout();
    }

    fn reset(&mut self) {
        let v = self.v;
        let c = |i: usize| v[i].clamp(PARAMS[i].min, PARAMS[i].max);
        self.thresh.settle(c(p::THRESH));
        self.thresh.reset();
        self.slope.settle(c(p::SLOPE));
        self.slope.reset();
        self.mix.settle(c(p::MIX));
        self.mix.reset();
        self.out.settle(c(p::OUT));
        self.out.reset();
        self.link.set_immediate(c(p::LINK));
        self.delta_g.set_immediate(if v[p::DELTA] >= 0.5 { 1.0 } else { 0.0 });
        self.bypass_g.set_immediate(if v[p::BYPASS] >= 0.5 { 0.0 } else { 1.0 });
        self.setup_g.set_immediate(1.0);
        self.setup = Setup::Idle;
        self.mode = self.want_mode();
        self.window = self.want_window();
        self.source = self.want_source();
        self.relayout();
        for s in self.sig.iter_mut() {
            s.ring.fill(0.0);
            s.p.fill(0.0);
            s.s.fill(0.0);
        }
        self.clear_engine();
        self.instant_frames = HOP_DIV;
        self.clock = 0;
        self.glide_step(true);
        self.makeup_now();
        self.makeup.set_immediate(self.makeup_want());
        self.acc = [[0.0; 4]; 8];
        self.m_dyn = [0.0; 8];
        self.m_dyn_age = [0; 8];
        self.m_gr = 0.0;
        self.m_gr_age = 0;
        self.m_lift = 0.0;
        self.m_lift_age = 0;
    }

    fn set_param(&mut self, id: &str, value: f32) {
        let Some(i) = PARAMS.iter().position(|pi| pi.id == id) else {
            return;
        };
        let old = self.v[i];
        self.v[i] = value;
        let pi = &PARAMS[i];
        let x = value.clamp(pi.min, pi.max);
        match i {
            p::THRESH if value != old => self.thresh.set_target(x, self.clock),
            p::SLOPE if value != old => self.slope.set_target(x, self.clock),
            p::MIX if value != old => self.mix.set_target(x, self.clock),
            p::OUT if value != old => self.out.set_target(x, self.clock),
            p::LINK => self.link.set_target(x),
            p::DELTA => self.delta_g.set_target(if x >= 0.5 { 1.0 } else { 0.0 }),
            p::BYPASS => self.bypass_g.set_target(if x >= 0.5 { 0.0 } else { 1.0 }),
            p::MODE | p::WINDOW => {
                let studio_window = i == p::WINDOW && self.mode == STUDIO && self.want_window() != self.window;
                let mode_change = i == p::MODE && self.want_mode() != self.mode;
                if (studio_window || mode_change) && !matches!(self.setup, Setup::Out | Setup::Wait) {
                    self.setup = Setup::Out;
                    self.setup_g.set_target(0.0);
                }
            }
            _ => {}
        }
    }

    fn meter(&self, id: &str) -> Option<f32> {
        match id {
            "gr" => Some(self.m_gr),
            "lift_gr" => Some(self.m_lift),
            _ => METERS[2..].iter().position(|m| *m == id).map(|k| self.m_dyn[k]),
        }
    }

    fn latency(&self) -> usize {
        self.latency_now()
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_with_key(buffers, None);
    }

    fn process_with_key(&mut self, buffers: &mut [&mut [f32]], key: Option<&[&[f32]]>) {
        SpectralComp::process_with_key(self, buffers, key);
    }
}
