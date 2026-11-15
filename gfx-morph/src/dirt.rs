// SPDX-License-Identifier: GPL-3.0-or-later
//! The dirt (README §2.4 – §2.6): one oversampled engine, up 2× / 4× once (1× / 2× at ≥ 88.2 kHz), the
//! split (Single · Two · Three, gfx-dsp `crossover::Tree`, LR4 with its compensation all-passes), per
//! band two parallel types A and B blended by the morph with the live ρ gain, the band's DC block, its
//! aligned dry and the per-band mix and level, the band sum, the droop correction, down once.
//!
//! A slot whose weight is zero is not computed; its cheap running states (Ring's and Decimate's phase,
//! Starve's and Rasp's envelopes) move on, and it wakes from rest. An engine has a fixed configuration
//! (factor, bands): `split` and `quality` changes run a second engine beside the first (lib.rs).

use crate::types::{Half, Inner, SlotParams, SlotState, DROOP_A};
use gfx_dsp::crossover::{Order, Tree};
use gfx_dsp::oversample::Oversampler2x;
use gfx_dsp::svf::{Svf, SvfMode};

/// Bands: low, body, air. Single runs body only; Two low and body; Three all.
pub const BANDS: usize = 3;
pub const LOW: usize = 0;
pub const BODY: usize = 1;
pub const AIR: usize = 2;

/// The ρ estimator's averaging time (ms).
pub const RHO_MS: f64 = 30.0;

/// A slot's parameters for one base sample: the type playing, and the one fading out after a type
/// change (gfx-grit's queue).
#[derive(Clone, Copy, Debug)]
pub struct SlotFrame {
    pub cur: SlotParams,
    pub old: SlotParams,
    /// weight of `cur` (raised cosine, 0 → 1 over 10 ms); 1 when no fade runs
    pub fade: f64,
    pub fading: bool,
    /// bumped on every type change: an engine seeing a new value moves its state to the fading side
    pub gen: u32,
}

impl SlotFrame {
    pub fn new() -> Self {
        Self { cur: SlotParams::off(), old: SlotParams::off(), fade: 1.0, fading: false, gen: 0 }
    }
}

impl Default for SlotFrame {
    fn default() -> Self {
        Self::new()
    }
}

/// What every engine reads for one base sample, per band.
#[derive(Clone, Copy, Debug)]
pub struct BandFrame {
    pub slots: [SlotFrame; 2],
    /// the morph μ: A weighs 1 − μ, B weighs μ
    pub mu: f64,
    /// whether each slot runs over the current control tick (its weight is not 0 at either end)
    pub run: [bool; 2],
    /// the ρ gain (1 while one slot runs)
    pub g_rho: f64,
    /// out += a_dry · dry + a_wet · wet (mix and level folded in)
    pub a_dry: f64,
    pub a_wet: f64,
}

impl Default for BandFrame {
    fn default() -> Self {
        Self { slots: [SlotFrame::new(); 2], mu: 0.0, run: [true, false], g_rho: 1.0, a_dry: 0.0, a_wet: 1.0 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub bands: [BandFrame; BANDS],
    /// crossover frequencies (Hz): low | body, body | air
    pub xf: [f64; 2],
}

impl Default for Frame {
    fn default() -> Self {
        Self { bands: [BandFrame::default(); BANDS], xf: [250.0, 3500.0] }
    }
}

/// Engine configuration: oversampling factor (1, 2, 4) and band count (1 – 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub factor: usize,
    pub bands: usize,
}

impl Config {
    /// Whether band `b` plays in this configuration.
    #[inline]
    pub fn has(&self, b: usize) -> bool {
        match self.bands {
            1 => b == BODY,
            2 => b != AIR,
            _ => true,
        }
    }
}

/// A slot in one engine: the playing type, the fading one, and whether it ran last sample.
#[derive(Clone, Debug, Default)]
pub struct SlotPair {
    pub cur: SlotState,
    pub old: SlotState,
    gen: u32,
    awake: bool,
}

impl SlotPair {
    fn reset(&mut self, gen: u32) {
        self.cur.reset();
        self.old.reset();
        self.gen = gen;
        self.awake = false;
    }

    #[inline]
    fn sync_gen(&mut self, f: &SlotFrame) {
        if f.gen != self.gen {
            self.gen = f.gen;
            std::mem::swap(&mut self.cur, &mut self.old);
            self.cur.reset();
        }
    }

    #[inline]
    fn tick(&mut self, f: &SlotFrame, x: &mut [f64; 2], nch: usize, ctx: &Inner) {
        self.sync_gen(f);
        if !self.awake {
            self.awake = true;
            self.cur.wake();
            self.old.wake();
        }
        if f.fading {
            let mut z = *x;
            self.old.tick(&f.old, &mut z, nch, ctx);
            self.cur.tick(&f.cur, x, nch, ctx);
            for ch in 0..nch {
                x[ch] = f.fade * x[ch] + (1.0 - f.fade) * z[ch];
            }
        } else {
            self.cur.tick(&f.cur, x, nch, ctx);
        }
    }

    #[inline]
    fn idle(&mut self, f: &SlotFrame, x: &[f64; 2], nch: usize, ctx: &Inner) {
        self.sync_gen(f);
        self.awake = false;
        self.cur.idle(&f.cur, x, nch, ctx);
    }
}

/// The ρ estimator's running averages (both channels pooled): means of A and B, and of A·B, A², B².
#[derive(Clone, Copy, Debug, Default)]
pub struct Rho {
    pub ma: f64,
    pub mb: f64,
    pub ab: f64,
    pub aa: f64,
    pub bb: f64,
}

impl Rho {
    /// The correlation of A and B (mean removed), clamped to −0.5 … 1; 1 when either is silent.
    pub fn rho(&self) -> f64 {
        let cab = self.ab - self.ma * self.mb;
        let caa = (self.aa - self.ma * self.ma).max(0.0);
        let cbb = (self.bb - self.mb * self.mb).max(0.0);
        let d = (caa * cbb).sqrt();
        if d < 1e-24 {
            1.0
        } else {
            (cab / d).clamp(-0.5, 1.0)
        }
    }
}

/// The ρ gain of a blend at μ: 1 / √((1 − μ)² + μ² + 2 μ (1 − μ) ρ) (gfx-acoustic's FIR-blend rule).
#[inline]
pub fn rho_gain(mu: f64, rho: f64) -> f64 {
    let a = 1.0 - mu;
    1.0 / (a * a + mu * mu + 2.0 * mu * a * rho).max(1e-6).sqrt()
}

#[derive(Clone, Debug, Default)]
struct BandState {
    slots: [SlotPair; 2],
    /// dry alignment, per channel
    half: [Half; 2],
    dc: [Svf; 2],
    rho: Rho,
}

pub struct Engine {
    pub cfg: Config,
    ctx: Inner,
    os: [[Oversampler2x; 2]; 2],
    tree: [Tree; 2],
    xf: [f64; 2],
    bands: [BandState; BANDS],
    droop: [f64; 2],
    rho_c: f64,
    /// test switch (README §5 Clean skeleton)
    pub droop_on: bool,
}

impl Engine {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            ctx: Inner::new(96000.0),
            os: Default::default(),
            tree: [Tree::new(Order::Lr4, cfg.bands.max(2)), Tree::new(Order::Lr4, cfg.bands.max(2))],
            xf: [0.0; 2],
            bands: Default::default(),
            droop: [0.0; 2],
            rho_c: 0.0,
            droop_on: true,
        }
    }

    /// Sets the configuration and the base rate; the state is reset.
    pub fn configure(&mut self, cfg: Config, base_sr: f32, f: &Frame) {
        self.cfg = cfg;
        let fs = base_sr as f64 * cfg.factor as f64;
        self.ctx = Inner::new(fs);
        self.rho_c = gfx_dsp::util::one_pole_coef(RHO_MS * 0.001 * fs);
        for t in self.tree.iter_mut() {
            *t = Tree::new(Order::Lr4, cfg.bands.max(2));
            t.prepare(fs as f32, 0.0);
        }
        for b in self.bands.iter_mut() {
            for d in b.dc.iter_mut() {
                d.set(SvfMode::HighPass, 5.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, fs as f32);
            }
        }
        self.reset(f);
    }

    pub fn reset(&mut self, f: &Frame) {
        for o in self.os.iter_mut().flatten() {
            o.reset();
        }
        self.xf = self.clamped(f.xf);
        for t in self.tree.iter_mut() {
            Self::set_tree(t, self.cfg.bands, self.xf, true);
            t.reset();
        }
        for (b, bf) in self.bands.iter_mut().zip(f.bands.iter()) {
            for (s, sf) in b.slots.iter_mut().zip(bf.slots.iter()) {
                s.reset(sf.gen);
            }
            b.half = Default::default();
            for d in b.dc.iter_mut() {
                d.reset();
            }
            b.rho = Rho::default();
        }
        self.droop = [0.0; 2];
    }

    /// The crossovers kept under 0.4 of the inner rate.
    fn clamped(&self, xf: [f64; 2]) -> [f64; 2] {
        let top = 0.4 * self.ctx.fs;
        [xf[0].min(top), xf[1].min(top)]
    }

    fn set_tree(t: &mut Tree, bands: usize, xf: [f64; 2], now: bool) {
        let n = bands.clamp(2, 3) - 1;
        for k in 0..n {
            if now {
                t.set_freq_now(k, xf[k] as f32);
            } else {
                t.set_freq(k, xf[k] as f32);
            }
        }
    }

    /// Copies the running phases of Ring and Decimate (pre-roll of a switch).
    pub fn copy_phases(&mut self, from: &Engine) {
        for (b, fb) in self.bands.iter_mut().zip(from.bands.iter()) {
            for (s, fs) in b.slots.iter_mut().zip(fb.slots.iter()) {
                s.cur.phase = fs.cur.phase;
                s.old.phase = fs.old.phase;
            }
        }
    }

    /// The ρ averages of band `b`.
    pub fn rho(&self, b: usize) -> Rho {
        self.bands[b].rho
    }

    /// One inner sample of both channels.
    #[inline]
    fn inner(&mut self, u: [f64; 2], nch: usize, f: &Frame) -> [f64; 2] {
        let mut split = [[0.0f64; gfx_dsp::crossover::MAX_BANDS]; 2];
        let nb = self.cfg.bands;
        for ch in 0..nch {
            if nb == 1 {
                split[ch][BODY] = u[ch];
            } else {
                self.tree[ch].process(u[ch], &mut split[ch]);
            }
        }
        let ctx = self.ctx;
        let rc = self.rho_c;
        let mut out = [0.0f64; 2];
        for b in 0..BANDS {
            if !self.cfg.has(b) {
                continue;
            }
            let st = &mut self.bands[b];
            let bf = &f.bands[b];
            let x = [split[0][b], split[1][b]];
            let mut h = [0.0; 2];
            for ch in 0..nch {
                h[ch] = st.half[ch].process(x[ch]);
            }
            let (wa, wb) = (1.0 - bf.mu, bf.mu);
            let mut ya = x;
            let mut yb = x;
            if bf.run[0] {
                st.slots[0].tick(&bf.slots[0], &mut ya, nch, &ctx);
            } else {
                st.slots[0].idle(&bf.slots[0], &x, nch, &ctx);
            }
            if bf.run[1] {
                st.slots[1].tick(&bf.slots[1], &mut yb, nch, &ctx);
            } else {
                st.slots[1].idle(&bf.slots[1], &x, nch, &ctx);
            }
            let mut wet = [0.0; 2];
            match bf.run {
                [true, true] => {
                    let r = &mut st.rho;
                    let (mut ma, mut mb, mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0, 0.0, 0.0);
                    for ch in 0..nch {
                        wet[ch] = (wa * ya[ch] + wb * yb[ch]) * bf.g_rho;
                        ma += ya[ch];
                        mb += yb[ch];
                        ab += ya[ch] * yb[ch];
                        aa += ya[ch] * ya[ch];
                        bb += yb[ch] * yb[ch];
                    }
                    let k = 1.0 / nch as f64;
                    r.ma = ma * k + (r.ma - ma * k) * rc;
                    r.mb = mb * k + (r.mb - mb * k) * rc;
                    r.ab = ab * k + (r.ab - ab * k) * rc;
                    r.aa = aa * k + (r.aa - aa * k) * rc;
                    r.bb = bb * k + (r.bb - bb * k) * rc;
                }
                [true, false] => wet[..nch].copy_from_slice(&ya[..nch]),
                [false, true] => wet[..nch].copy_from_slice(&yb[..nch]),
                [false, false] => {}
            }
            for ch in 0..nch {
                let w = st.dc[ch].process(wet[ch] as f32) as f64;
                out[ch] += bf.a_dry * h[ch] + bf.a_wet * w;
            }
        }
        if self.droop_on {
            let g = 1.0 + DROOP_A;
            for ch in 0..nch {
                let d = &mut self.droop[ch];
                *d = g * out[ch] - DROOP_A * *d;
                out[ch] = *d;
            }
        }
        out
    }

    /// One base sample of both channels.
    #[inline]
    pub fn process(&mut self, x: [f32; 2], nch: usize, f: &Frame) -> [f32; 2] {
        if self.cfg.bands > 1 {
            let xf = self.clamped(f.xf);
            if xf != self.xf {
                self.xf = xf;
                for t in self.tree[..nch].iter_mut() {
                    Self::set_tree(t, self.cfg.bands, xf, false);
                }
            }
        }
        let mut y = [0.0f32; 2];
        match self.cfg.factor {
            1 => {
                let o = self.inner([x[0] as f64, x[1] as f64], nch, f);
                y = [o[0] as f32, o[1] as f32];
            }
            2 => {
                let mut u = [[0.0f32; 2]; 2];
                for ch in 0..nch {
                    u[ch] = self.os[0][ch].up(x[ch]);
                }
                let mut v = [[0.0f32; 2]; 2];
                for k in 0..2 {
                    let o = self.inner([u[0][k] as f64, u[1][k] as f64], nch, f);
                    v[0][k] = o[0] as f32;
                    v[1][k] = o[1] as f32;
                }
                for ch in 0..nch {
                    y[ch] = self.os[0][ch].down(v[ch]);
                }
            }
            _ => {
                let mut u = [[0.0f32; 4]; 2];
                for ch in 0..nch {
                    let a = self.os[0][ch].up(x[ch]);
                    let b0 = self.os[1][ch].up(a[0]);
                    let b1 = self.os[1][ch].up(a[1]);
                    u[ch] = [b0[0], b0[1], b1[0], b1[1]];
                }
                let mut v = [[0.0f32; 4]; 2];
                for k in 0..4 {
                    let o = self.inner([u[0][k] as f64, u[1][k] as f64], nch, f);
                    v[0][k] = o[0] as f32;
                    v[1][k] = o[1] as f32;
                }
                for ch in 0..nch {
                    let a = self.os[1][ch].down([v[ch][0], v[ch][1]]);
                    let b = self.os[1][ch].down([v[ch][2], v[ch][3]]);
                    y[ch] = self.os[0][ch].down([a, b]);
                }
            }
        }
        y
    }
}
