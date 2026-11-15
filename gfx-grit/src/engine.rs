// SPDX-License-Identifier: GPL-3.0-or-later
//! One oversampled engine (README §2.1 – §2.3): up 1× / 2× / 4× once, the Linkwitz-Riley tree with
//! its compensation all-passes (gfx-dsp `crossover::Tree`), per band two slots, the DC block, the
//! aligned dry and the per-band mix and gain, the band sum, the droop correction, down once.
//!
//! An engine has a fixed configuration (factor, slope). `quality` and `slope` changes run a second
//! engine beside the first (lib.rs): the parameters ([`Frame`]) are shared, the states are not.

use crate::modules::{Half, Inner, SlotFrame, SlotState};
use gfx_dsp::crossover::{Order, Tree};
use gfx_dsp::oversample::Oversampler2x;
use gfx_dsp::svf::{Svf, SvfMode};

pub const BANDS: usize = 4;

/// Droop correction C(z) = ((1 + a) / (1 + a z⁻¹))², after the band sum (README §2.1).
pub const DROOP_A: f64 = 0.8;

/// What every engine reads for one base sample.
#[derive(Clone, Copy, Debug)]
pub struct BandFrame {
    pub slots: [SlotFrame; 2],
    /// out = a_dry · dry + a_wet · wet (mix, gain, solo, bypass folded in)
    pub a_dry: f64,
    pub a_wet: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub bands: [BandFrame; BANDS],
    /// Effective crossover frequencies (Hz).
    pub xf: [f64; 3],
}

impl Frame {
    pub fn new() -> Self {
        Self {
            bands: [BandFrame { slots: [SlotFrame::new(0), SlotFrame::new(1)], a_dry: 0.0, a_wet: 1.0 }; BANDS],
            xf: [112.47, 632.46, 3556.6],
        }
    }
}

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

/// Engine configuration: oversampling factor (1, 2, 4) and crossover order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub factor: usize,
    pub order: Order,
}

#[derive(Clone, Debug, Default)]
struct BandState {
    slots: [SlotState; 2],
    /// dry alignment: [channel][first, second half sample]
    half: [[Half; 2]; 2],
    dc: [Svf; 2],
}

pub struct Engine {
    pub cfg: Config,
    base_sr: f32,
    ctx: Inner,
    os: [[Oversampler2x; 2]; 2],
    tree: [Tree; 2],
    xf: [f64; 3],
    bands: [BandState; BANDS],
    droop: [[f64; 2]; 2],
    /// test switches (README §5 Clean sum / Droop rows)
    pub droop_on: bool,
    pub tree_off: bool,
}

impl Engine {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            base_sr: 48000.0,
            ctx: Inner::new(96000.0),
            os: Default::default(),
            tree: [Tree::new(cfg.order, BANDS), Tree::new(cfg.order, BANDS)],
            xf: [0.0; 3],
            bands: Default::default(),
            droop: [[0.0; 2]; 2],
            droop_on: true,
            tree_off: false,
        }
    }

    /// Sets the configuration and the base rate; the state is reset.
    pub fn configure(&mut self, cfg: Config, base_sr: f32, f: &Frame) {
        self.cfg = cfg;
        self.base_sr = base_sr;
        let fs = base_sr as f64 * cfg.factor as f64;
        self.ctx = Inner::new(fs);
        for t in self.tree.iter_mut() {
            *t = Tree::new(cfg.order, BANDS);
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
        for t in self.tree.iter_mut() {
            for k in 0..3 {
                t.set_freq_now(k, f.xf[k] as f32);
            }
            t.reset();
        }
        self.xf = f.xf;
        for (b, bf) in self.bands.iter_mut().zip(f.bands.iter()) {
            for (s, sf) in b.slots.iter_mut().zip(bf.slots.iter()) {
                s.reset(sf.gen);
            }
            b.half = Default::default();
            for d in b.dc.iter_mut() {
                d.reset();
            }
        }
        self.droop = [[0.0; 2]; 2];
    }

    /// Copies the running phases of Ring mod and Sample divide (pre-roll of a switch).
    pub fn copy_phases(&mut self, from: &Engine) {
        for (b, fb) in self.bands.iter_mut().zip(from.bands.iter()) {
            for (s, fs) in b.slots.iter_mut().zip(fb.slots.iter()) {
                s.cur.phase = fs.cur.phase;
                s.old.phase = fs.old.phase;
            }
        }
    }

    /// One inner sample of both channels.
    #[inline]
    fn inner(&mut self, u: [f64; 2], nch: usize, f: &Frame) -> [f64; 2] {
        let mut split = [[0.0f64; gfx_dsp::crossover::MAX_BANDS]; 2];
        for ch in 0..nch {
            if self.tree_off {
                // reference: the tree's all-pass on band 0, nothing in the others
                self.tree[ch].process(u[ch], &mut split[ch]);
                let s: f64 = split[ch][..BANDS].iter().sum();
                split[ch] = [0.0; gfx_dsp::crossover::MAX_BANDS];
                split[ch][0] = s;
            } else {
                self.tree[ch].process(u[ch], &mut split[ch]);
            }
        }
        let ctx = self.ctx;
        let mut out = [0.0f64; 2];
        for (b, (st, bf)) in self.bands.iter_mut().zip(f.bands.iter()).enumerate() {
            let mut x = [split[0][b], split[1][b]];
            let mut h1 = [0.0; 2];
            let mut h2 = [0.0; 2];
            for ch in 0..nch {
                h1[ch] = st.half[ch][0].process(x[ch]);
                h2[ch] = st.half[ch][1].process(h1[ch]);
            }
            st.slots[0].tick(&bf.slots[0], &mut x, &h1, nch, &ctx);
            st.slots[1].tick(&bf.slots[1], &mut x, &h2, nch, &ctx);
            for ch in 0..nch {
                let wet = st.dc[ch].process(x[ch] as f32) as f64;
                out[ch] += bf.a_dry * h2[ch] + bf.a_wet * wet;
            }
        }
        if self.droop_on {
            let g = 1.0 + DROOP_A;
            for ch in 0..nch {
                let d = &mut self.droop[ch];
                d[0] = g * out[ch] - DROOP_A * d[0];
                d[1] = g * d[0] - DROOP_A * d[1];
                out[ch] = d[1];
            }
        }
        out
    }

    /// One base sample of both channels.
    #[inline]
    pub fn process(&mut self, x: [f32; 2], nch: usize, f: &Frame) -> [f32; 2] {
        if f.xf != self.xf {
            self.xf = f.xf;
            for t in self.tree[..nch].iter_mut() {
                for k in 0..3 {
                    t.set_freq(k, f.xf[k] as f32);
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
                    u[ch] = self.os[ch][0].up(x[ch]);
                }
                let mut v = [[0.0f32; 2]; 2];
                for k in 0..2 {
                    let o = self.inner([u[0][k] as f64, u[1][k] as f64], nch, f);
                    v[0][k] = o[0] as f32;
                    v[1][k] = o[1] as f32;
                }
                for ch in 0..nch {
                    y[ch] = self.os[ch][0].down(v[ch]);
                }
            }
            _ => {
                let mut u = [[0.0f32; 4]; 2];
                for ch in 0..nch {
                    let a = self.os[ch][0].up(x[ch]);
                    let b0 = self.os[ch][1].up(a[0]);
                    let b1 = self.os[ch][1].up(a[1]);
                    u[ch] = [b0[0], b0[1], b1[0], b1[1]];
                }
                let mut v = [[0.0f32; 4]; 2];
                for k in 0..4 {
                    let o = self.inner([u[0][k] as f64, u[1][k] as f64], nch, f);
                    v[0][k] = o[0] as f32;
                    v[1][k] = o[1] as f32;
                }
                for ch in 0..nch {
                    let a = self.os[ch][1].down([v[ch][0], v[ch][1]]);
                    let b = self.os[ch][1].down([v[ch][2], v[ch][3]]);
                    y[ch] = self.os[ch][0].down([a, b]);
                }
            }
        }
        y
    }
}
