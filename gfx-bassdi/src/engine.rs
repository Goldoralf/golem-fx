// SPDX-License-Identifier: GPL-3.0-or-later
//! The dirt engine and its clean twin (README §2.2, §2.3).
//!
//! An engine is one model's dirt path **and** the clean path that runs through the same all-passes:
//! every linear section of the dirt is a gfx-dsp `crossover` band-gain (twin + (g − 1) · band) whose
//! twin runs on the clean, the clean goes through the same half-band pairs (identity) and the same
//! half-sample delays as ADAA1. At small signals the dirt is then the clean times a real, positive
//! magnitude: the blend is a crossfade of magnitudes, it cannot comb.

use gfx_dsp::adaa::{Adaa1, HalfSample, Shaper};
use gfx_dsp::crossover::{Band, BandGain, Order, Split, Twin};
use gfx_dsp::envelope::PeakFollower;
use gfx_dsp::oversample::Oversampler2x;

/// Oversampling factor of the dirt (three 2× half-band stages, as gfx-emet).
pub const OS: usize = 8;
/// Low pre section at 4 × `low_note` (165 Hz at E1), high pre section at 1.5 kHz.
pub const LOW_SECTION: f64 = 4.0;
pub const HIGH_SECTION: f32 = 1500.0;
/// The interstage LR4 low-pass (8×), between the two stages (review: −79 dBFS worst alias).
pub const INTER_LP: f32 = 12000.0;
/// Glide of the `low_note`-scaled corners (README §2.8: 100 ms in log-Hz).
pub const LOW_NOTE_GLIDE_MS: f32 = 100.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    Modern,
    Classic,
    Vintage,
}

impl Model {
    pub const ALL: [Model; 3] = [Model::Modern, Model::Classic, Model::Vintage];

    pub fn from_index(i: u32) -> Self {
        *Self::ALL.get(i as usize).unwrap_or(&Model::Modern)
    }
}

/// How the model's sag / bias moves with the envelope of stage 1's input (`sag` 50 % = these).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sag {
    None,
    /// Stage 2's gain lowered by up to this many dB.
    Stage2Db(f64),
    /// Stage 1's bias moved up by this much.
    Bias(f64),
}

/// A model's voicing (README §2.3; values [A], to be voiced by ear).
#[derive(Clone, Copy, Debug)]
pub struct Voicing {
    /// Model's own offsets on the two pre sections (dB), `grunt` / `attack` add to them.
    pub low_db: f64,
    pub high_db: f64,
    /// Gain into stage 1 at `gain` 0 and 100 % (dB), linear in dB between.
    pub gain_db: (f64, f64),
    /// Stage 1's bias b: tanh(x + b) − tanh(b).
    pub bias: f64,
    /// Interstage high shelf (Hz, dB), followed by the 12 kHz LR4 low-pass; `None` = one stage.
    pub inter: Option<(f32, f64)>,
    /// Stage 2 (tanh) gain in dB.
    pub stage2_db: f64,
    pub sag: Sag,
    /// Post section: LR4 low-pass (the fizz cut).
    pub fizz: f32,
}

impl Voicing {
    pub fn of(m: Model) -> Self {
        match m {
            Model::Modern => Voicing {
                low_db: -6.0,
                high_db: 3.0,
                gain_db: (6.0, 48.0),
                bias: 0.25,
                inter: Some((3000.0, -6.0)),
                stage2_db: 12.0,
                sag: Sag::None,
                fizz: 6500.0,
            },
            Model::Classic => Voicing {
                low_db: 0.0,
                high_db: 0.0,
                gain_db: (0.0, 36.0),
                bias: 0.15,
                inter: Some((4000.0, -3.0)),
                // the README leaves the power stage's gain open: +6 dB [A]
                stage2_db: 6.0,
                sag: Sag::Stage2Db(3.0),
                fizz: 5000.0,
            },
            Model::Vintage => Voicing {
                low_db: 0.0,
                high_db: -3.0,
                gain_db: (0.0, 30.0),
                bias: 0.35,
                inter: None,
                stage2_db: 0.0,
                sag: Sag::Bias(0.15),
                fizz: 4500.0,
            },
        }
    }

    pub fn two_stages(&self) -> bool {
        self.inter.is_some()
    }

    /// Gain into stage 1 (dB) at `gain` 0 – 100 %.
    pub fn stage1_db(&self, gain: f64) -> f64 {
        self.gain_db.0 + (self.gain_db.1 - self.gain_db.0) * (gain / 100.0).clamp(0.0, 1.0)
    }
}

/// 8× up / down: three cascaded half-band stages (gfx-emet's).
#[derive(Clone, Debug, Default)]
pub struct Os8 {
    up: [Oversampler2x; 3],
    down: [Oversampler2x; 3],
}

impl Os8 {
    pub fn reset(&mut self) {
        for o in self.up.iter_mut().chain(self.down.iter_mut()) {
            o.reset();
        }
    }

    #[inline]
    pub fn up(&mut self, x: f32) -> [f32; OS] {
        let a = self.up[0].up(x);
        let mut b = [0.0f32; 4];
        for k in 0..2 {
            let u = self.up[1].up(a[k]);
            b[2 * k] = u[0];
            b[2 * k + 1] = u[1];
        }
        let mut c = [0.0f32; OS];
        for k in 0..4 {
            let u = self.up[2].up(b[k]);
            c[2 * k] = u[0];
            c[2 * k + 1] = u[1];
        }
        c
    }

    #[inline]
    pub fn down(&mut self, c: [f32; OS]) -> f32 {
        let mut b = [0.0f32; 4];
        for k in 0..4 {
            b[k] = self.down[2].down([c[2 * k], c[2 * k + 1]]);
        }
        let mut a = [0.0f32; 2];
        for k in 0..2 {
            a[k] = self.down[1].down([b[2 * k], b[2 * k + 1]]);
        }
        self.down[0].down(a)
    }
}

/// The per-sample controls of an engine (computed once per sample for every channel).
#[derive(Clone, Copy, Debug)]
pub struct Ctl {
    /// Pre sections' total gains (dB): model + `grunt`, model + `attack`.
    pub low_db: f64,
    pub high_db: f64,
    /// Linear gain into stage 1.
    pub g1: f32,
    /// `sag` relative to the model's own (0 – 2).
    pub sag: f64,
    /// Output scale of the dirt: loudness compensation × `dirt_level`.
    pub out: f32,
}

/// One model's dirt path and clean twin, one channel.
#[derive(Clone, Debug)]
pub struct Engine {
    pub model: Model,
    v: Voicing,
    // the dirt
    pre_lo: BandGain,
    pre_hi: BandGain,
    os: Os8,
    st1: Adaa1,
    inter_hi: BandGain,
    inter_lp: Split,
    st2: Adaa1,
    post: Split,
    env: PeakFollower,
    /// Linear pre-smoothing of x² (two one-poles, 1 ms) and its coefficient.
    ms: [f64; 2],
    ms_coef: f64,
    g2: f32,
    low_db: f64,
    high_db: f64,
    // the clean: the same all-passes, half-bands and half samples
    c_lo: Twin,
    c_hi: Twin,
    c_os: Os8,
    c_half: [HalfSample; 2],
    c_inter_hi: Twin,
    c_inter_lp: Twin,
    c_post: Twin,
}

impl Engine {
    pub fn new(model: Model) -> Self {
        let v = Voicing::of(model);
        let pre_lo = BandGain::new(Order::Lr2, Band::Low);
        let pre_hi = BandGain::new(Order::Lr2, Band::High);
        let inter_hi = BandGain::new(Order::Lr2, Band::High);
        Self {
            model,
            v,
            c_lo: pre_lo.new_twin(),
            c_hi: pre_hi.new_twin(),
            c_inter_hi: inter_hi.new_twin(),
            pre_lo,
            pre_hi,
            os: Os8::default(),
            st1: Adaa1::new(),
            inter_hi,
            inter_lp: Split::new(Order::Lr4),
            st2: Adaa1::new(),
            post: Split::new(Order::Lr4),
            env: PeakFollower::new(5.0, 150.0),
            ms: [0.0; 2],
            ms_coef: 0.0,
            g2: 1.0,
            low_db: f64::NAN,
            high_db: f64::NAN,
            c_os: Os8::default(),
            c_half: Default::default(),
            c_inter_lp: Twin::new(Order::Lr4),
            c_post: Twin::new(Order::Lr4),
        }
    }

    /// Sizes the sections for `sample_rate` and a lowest note `low_note` (Hz).
    pub fn prepare(&mut self, sample_rate: f32, low_note: f32) {
        let os = sample_rate * OS as f32;
        self.pre_lo.prepare(sample_rate, LOW_NOTE_GLIDE_MS);
        self.c_lo.prepare(sample_rate, LOW_NOTE_GLIDE_MS);
        self.pre_hi.prepare(sample_rate, 0.0);
        self.c_hi.prepare(sample_rate, 0.0);
        self.inter_hi.prepare(os, 0.0);
        self.c_inter_hi.prepare(os, 0.0);
        self.inter_lp.prepare(os, 0.0);
        self.c_inter_lp.prepare(os, 0.0);
        self.post.prepare(sample_rate, 0.0);
        self.c_post.prepare(sample_rate, 0.0);
        self.env.prepare(sample_rate);
        self.ms_coef = gfx_dsp::util::one_pole_coef(0.001 * sample_rate as f64);
        self.set_model(self.model, low_note);
    }

    /// Changes the voicing (an idle engine, before a crossfade) and sets every corner at once.
    pub fn set_model(&mut self, model: Model, low_note: f32) {
        self.model = model;
        self.v = Voicing::of(model);
        let lo = (LOW_SECTION * low_note as f64) as f32;
        self.pre_lo.set_freq_now(lo);
        self.c_lo.set_freq_now(lo);
        self.pre_hi.set_freq_now(HIGH_SECTION);
        self.c_hi.set_freq_now(HIGH_SECTION);
        let (fi, gi) = self.v.inter.unwrap_or((3000.0, 0.0));
        self.inter_hi.set_freq_now(fi);
        self.c_inter_hi.set_freq_now(fi);
        self.inter_hi.set_gain_db(gi);
        self.inter_lp.set_freq_now(INTER_LP);
        self.c_inter_lp.set_freq_now(INTER_LP);
        self.post.set_freq_now(self.v.fizz);
        self.c_post.set_freq_now(self.v.fizz);
        self.g2 = 10f32.powf(self.v.stage2_db as f32 / 20.0);
        self.low_db = f64::NAN;
        self.high_db = f64::NAN;
    }

    /// The lowest note moved: the low section and its twin glide to 4 L together.
    pub fn set_low_note(&mut self, low_note: f32) {
        let lo = (LOW_SECTION * low_note as f64) as f32;
        self.pre_lo.set_freq(lo);
        self.c_lo.set_freq(lo);
    }

    pub fn reset(&mut self) {
        self.pre_lo.reset();
        self.pre_hi.reset();
        self.os.reset();
        self.st1.reset(Shaper::Asym(self.v.bias));
        self.inter_hi.reset();
        self.inter_lp.reset();
        self.st2.reset(Shaper::Tanh);
        self.post.reset();
        self.env.reset();
        self.ms = [0.0; 2];
        self.c_lo.reset();
        self.c_hi.reset();
        self.c_os.reset();
        self.c_half.iter_mut().for_each(HalfSample::reset);
        self.c_inter_hi.reset();
        self.c_inter_lp.reset();
        self.c_post.reset();
    }

    pub fn voicing(&self) -> &Voicing {
        &self.v
    }

    /// One sample: (clean, dirt). `clean_in` and `dirt_in` differ only by the compressor (path Dirt).
    #[inline]
    pub fn process(&mut self, clean_in: f32, dirt_in: f32, c: &Ctl) -> (f32, f32) {
        let v = self.v;
        let two = v.two_stages();
        if c.low_db != self.low_db {
            self.low_db = c.low_db;
            self.pre_lo.set_gain_db(c.low_db);
        }
        if c.high_db != self.high_db {
            self.high_db = c.high_db;
            self.pre_hi.set_gain_db(c.high_db);
        }
        // ---- dirt
        let d = self.pre_hi.process(self.pre_lo.process(dirt_in as f64)) as f32 * c.g1;
        // the envelope of what stage 1 sees, full when it clips (peak ≥ 1): x², smoothed linearly
        // (two 1 ms one-poles) before the 5 ms / 150 ms follower, read as a sine's peak. A follower
        // on the raw samples switches attack / release with the sampling phase of a high note and
        // modulates the sag inharmonically
        let sq = (d as f64) * (d as f64);
        self.ms[0] = sq + (self.ms[0] - sq) * self.ms_coef;
        self.ms[1] = self.ms[0] + (self.ms[1] - self.ms[0]) * self.ms_coef;
        let e = ((2.0 * self.env.next(self.ms[1] as f32) as f64).sqrt()).min(1.0) * c.sag;
        let (bias, g2) = match v.sag {
            Sag::None => (v.bias, self.g2),
            Sag::Bias(b) => (v.bias + b * e, self.g2),
            Sag::Stage2Db(db) => (v.bias, self.g2 * 10f32.powf((-db * e) as f32 / 20.0)),
        };
        let s1 = Shaper::Asym(bias);
        let up = self.os.up(d);
        let mut out = [0.0f32; OS];
        for k in 0..OS {
            let mut y = self.st1.process(s1, up[k]);
            if two {
                let t = self.inter_hi.process(y as f64);
                let t = self.inter_lp.process(t).low;
                y = self.st2.process(Shaper::Tanh, t as f32 * g2);
            }
            out[k] = y;
        }
        let d = self.os.down(out);
        let d = self.post.process(d as f64).low as f32 * c.out;
        // ---- clean
        let x = self.c_hi.process(self.c_lo.process(clean_in as f64)) as f32;
        let up = self.c_os.up(x);
        let mut out = [0.0f32; OS];
        for k in 0..OS {
            let mut y = self.c_half[0].process(up[k]);
            if two {
                let t = self.c_inter_lp.process(self.c_inter_hi.process(y as f64));
                y = self.c_half[1].process(t as f32);
            }
            out[k] = y;
        }
        let x = self.c_os.down(out);
        let x = self.c_post.process(x as f64) as f32;
        (x, d)
    }
}
