// SPDX-License-Identifier: GPL-3.0-or-later
//! After the mix, shared by both paths: the four-band EQ (README §2.5) and the speaker emulation
//! (README §2.7). Matched second-order sections (gfx-dsp `matched`) in TPT SVFs, so the responses hold
//! from 44.1 to 192 kHz.

use gfx_dsp::matched::{design, Analog, Biquad};
use gfx_dsp::svf::Svf;

/// The EQ's fixed corners: bass shelf at 2 × `low_note`, treble shelf at 5 kHz, shelves Q 0.7.
pub const BASS_SHELF: f64 = 2.0;
pub const TREBLE_HZ: f64 = 5000.0;
pub const SHELF_Q: f64 = 0.7;
/// Bells: gfx-eq's proportional-Q law with its defaults (base Q 0.67, 50 %): Q × 10^(0.5 |g| / 40).
pub const BELL_Q: f64 = 0.67;
pub const BELL_PROP: f64 = 0.5;

/// The bell of the mid bands at `gain_db`.
pub fn bell(gain_db: f64) -> Analog {
    Analog::bell(BELL_Q * 10f64.powf(BELL_PROP * gain_db.abs() / 40.0), gain_db)
}

/// The EQ's four analog targets (frequency, prototype); `None` = the band is at 0 dB (bypassed).
pub fn eq_targets(low_note: f64, bass: f64, lo_mid: (f64, f64), hi_mid: (f64, f64), treble: f64) -> [Option<(Analog, f64)>; 4] {
    let some = |g: f64, p: Analog, f: f64| if g != 0.0 { Some((p, f)) } else { None };
    [
        some(bass, Analog::low_shelf(SHELF_Q, bass), BASS_SHELF * low_note),
        some(lo_mid.0, bell(lo_mid.0), lo_mid.1),
        some(hi_mid.0, bell(hi_mid.0), hi_mid.1),
        some(treble, Analog::high_shelf(SHELF_Q, treble), TREBLE_HZ),
    ]
}

/// One channel's EQ: a section per band, bypassed (not run) at 0 dB.
#[derive(Clone, Debug, Default)]
pub struct Eq {
    s: [Svf; 4],
    on: [bool; 4],
    last: [Option<Biquad>; 4],
}

impl Eq {
    pub fn reset(&mut self) {
        self.s.iter_mut().for_each(Svf::reset);
    }

    /// New designs (control rate); a band that comes on starts from silence, a moving band reaches
    /// its new coefficients linearly over `ramp` samples.
    pub fn set(&mut self, targets: &[Option<(Analog, f64)>; 4], sample_rate: f64, ramp: u32) {
        for k in 0..4 {
            match targets[k] {
                Some((p, f)) => {
                    let b = design(p, f, sample_rate);
                    if !self.on[k] {
                        self.s[k].reset();
                        self.s[k].set_biquad(&b);
                        self.on[k] = true;
                    } else if self.last[k] != Some(b) {
                        if ramp == 0 {
                            self.s[k].set_biquad(&b);
                        } else {
                            self.s[k].set_biquad_ramped(&b, ramp);
                        }
                    }
                    self.last[k] = Some(b);
                }
                None => {
                    self.on[k] = false;
                    self.last[k] = None;
                }
            }
        }
    }

    #[inline]
    pub fn process(&mut self, mut x: f32) -> f32 {
        for k in 0..4 {
            if self.on[k] {
                x = self.s[k].process(x);
            }
        }
        x
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CabModel {
    Off,
    Smooth,
    Ten8,
    Ten4,
    Fifteen1,
}

impl CabModel {
    pub const ALL: [CabModel; 5] = [CabModel::Off, CabModel::Smooth, CabModel::Ten8, CabModel::Ten4, CabModel::Fifteen1];

    pub fn from_index(i: u32) -> Self {
        *Self::ALL.get(i as usize).unwrap_or(&CabModel::Smooth)
    }
}

/// Butterworth-4 section Qs.
const BW4: [f64; 2] = [0.541_196, 1.306_563];

/// A cab model as analog sections: the low section (the box) and the body. All values are [A]:
/// designed from the box alignments and the makers' published responses until freely licensed IRs
/// are fitted (README §2.7, open question 4). The body keeps within ±0.3 dB of flat below 100 Hz, so
/// `cab_lows` off is the cab above the bass only.
pub struct CabDesign {
    pub low: [Option<(Analog, f64)>; 2],
    pub body: [Option<(Analog, f64)>; 6],
}

pub fn cab_design(m: CabModel, low_note: f64) -> CabDesign {
    let hp = |q: f64, f: f64| Some((Analog::high_pass(q), f));
    let lp = |q: f64, f: f64| Some((Analog::low_pass(q), f));
    let bell = |q: f64, g: f64, f: f64| Some((Analog::bell(q, g), f));
    match m {
        CabModel::Off => CabDesign { low: [None, None], body: [None; 6] },
        // a multiply-miked average: high-pass at 0.85 L (range-aware), broad dip at 500 Hz, 4th-order
        // low-pass at 5 kHz without a resonant peak (sections Q 0.54 and 0.6)
        CabModel::Smooth => CabDesign {
            low: [hp(std::f64::consts::FRAC_1_SQRT_2, 0.85 * low_note), None],
            body: [bell(1.4, -2.0, 500.0), lp(0.541_196, 5000.0), lp(0.6, 5000.0), None, None, None],
        },
        // sealed 8×10: a 2nd-order box (Q 1.1: the big low-mid weight), mid dip, cone breakup, steep top.
        // Anchors: 58 Hz – 5 kHz at ±3 dB, about −10 dB at 40 Hz (SVT-810E manual)
        CabModel::Ten8 => CabDesign {
            low: [hp(1.1, 62.0), None],
            body: [bell(1.4, -2.5, 800.0), bell(2.0, 3.0, 2200.0), lp(BW4[0], 5200.0), lp(BW4[1], 5200.0), None, None],
        },
        // ported 4×10 with a horn: 24 dB / oct below the tuning, mid dip, breakup, horn shelf
        CabModel::Ten4 => CabDesign {
            low: [hp(BW4[0], 45.0), hp(BW4[1], 45.0)],
            body: [
                bell(1.4, -2.5, 500.0),
                bell(1.5, 2.0, 2500.0),
                Some((Analog::high_shelf(0.7, 4.0), 5000.0)),
                lp(BW4[0], 9000.0),
                lp(BW4[1], 9000.0),
                None,
            ],
        },
        // ported 1×15: deep and dark
        CabModel::Fifteen1 => CabDesign {
            low: [hp(BW4[0], 36.0), hp(BW4[1], 36.0)],
            body: [bell(1.4, -2.0, 700.0), bell(1.5, 1.5, 1500.0), lp(BW4[0], 3200.0), lp(BW4[1], 3200.0), None, None],
        },
    }
}

/// One cab model running on one channel.
#[derive(Clone, Debug)]
pub struct Cab {
    pub model: CabModel,
    low: [Svf; 2],
    n_low: usize,
    body: [Svf; 6],
    n_body: usize,
    last_note: f64,
}

impl Cab {
    pub fn new(model: CabModel) -> Self {
        Self { model, low: Default::default(), n_low: 0, body: Default::default(), n_body: 0, last_note: f64::NAN }
    }

    /// (Re)designs the sections for `model` at `low_note`. `ramp` > 0 glides the coefficients
    /// (a moving `low_note`); a new model starts from silence.
    pub fn set(&mut self, model: CabModel, low_note: f64, sample_rate: f64, ramp: u32) {
        let fresh = model != self.model || self.last_note.is_nan();
        if !fresh && low_note == self.last_note {
            return;
        }
        self.model = model;
        self.last_note = low_note;
        let d = cab_design(model, low_note);
        let put = |s: &mut [Svf], list: &[Option<(Analog, f64)>]| {
            let mut n = 0;
            for (p, f) in list.iter().flatten() {
                let b = design(*p, *f, sample_rate);
                if fresh {
                    s[n].reset();
                    s[n].set_biquad(&b);
                } else {
                    s[n].set_biquad_ramped(&b, ramp.max(1));
                }
                n += 1;
            }
            n
        };
        self.n_low = put(&mut self.low, &d.low);
        self.n_body = put(&mut self.body, &d.body);
    }

    pub fn reset(&mut self) {
        self.low.iter_mut().for_each(Svf::reset);
        self.body.iter_mut().for_each(Svf::reset);
    }

    /// `lows`: 1 = the low section in, 0 = out (`cab_lows`, crossfaded by the caller).
    #[inline]
    pub fn process(&mut self, x: f32, lows: f32) -> f32 {
        if self.model == CabModel::Off {
            return x;
        }
        // the low section always runs, so `cab_lows` crossfades between two warm signals
        let mut l = x;
        for s in self.low[..self.n_low].iter_mut() {
            l = s.process(l);
        }
        let mut v = if lows >= 1.0 {
            l
        } else if lows <= 0.0 {
            x
        } else {
            x + (l - x) * lows
        };
        for s in self.body[..self.n_body].iter_mut() {
            v = s.process(v);
        }
        v
    }
}
