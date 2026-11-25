// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-stretch — golem's time stretcher (design: README.md).
//!
//! - **Memory** (memory.rs): 2 × 60.25 s per channel in f16, as two takes (Capture) or one ring
//!   (Rolling); a 250 ms pre-roll ring always records, for the loop seam.
//! - **Heads**: at most two read heads, each at 1 / `amount` of real time, crossfaded in power (random
//!   mode) or as complex spectra (coherent mode) on the frame grid.
//!   - Capture: `capture` on records into the idle take, off commits it (the seam crossfaded with the
//!     pre-roll), and a new head plays it looped while the old one fades out over `xfade`.
//!   - Rolling: the head drifts behind the present and resyncs to it when the lag reaches `maxlen`.
//!   - Freeze: the heads stop and their last analysis is held; new phases on every hop.
//! - **Engine** (engine.rs, on gfx-dsp `stft`): Hann / Hann at hop N / 4, N = 256 – 2^18 (`fft` in ms,
//!   half-octave grid), every pass cut into units spread over the hop on sample counts; a size change
//!   runs a second engine and crossfades.
//! - **Wet**: low cut, tone, `vol`, a soft limiter. The dry (`thru`) is never delayed; latency 0.
//!
//! Real-time safe, deterministic.

#![allow(clippy::needless_range_loop)]

pub mod engine;
pub mod memory;

#[cfg(feature = "plugin")]
pub mod plugin;

#[cfg(test)]
mod tests;

use engine::{Engine, FrameSet, HeadSnap, HEADS};
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::smooth::OnePole;
use gfx_dsp::stft::{self, Phasors, SizeFade, Tables};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::{Effect, ParamInfo};
use memory::{Pool, Sample, Src};

pub const NP: usize = 21;

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::float("amount", "Stretch amount", 0.1, 1024.0, 4.0).with_exp(),
    ParamInfo::float("fft", "FFT size", 5.0, 5500.0, 683.0),
    ParamInfo::float("spread", "Frequency spread", 0.0, 2400.0, 0.0).with_exp(),
    ParamInfo::float("vol", "Main volume", -24.0, 12.0, -3.0).with_exp(),
    ParamInfo::enumeration("mode", "Mode", 2, 1),
    ParamInfo::boolean("capture", "Capture", false),
    ParamInfo::boolean("freeze", "Freeze", false),
    ParamInfo::boolean("thru", "Pass input through", true),
    ParamInfo::float("pitch", "Pitch shift", -24.0, 24.0, 0.0).with_exp(),
    ParamInfo::float("fshift", "Frequency shift", -1000.0, 1000.0, 0.0).with_exp(),
    ParamInfo::float("compress", "Compress", 0.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("maxlen", "Max capture length", 1.0, 60.0, 10.0),
    ParamInfo::float("cloud", "Cloud", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::float("width", "Width", 0.0, 100.0, 100.0),
    ParamInfo::float("xfade", "Crossfade", 50.0, 10000.0, 2000.0),
    ParamInfo::float("hpf", "Low Cut", 20.0, 500.0, 20.0),
    ParamInfo::float("lpf", "Tone", 1000.0, 20000.0, 20000.0).with_exp(),
    ParamInfo::boolean("spread_on", "Enable spread", true),
    ParamInfo::boolean("comp_on", "Enable compressor", true),
    ParamInfo::boolean("trails", "Trails", true),
    ParamInfo::boolean("bypass", "Bypass", false),
];

pub mod p {
    pub const AMOUNT: usize = 0;
    pub const FFT: usize = 1;
    pub const SPREAD: usize = 2;
    pub const VOL: usize = 3;
    pub const MODE: usize = 4;
    pub const CAPTURE: usize = 5;
    pub const FREEZE: usize = 6;
    pub const THRU: usize = 7;
    pub const PITCH: usize = 8;
    pub const FSHIFT: usize = 9;
    pub const COMPRESS: usize = 10;
    pub const MAXLEN: usize = 11;
    pub const CLOUD: usize = 12;
    pub const WIDTH: usize = 13;
    pub const XFADE: usize = 14;
    pub const HPF: usize = 15;
    pub const LPF: usize = 16;
    pub const SPREAD_ON: usize = 17;
    pub const COMP_ON: usize = 18;
    pub const TRAILS: usize = 19;
    pub const BYPASS: usize = 20;
}

/// `mode` labels (enum order, final).
pub const MODES: [&str; 2] = ["Capture", "Rolling"];
pub const CAPTURE: usize = 0;
pub const ROLLING: usize = 1;

/// Read-only meters: `state` 0 Empty · 1 Capturing · 2 Playing · 3 Frozen · 4 Rolling.
pub const METERS: [&str; 1] = ["state"];

/// golem's conversion of a saved `stretch` chain (README §3.4), written at 48 kHz: PaulXStretch's
/// `spread` (0 – 100 %) to the blur's σ in cents. Its four one-poles on amplitudes make a kernel of
/// σ = 2,400 · log₂(fs / 40) / (16,384 · (−ln(1 − 2^(−10 n²)))) cents; a partial's power profile is
/// that kernel squared, 0.59 σ wide, and ours blurs the power itself.
pub fn spread_from_catalogue(pct: f64) -> f64 {
    let n = pct / 100.0;
    if n <= 0.0 {
        return 0.0;
    }
    let a = 1.0 - (-10.0 * n * n).exp2();
    0.59 * 2400.0 * (48000.0f64 / 40.0).log2() / (16384.0 * -a.ln())
}

/// golem's conversion of `fft` (0 – 100 %) to the window in ms: B = the 2^a 3^b 5^c 7^d 11^e
/// nearest to ⌊2^(7 + 10 n)⌋ (PaulXStretch's FFTW sizes), window 2B at 48 kHz.
pub fn fft_from_catalogue(pct: f64) -> f64 {
    let x = (7.0 + 10.0 * pct / 100.0).exp2().floor();
    let mut best = 1.0f64;
    let mut a = 1.0f64;
    while a <= 2.0 * x {
        let mut b = a;
        while b <= 2.0 * x {
            let mut c = b;
            while c <= 2.0 * x {
                let mut d = c;
                while d <= 2.0 * x {
                    let mut e = d;
                    while e <= 2.0 * x {
                        if (e - x).abs() < (best - x).abs() {
                            best = e;
                        }
                        e *= 11.0;
                    }
                    d *= 7.0;
                }
                c *= 5.0;
            }
            b *= 3.0;
        }
        a *= 2.0;
    }
    2000.0 * best / 48000.0
}

#[inline]
fn db(x: f32) -> f32 {
    10f32.powf(x / 20.0)
}

/// A read head. Its weight is a raised-cosine ramp (power domain), latched per frame.
#[derive(Clone, Copy, Debug, Default)]
struct Head {
    active: bool,
    gen: u32,
    p: f64,
    slot: usize,
    len: usize,
    /// its take records: it stops reading and holds its spectrum (README §2.1)
    hold: bool,
    from: f32,
    to: f32,
    pos: u32,
    flen: u32,
}

impl Head {
    fn weight(&self) -> f32 {
        if self.pos >= self.flen {
            self.to
        } else {
            let x = self.pos as f32 / self.flen as f32;
            self.from + (self.to - self.from) * (0.5 - 0.5 * (std::f32::consts::PI * x).cos())
        }
    }
    fn fade_to(&mut self, to: f32, len: u32) {
        self.from = self.weight();
        self.to = to;
        self.pos = 0;
        self.flen = len.max(1);
    }
    fn fading(&self) -> bool {
        self.pos < self.flen
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Take {
    len: usize,
    mono: bool,
}

#[derive(Clone, Copy, Debug)]
struct Rec {
    slot: usize,
    len: usize,
    max: usize,
    mono: bool,
}

/// A committed take whose loop seam is being crossfaded, [`SEAL_UNIT`] samples per sample (a 250 ms
/// seam is done in 8 ms at any rate, never one spike); its head starts when it is done.
#[derive(Clone, Copy, Debug)]
struct Seal {
    slot: usize,
    len: usize,
    s: usize,
    i: usize,
    mono: bool,
}

const SEAL_UNIT: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq)]
enum ModePhase {
    Idle,
    /// fading the wet out before the switch (samples left)
    Out(u32),
    /// the engines' rings draining after it
    Drain(u32),
    In,
}

/// The processor, generic over the pool's sample: [`Stretch`] (f16, shipped); the tests also run it
/// in f32.
pub struct StretchCore<S: Sample> {
    v: [f32; NP],
    sr: f32,
    nch: usize,
    tables: Option<Box<Tables>>,
    phasors: Phasors,
    eng: [Engine; 2],
    cur: usize,
    sfade: SizeFade,
    pool: Pool<S>,
    /// the pre-roll ring: 2 × 250 ms (the 250 ms before a take stay intact for 250 ms after it)
    pre: Vec<Vec<f32>>,
    pre_len: usize,
    /// 250 ms
    pre_roll: usize,
    pre_w: u64,
    /// the pre-roll before the take, copied aside one sample per recorded sample
    seam: Vec<Vec<f32>>,
    seam_q0: u64,
    seam_avail: usize,
    seam_saved: usize,
    seal: Option<Seal>,
    mode: usize,
    mode_target: usize,
    mode_phase: ModePhase,
    mode_gain: f32,
    w: u64,
    last_diff: u64,
    takes: [Take; 2],
    rec: Option<Rec>,
    capture_prev: bool,
    heads: [Head; HEADS],
    gen_next: u32,
    bypassed: bool,
    gate: f32,
    gate_target: f32,
    gate_step: f32,
    thru: OnePole,
    byp: OnePole,
    hpf: OnePole,
    vol: ExpSmoother,
    lpf: ExpSmoother,
    hp: [[Svf; 2]; 2],
    lp: [[Svf; 2]; 2],
    filt_count: u32,
    guard: f32,
    now: u64,
    state: f32,
    /// tests: the engines' output alone (no filters, `vol`, limiter or dry)
    #[cfg(test)]
    pub(crate) raw_wet: bool,
}

/// The shipped processor: f16 takes.
pub type Stretch = StretchCore<half::f16>;

impl<S: Sample> Default for StretchCore<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Sample> StretchCore<S> {
    pub fn new() -> Self {
        let mut v = [0.0; NP];
        for (i, p) in PARAMS.iter().enumerate() {
            v[i] = p.default;
        }
        Self {
            v,
            sr: 48000.0,
            nch: 1,
            tables: None,
            phasors: Phasors::new(),
            eng: [Engine::new(), Engine::new()],
            cur: 0,
            sfade: SizeFade::default(),
            pool: Pool::new(),
            pre: Vec::new(),
            pre_len: 2,
            pre_roll: 1,
            pre_w: 0,
            seam: Vec::new(),
            seam_q0: 0,
            seam_avail: 0,
            seam_saved: 0,
            seal: None,
            mode: ROLLING,
            mode_target: ROLLING,
            mode_phase: ModePhase::Idle,
            mode_gain: 1.0,
            w: 0,
            last_diff: 0,
            takes: [Take::default(); 2],
            rec: None,
            capture_prev: false,
            heads: [Head::default(); HEADS],
            gen_next: 0,
            bypassed: false,
            gate: 1.0,
            gate_target: 1.0,
            gate_step: 0.0,
            thru: OnePole::new(20.0, 1.0, 1e-5),
            byp: OnePole::new(20.0, 0.0, 1e-5),
            hpf: OnePole::new(30.0, 20.0, 0.01),
            vol: ExpSmoother::new(-24.0, 12.0, -3.0),
            lpf: ExpSmoother::new(1000.0, 20000.0, 20000.0),
            hp: [[Svf::new(), Svf::new()], [Svf::new(), Svf::new()]],
            lp: [[Svf::new(), Svf::new()], [Svf::new(), Svf::new()]],
            filt_count: 0,
            guard: 1.0,
            now: 0,
            state: 0.0,
            #[cfg(test)]
            raw_wet: false,
        }
    }

    /// The window size the `fft` setting asks for (grid index).
    fn target_si(&self) -> usize {
        stft::nearest_size(self.v[p::FFT] as f64 * 0.001 * self.sr as f64)
    }

    /// Bytes held (pool, engines, tables, rings): the README's memory figure.
    pub fn memory_bytes(&self) -> usize {
        self.pool.bytes()
            + self.eng.iter().map(|e| e.bytes()).sum::<usize>()
            + self.tables.as_ref().map_or(0, |t| t.bytes())
            + (self.pre.iter().map(|x| x.len()).sum::<usize>() + self.seam.iter().map(|x| x.len()).sum::<usize>()) * 4
    }

    /// The `state` meter.
    pub fn state(&self) -> f32 {
        self.state
    }

    /// The current engine's window size and hop (samples).
    pub fn window(&self) -> (usize, usize) {
        (self.eng[self.cur].n, self.eng[self.cur].hop)
    }

    /// Active heads (the "never more than two" check).
    pub fn active_heads(&self) -> usize {
        self.heads.iter().filter(|h| h.active).count()
    }

    /// The newest head's position (samples) and the Rolling write count.
    pub fn head_pos(&self) -> Option<(f64, u64)> {
        self.main().map(|i| (self.heads[i].p, self.w))
    }

    /// The committed takes' lengths.
    pub fn take_lens(&self) -> [usize; 2] {
        [self.takes[0].len, self.takes[1].len]
    }

    /// Whether a size crossfade runs.
    pub fn size_fading(&self) -> bool {
        self.sfade.on
    }

    /// Units of the current engine's frame (CPU report).
    pub fn frame_units(&self) -> usize {
        self.eng[self.cur].frame_units()
    }

    fn set_filters(&mut self) {
        let lpf = self.lpf.value().clamp(200.0, 20000.0);
        let hpf = self.hpf.value();
        for c in 0..2 {
            // 4th order: two 2-pole sections (Butterworth Qs)
            self.hp[c][0].set(SvfMode::HighPass, hpf, 0.541_196, 0.0, self.sr);
            self.hp[c][1].set(SvfMode::HighPass, hpf, 1.306_563, 0.0, self.sr);
            self.lp[c][0].set(SvfMode::LowPass, lpf.min(self.sr * 0.45), std::f32::consts::FRAC_1_SQRT_2, 0.0, self.sr);
            self.lp[c][1].set(SvfMode::LowPass, lpf.min(self.sr * 0.45), std::f32::consts::FRAC_1_SQRT_2, 0.0, self.sr);
        }
    }

    /// The newest active head.
    fn main(&self) -> Option<usize> {
        (0..HEADS).filter(|&i| self.heads[i].active).max_by_key(|&i| self.heads[i].gen)
    }

    /// Starts a head (dropping the oldest if both are busy: never more than two).
    fn spawn(&mut self, p: f64, slot: usize, len: usize, from: f32, flen: u32) -> usize {
        let i = match (0..HEADS).find(|&i| !self.heads[i].active) {
            Some(i) => i,
            None => (0..HEADS).min_by_key(|&i| self.heads[i].gen).unwrap_or(0),
        };
        self.gen_next = self.gen_next.wrapping_add(1);
        self.heads[i] = Head { active: true, gen: self.gen_next, p, slot, len, hold: false, from, to: 1.0, pos: 0, flen: flen.max(1) };
        if from >= 1.0 {
            self.heads[i].pos = self.heads[i].flen;
        }
        i
    }

    /// Minimum lag in Rolling: half the largest window running.
    fn l0(&self) -> f64 {
        let mut n = self.eng[self.cur].n;
        if self.sfade.on {
            n = n.max(self.eng[1 - self.cur].n);
        }
        (n / 2) as f64
    }

    fn xfade_samples(&self) -> u32 {
        (self.v[p::XFADE] * 0.001 * self.sr) as u32
    }

    /// `capture` off → on: recording into the idle take (the pre-roll is copied aside as it goes).
    fn start_rec(&mut self) {
        if self.rec.is_some() {
            return;
        }
        while self.seal.is_some() {
            self.seal_step(usize::MAX);
        }
        let idle = match self.main() {
            Some(i) => 1 - self.heads[i].slot,
            None => 0,
        };
        for h in &mut self.heads {
            if h.active && h.slot == idle {
                h.hold = true;
            }
        }
        self.seam_q0 = self.pre_w;
        self.seam_avail = (self.pre_w as usize).min(self.pre_roll);
        self.seam_saved = 0;
        let max = ((self.v[p::MAXLEN].clamp(1.0, 60.0) * self.sr) as usize).min(self.pool.half);
        self.takes[idle] = Take::default();
        self.rec = Some(Rec { slot: idle, len: 0, max, mono: true });
    }

    /// `capture` on → off or `maxlen` reached: the seam, then a new head on the take.
    fn commit(&mut self) {
        let Some(r) = self.rec.take() else { return };
        let lt = r.len;
        if (lt as f32) < 0.05 * self.sr {
            return; // a bounce
        }
        let s = self.pre_roll.min(lt / 4);
        self.seal = Some(Seal { slot: r.slot, len: lt, s, i: 0, mono: r.mono });
        self.seal_step(SEAL_UNIT);
    }

    /// The pre-roll sample `j` (0 = the oldest of the 250 ms before the take).
    #[inline]
    fn pre_at(&self, c: usize, j: usize) -> f32 {
        if j < self.seam_saved {
            self.seam[c][j]
        } else {
            let a = self.seam_q0 as usize + self.pre_len - self.seam_avail + j;
            self.pre[c][a % self.pre_len]
        }
    }

    /// Up to `count` samples of the seam: the take's last S samples crossfaded (equal power) into
    /// the S pre-roll samples before its start, so reading on from its end into its start continues
    /// as the input did. Then the take is committed and its head starts.
    fn seal_step(&mut self, count: usize) {
        let Some(mut sl) = self.seal else { return };
        let end = sl.s.min(sl.i.saturating_add(count));
        for i in sl.i..end {
            let th = (i as f32 + 0.5) / sl.s as f32 * std::f32::consts::FRAC_PI_2;
            let (sn, cs) = th.sin_cos();
            let pj = self.seam_avail as isize - sl.s as isize + i as isize;
            let mut vals = [0.0f32; 2];
            for c in 0..self.nch {
                let pre = if pj >= 0 { self.pre_at(c, pj as usize) } else { 0.0 };
                let at = sl.len - sl.s + i;
                let tk = self.pool.read_take(c, sl.slot, at);
                self.pool.write_take(c, sl.slot, at, tk * cs + pre * sn);
                vals[c] = self.pool.read_take(c, sl.slot, at);
            }
            if self.nch == 2 && vals[0] != vals[1] {
                sl.mono = false;
            }
        }
        sl.i = end;
        if sl.i < sl.s {
            self.seal = Some(sl);
            return;
        }
        self.seal = None;
        self.takes[sl.slot] = Take { len: sl.len, mono: sl.mono };
        let xf = self.xfade_samples();
        if let Some(o) = self.main() {
            // the other (older) head, if any, is dropped: never more than two
            for i in 0..HEADS {
                if i != o {
                    self.heads[i].active = false;
                }
            }
            self.heads[o].fade_to(0.0, xf);
        }
        self.spawn(0.0, sl.slot, sl.len, 0.0, xf);
    }

    fn switch_mode(&mut self) {
        self.mode = self.mode_target;
        self.heads = [Head::default(); HEADS];
        self.takes = [Take::default(); 2];
        self.rec = None;
        self.seal = None;
        self.w = 0;
        self.last_diff = 0;
        if self.mode == ROLLING {
            let l0 = self.l0();
            self.spawn(-l0, 0, 0, 1.0, 1);
        }
    }

    fn set_gate(&mut self, target: f32, samples: f32) {
        self.gate_target = target;
        self.gate_step = 1.0 / samples.max(1.0);
    }

    fn frame_set(&self, e: usize) -> FrameSet {
        let v = &self.v;
        let n = self.eng[e].n;
        let cloud = v[p::CLOUD] * 0.01;
        let coherent = cloud < 0.9995;
        let freeze = v[p::FREEZE] > 0.5;
        let mut heads = [HeadSnap::default(); HEADS];
        for s in 0..HEADS {
            let h = &self.heads[s];
            if !h.active {
                continue;
            }
            let src = if self.mode == ROLLING { Src::Ring { w: self.w } } else { Src::Take { slot: h.slot, len: h.len } };
            let start = (h.p - (n / 2) as f64 + 0.5).floor() as i64 - if coherent { (n / 16) as i64 } else { 0 };
            let mono = self.nch == 2 && if self.mode == ROLLING { start >= self.last_diff as i64 } else { self.takes[h.slot].mono };
            heads[s] = HeadSnap { active: true, gen: h.gen, pos: h.p, src, weight: h.weight(), frozen: freeze || h.hold, mono };
        }
        FrameSet {
            coherent,
            cloud: if coherent { cloud } else { 1.0 },
            width: v[p::WIDTH] * 0.01,
            ratio: 2f32.powf(v[p::PITCH] / 12.0),
            shift_hz: v[p::FSHIFT],
            sigma: if v[p::SPREAD_ON] > 0.5 { v[p::SPREAD] } else { 0.0 },
            comp: if v[p::COMP_ON] > 0.5 { v[p::COMPRESS] * 0.01 } else { 0.0 },
            identity: v[p::AMOUNT] == 1.0 && v[p::PITCH] == 0.0 && v[p::FSHIFT] == 0.0,
            heads,
        }
    }

    #[inline]
    fn tick_engine(&mut self, e: usize, y: &mut [f32; 2]) {
        if self.eng[e].frame_due() {
            let set = self.frame_set(e);
            self.eng[e].start_frame(set);
        }
        let t = self.tables.as_deref().expect("prepared");
        self.eng[e].tick(&self.pool, t, &self.phasors, y);
    }

    fn reset_heads_and_memory(&mut self) {
        self.heads = [Head::default(); HEADS];
        self.takes = [Take::default(); 2];
        self.rec = None;
        self.w = 0;
        self.last_diff = 0;
        self.pre_w = 0;
        self.seam_saved = 0;
        self.seam_avail = 0;
        self.seal = None;
        self.gen_next = 0;
    }
}

impl<S: Sample> Effect for StretchCore<S> {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate;
        self.nch = channels.clamp(1, 2);
        if self.tables.is_none() {
            self.tables = Some(Box::new(Tables::new()));
        }
        self.pool.prepare(sample_rate, self.nch);
        let t = self.tables.as_deref().expect("built");
        for e in &mut self.eng {
            e.prepare(sample_rate, self.nch, t);
        }
        self.pre_roll = ((0.25 * sample_rate) as usize).max(1);
        self.pre_len = 2 * self.pre_roll;
        self.pre = (0..self.nch).map(|_| memory::touched(self.pre_len, 0.0)).collect();
        self.seam = (0..self.nch).map(|_| memory::touched(self.pre_roll, 0.0)).collect();
        for s in [&mut self.thru, &mut self.byp, &mut self.hpf] {
            s.prepare(sample_rate);
        }
        self.vol.prepare(sample_rate);
        self.lpf.prepare(sample_rate);
        self.reset();
    }

    fn reset(&mut self) {
        let v = self.v;
        self.thru.set_target(if v[p::THRU] > 0.5 { 1.0 } else { 0.0 });
        self.byp.set_target(if v[p::BYPASS] > 0.5 { 1.0 } else { 0.0 });
        self.hpf.set_target(v[p::HPF]);
        for s in [&mut self.thru, &mut self.byp, &mut self.hpf] {
            s.settle();
        }
        self.vol.settle(v[p::VOL]);
        self.vol.reset();
        self.lpf.settle(v[p::LPF]);
        self.lpf.reset();
        if self.tables.is_some() {
            let si = self.target_si();
            let t = self.tables.as_deref().expect("built");
            for e in &mut self.eng {
                e.clear_all();
            }
            self.eng[0].configure(si, t);
        }
        self.cur = 0;
        self.sfade = SizeFade::default();
        self.reset_heads_and_memory();
        self.mode = v[p::MODE].round().clamp(0.0, 1.0) as usize;
        self.mode_target = self.mode;
        self.mode_phase = ModePhase::Idle;
        self.mode_gain = 1.0;
        if self.mode == ROLLING {
            let l0 = self.l0();
            self.spawn(-l0, 0, 0, 1.0, 1);
        }
        // `capture` as restored is the baseline: no edge at load
        self.capture_prev = v[p::CAPTURE] > 0.5;
        self.bypassed = v[p::BYPASS] > 0.5;
        self.gate = if self.bypassed { 0.0 } else { 1.0 };
        self.gate_target = self.gate;
        self.gate_step = 0.0;
        for c in 0..2 {
            for f in self.hp[c].iter_mut().chain(self.lp[c].iter_mut()) {
                f.reset();
            }
        }
        self.set_filters();
        self.filt_count = 0;
        self.guard = 1.0;
        self.now = 0;
        self.state = if self.mode == ROLLING { 4.0 } else { 0.0 };
    }

    fn set_param(&mut self, id: &str, value: f32) {
        if let Some(i) = PARAMS.iter().position(|p| p.id == id) {
            let pi = &PARAMS[i];
            let v = value.clamp(pi.min, pi.max);
            self.v[i] = v;
            match i {
                p::VOL => self.vol.set_target(v, self.now),
                p::LPF => self.lpf.set_target(v, self.now),
                p::HPF => self.hpf.set_target(v),
                p::THRU => self.thru.set_target(if v > 0.5 { 1.0 } else { 0.0 }),
                p::BYPASS => self.byp.set_target(if v > 0.5 { 1.0 } else { 0.0 }),
                _ => {}
            }
        }
    }

    fn meter(&self, id: &str) -> Option<f32> {
        match id {
            "state" => Some(self.state),
            _ => None,
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(self.nch);
        if nch == 0 || self.tables.is_none() {
            return;
        }
        let n = buffers[0].len();
        let sr = self.sr;
        let v = self.v;
        let inc = 1.0 / v[p::AMOUNT].max(0.1) as f64;
        let amount = v[p::AMOUNT].max(0.1) as f64;
        let freeze = v[p::FREEZE] > 0.5;
        let trails = v[p::TRAILS] > 0.5;

        // ---- block-level events: mode, bypass, capture
        let want_mode = v[p::MODE].round().clamp(0.0, 1.0) as usize;
        if want_mode != self.mode && self.mode_phase == ModePhase::Idle {
            self.mode_target = want_mode;
            self.mode_phase = ModePhase::Out((0.05 * sr) as u32);
        }
        let byp = v[p::BYPASS] > 0.5;
        if byp != self.bypassed {
            self.bypassed = byp;
            if byp {
                self.commit();
                let len = if trails { (v[p::XFADE] * 0.001 * sr).max(self.eng[self.cur].n as f32) } else { 0.02 * sr };
                self.set_gate(0.0, len);
            } else {
                self.set_gate(1.0, 0.02 * sr);
                if self.mode == CAPTURE {
                    if let Some(m) = self.main() {
                        // the take is still there: it fades back in from its start
                        let (slot, len) = (self.heads[m].slot, self.heads[m].len);
                        self.heads = [Head::default(); HEADS];
                        let xf = self.xfade_samples();
                        self.spawn(0.0, slot, len, 0.0, xf);
                    }
                }
            }
        }
        let cap = v[p::CAPTURE] > 0.5;
        if cap != self.capture_prev {
            self.capture_prev = cap;
            if self.mode == CAPTURE && !self.bypassed && self.mode_phase == ModePhase::Idle {
                if cap {
                    self.start_rec();
                } else {
                    self.commit();
                }
            }
        }
        let target_si = self.target_si();
        let xf = self.xfade_samples();
        let maxlen = v[p::MAXLEN] as f64 * sr as f64;
        let raw = {
            #[cfg(test)]
            {
                self.raw_wet
            }
            #[cfg(not(test))]
            {
                false
            }
        };

        for i in 0..n {
            self.now += 1;
            let mut x = [0.0f32; 2];
            for c in 0..nch {
                let s = buffers[c][i];
                x[c] = if s.is_finite() { s.clamp(-60000.0, 60000.0) } else { 0.0 };
            }
            if nch == 1 {
                x[1] = x[0];
            }

            // ---- memory
            let record = !self.bypassed && matches!(self.mode_phase, ModePhase::Idle | ModePhase::In);
            if self.seal.is_some() {
                self.seal_step(SEAL_UNIT);
            }
            if record {
                if self.rec.is_some() && self.seam_saved < self.seam_avail {
                    // one pre-roll sample aside per recorded sample, oldest first: all are safe
                    // after 250 ms, and the rest stay intact in the ring until then
                    let j = self.seam_saved;
                    for c in 0..nch {
                        self.seam[c][j] = self.pre_at(c, j);
                    }
                    self.seam_saved += 1;
                }
                let q = (self.pre_w % self.pre_len as u64) as usize;
                for c in 0..nch {
                    self.pre[c][q] = x[c];
                }
                self.pre_w += 1;
                if self.mode == ROLLING {
                    for c in 0..nch {
                        self.pool.write_ring(c, self.w, x[c]);
                    }
                    if nch == 2 && x[0] != x[1] {
                        self.last_diff = self.w + 1;
                    }
                    self.w += 1;
                } else if let Some(r) = self.rec.as_mut() {
                    for c in 0..nch {
                        self.pool.write_take(c, r.slot, r.len, x[c]);
                    }
                    if nch == 2 && x[0] != x[1] {
                        r.mono = false;
                    }
                    r.len += 1;
                    if r.len >= r.max {
                        // auto-commit at `maxlen`; the next take needs a new off → on
                        self.commit();
                    }
                }
            }

            // ---- heads
            let l0 = self.l0();
            let wf = self.w as f64;
            for h in &mut self.heads {
                if !h.active {
                    continue;
                }
                if !(freeze || h.hold) {
                    h.p += inc;
                    if self.mode == CAPTURE {
                        if h.len > 0 {
                            h.p = h.p.rem_euclid(h.len as f64);
                        }
                    } else {
                        h.p = h.p.min(wf - l0);
                    }
                }
                if h.fading() {
                    h.pos += 1;
                } else if h.to <= 0.0 {
                    h.active = false;
                }
            }
            if self.mode == ROLLING && !freeze && record {
                if let Some(m) = self.main() {
                    let others = self.heads.iter().filter(|h| h.active).count();
                    let limit = maxlen.max(2.0 * self.eng[self.cur].n as f64);
                    if others == 1 && wf - self.heads[m].p > limit {
                        // resync: a new head at the present, the old one fades out (≤ half the period)
                        let half = if amount > 1.0 { 0.5 * limit * amount / (amount - 1.0) } else { f64::INFINITY };
                        let f = (xf as f64).min(half).max(1.0) as u32;
                        self.heads[m].fade_to(0.0, f);
                        self.spawn(wf - l0, 0, 0, 0.0, f);
                    }
                }
            }

            // ---- engines
            let mut y = [0.0f32; 2];
            let cur = self.cur;
            self.tick_engine(cur, &mut y);
            if self.sfade.on {
                let o = 1 - cur;
                let mut y2 = [0.0f32; 2];
                self.tick_engine(o, &mut y2);
                let (go, gn, done) = self.sfade.next();
                for c in 0..2 {
                    y[c] = y[c] * go + y2[c] * gn;
                }
                if done {
                    self.eng[cur].deactivate();
                    self.cur = o;
                }
            } else if target_si != self.eng[cur].si && self.eng[1 - cur].is_clean() {
                // a size change: a second engine on the same heads, crossfaded once its frames overlap
                let o = 1 - cur;
                let t = self.tables.as_deref().expect("prepared");
                self.eng[o].configure(target_si, t);
                let (no, ho) = (self.eng[o].n, self.eng[o].hop);
                self.sfade.start(no + ho, ((0.05 * sr) as usize).max(no));
            }
            if (self.now & 31) == 0 {
                let o = 1 - self.cur;
                if !self.eng[o].active {
                    self.eng[o].clear_step();
                }
            }

            // ---- mode switch
            match self.mode_phase {
                ModePhase::Idle => {}
                ModePhase::Out(k) => {
                    let len = (0.05 * sr).max(1.0);
                    self.mode_gain = k as f32 / len;
                    if k == 0 {
                        self.switch_mode();
                        let d = self.eng[self.cur].latency().max(if self.sfade.on { self.eng[1 - self.cur].latency() } else { 0 });
                        self.mode_phase = ModePhase::Drain(d as u32);
                    } else {
                        self.mode_phase = ModePhase::Out(k - 1);
                    }
                }
                ModePhase::Drain(k) => {
                    self.mode_gain = 0.0;
                    self.mode_phase = if k == 0 { ModePhase::In } else { ModePhase::Drain(k - 1) };
                }
                ModePhase::In => {
                    self.mode_gain = (self.mode_gain + 1.0 / (0.05 * sr)).min(1.0);
                    if self.mode_gain >= 1.0 {
                        self.mode_phase = ModePhase::Idle;
                    }
                }
            }

            // ---- wet chain and dry
            let vol_db = self.vol.next();
            self.lpf.next();
            self.hpf.next();
            self.filt_count += 1;
            if self.filt_count >= 16 {
                self.filt_count = 0;
                if !self.lpf.is_settled() || !self.hpf.is_settled() {
                    self.set_filters();
                }
            }
            if self.gate != self.gate_target {
                self.gate = if self.gate < self.gate_target { (self.gate + self.gate_step).min(self.gate_target) } else { (self.gate - self.gate_step).max(self.gate_target) };
            }
            let thru = self.thru.next();
            let b = self.byp.next();
            let dry_g = thru + (1.0 - thru) * b;
            let wg = self.gate * self.mode_gain * self.guard;
            let wet_g = db(vol_db);
            let mut out = [0.0f32; 2];
            let mut bad = false;
            for c in 0..nch {
                if raw {
                    out[c] = y[c];
                    continue;
                }
                let mut w = y[c];
                w = self.hp[c][0].process(w);
                w = self.hp[c][1].process(w);
                w = self.lp[c][0].process(w);
                w = self.lp[c][1].process(w);
                w *= wet_g;
                // soft limiter: tanh knee from −6 dBFS, ceiling −1 dBFS
                let aw = w.abs();
                if aw > 0.5 {
                    let ceil = 0.891_25;
                    w = w.signum() * (0.5 + (ceil - 0.5) * ((aw - 0.5) / (ceil - 0.5)).tanh());
                }
                let o = x[c] * dry_g + w * wg;
                if !o.is_finite() {
                    bad = true;
                }
                out[c] = o;
            }
            if bad {
                // guard: the engines start again, the wet fades back in over 10 ms
                let si = self.eng[self.cur].si;
                let t = self.tables.as_deref().expect("prepared");
                for e in &mut self.eng {
                    e.clear_all();
                }
                self.eng[0].configure(si, t);
                self.cur = 0;
                self.sfade = SizeFade::default();
                for c in 0..2 {
                    for f in self.hp[c].iter_mut().chain(self.lp[c].iter_mut()) {
                        f.reset();
                    }
                }
                self.guard = 0.0;
                for c in 0..nch {
                    buffers[c][i] = 0.0;
                }
                continue;
            }
            if self.guard < 1.0 {
                self.guard = (self.guard + 1.0 / (0.01 * sr)).min(1.0);
            }
            for c in 0..nch {
                buffers[c][i] = out[c];
            }
        }

        // ---- the `state` meter
        self.state = if self.mode == ROLLING {
            if freeze {
                3.0
            } else {
                4.0
            }
        } else if self.rec.is_some() || self.seal.is_some() {
            1.0
        } else if self.main().is_some() {
            if freeze {
                3.0
            } else {
                2.0
            }
        } else {
            0.0
        };
    }
}
