// SPDX-License-Identifier: GPL-3.0-or-later
//! The transmitter and channel (README §2.3): Phone Line, Digital Line, Cell Codec, AM, NBFM, and the
//! noise sources they share.

use crate::{Ctx, REF};
use gfx_dsp::adaa::{Adaa1, Shaper};
use gfx_dsp::baseband::{noise_bandwidth, noise_variance, Cx, Discriminator, EnvelopeDetector, Gauss, IfFilter, Multipath, NoiseSquelch, Rotator, Walk};
use gfx_dsp::blep::{Kernel, Reader, Renderer};
use gfx_dsp::codec::{NoiseSuppressor, Plc, Predictor};
use gfx_dsp::converter::{Adc, Format, Shape};
use gfx_dsp::envelope::PeakFollower;
use gfx_dsp::lfo::Rng;
use gfx_dsp::matched::{design, Analog};
use gfx_dsp::oversample::Oversampler2x;
use gfx_dsp::svf::{Svf, SvfMode};
use std::f64::consts::{PI, TAU};

#[inline]
pub fn db(x: f64) -> f64 {
    10f64.powf(x / 20.0)
}

/// A noise level (dB re the reference) as a linear amplitude at `noise` 50 % (0 for "none"). `noise`
/// moves it by 0.6 dB per % and fades it to exactly 0 below 10 % (README §3): × `Ctx::nf`.
pub fn noise_lin(base_db: f64) -> f64 {
    if base_db <= -150.0 {
        0.0
    } else {
        REF * db(base_db)
    }
}

/// ±10 dB and ±12 dB as gains.
const DB_M10: f64 = 0.316_227_766_016_837_94;
const DB_P10: f64 = 3.162_277_660_168_379_5;
const DB_M12: f64 = 0.251_188_643_150_958;
const DB_P12: f64 = 3.981_071_705_534_973;
const DB_P30: f64 = 31.622_776_601_683_793;

/// Uniform white noise of unit RMS.
#[inline]
fn white(r: &mut Rng) -> f64 {
    r.bipolar() as f64 * 1.732_050_8
}

/// The 4th-order Butterworth high-pass sections' Qs, and the 6th-order Chebyshev (0.5 dB) low-pass's
/// (relative frequency, Q) sections.
const BW4: [f64; 2] = [0.541_196_1, 1.306_563];
fn cheby6() -> [(f64, f64); 3] {
    let eps = (10f64.powf(0.05) - 1.0).sqrt();
    let a = (1.0 / eps).asinh() / 6.0;
    let mut out = [(0.0, 0.0); 3];
    for (k, o) in out.iter_mut().enumerate() {
        let th = PI * (2 * k + 1) as f64 / 12.0;
        let s = a.sinh() * th.sin();
        let w = a.cosh() * th.cos();
        let w0 = (s * s + w * w).sqrt();
        *o = (w0, w0 / (2.0 * s));
    }
    out
}

/// A low-pass made of matched sections (Chebyshev or Butterworth), re-designable at control rate.
#[derive(Clone, Debug, Default)]
pub struct LpChain {
    s: [Svf; 3],
    n: usize,
}

impl LpChain {
    pub fn cheby(&mut self, hz: f64, sr: f64, ramp: u32) {
        let c = cheby6();
        self.n = 3;
        for k in 0..3 {
            let b = design(Analog::low_pass(c[k].1), (hz * c[k].0).min(0.45 * sr), sr);
            if ramp > 0 {
                self.s[k].set_biquad_ramped(&b, ramp);
            } else {
                self.s[k].set_biquad(&b);
            }
        }
    }
    pub fn butter4(&mut self, hz: f64, sr: f64, ramp: u32) {
        self.n = 2;
        for k in 0..2 {
            let b = design(Analog::low_pass(BW4[k]), hz.min(0.45 * sr), sr);
            if ramp > 0 {
                self.s[k].set_biquad_ramped(&b, ramp);
            } else {
                self.s[k].set_biquad(&b);
            }
        }
    }
    pub fn reset(&mut self) {
        for s in self.s.iter_mut() {
            s.reset();
        }
    }
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let mut y = x;
        for s in self.s[..self.n].iter_mut() {
            y = s.process(y);
        }
        y
    }
}

/// A 4th-order Butterworth high-pass of matched sections.
#[derive(Clone, Debug, Default)]
pub struct HpChain {
    s: [Svf; 2],
}

impl HpChain {
    pub fn set(&mut self, hz: f64, sr: f64, ramp: u32) {
        for k in 0..2 {
            let b = design(Analog::high_pass(BW4[k]), hz.min(0.45 * sr), sr);
            if ramp > 0 {
                self.s[k].set_biquad_ramped(&b, ramp);
            } else {
                self.s[k].set_biquad(&b);
            }
        }
    }
    pub fn reset(&mut self) {
        for s in self.s.iter_mut() {
            s.reset();
        }
    }
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.s[0].process(x);
        self.s[1].process(y)
    }
}

/// Seeded babble: band-passed noise under a 3 – 6 Hz syllabic envelope (distant speech).
#[derive(Clone, Debug)]
pub struct Babble {
    rng: Rng,
    bp: [Svf; 2],
    env: Walk,
    rate: Walk,
}

impl Babble {
    pub fn new(seed: u64) -> Self {
        Self { rng: Rng::new(seed), bp: [Svf::new(), Svf::new()], env: Walk::new(seed ^ 0x51), rate: Walk::new(seed ^ 0x52) }
    }
    pub fn prepare(&mut self, sr: f64) {
        self.bp[0].set(SvfMode::BandPass, 500.0, 0.8, 0.0, sr as f32);
        self.bp[1].set(SvfMode::BandPass, 1500.0, 0.8, 0.0, sr as f32);
    }
    pub fn reset(&mut self) {
        self.rng.reset();
        for s in self.bp.iter_mut() {
            s.reset();
        }
        self.env.reset();
        self.rate.reset();
    }
    /// About unit RMS while talking.
    #[inline]
    pub fn next(&mut self, sr: f64) -> f64 {
        let w = white(&mut self.rng) as f32;
        let a = self.bp[0].process(w);
        let b = self.bp[1].process(w);
        let r = 4.5 + 1.5 * self.rate.next(0.3 / sr);
        let e = self.env.next(2.0 * r / sr).max(0.0);
        2.2 * (a + b) as f64 * e * 1.7
    }
}

/// A Poisson process of impulses (crackle, clicks, atmospherics).
#[derive(Clone, Debug)]
pub struct Crackle {
    rng: Rng,
}

impl Crackle {
    pub fn new(seed: u64) -> Self {
        Self { rng: Rng::new(seed) }
    }
    pub fn reset(&mut self) {
        self.rng.reset();
    }
    /// An impulse (±0.2 … ±1) with probability `rate` / `sr` per sample, else 0.
    #[inline]
    pub fn next(&mut self, rate: f64, sr: f64) -> f64 {
        let u = self.rng.next_f64();
        if rate > 0.0 && u < rate / sr {
            let a = 0.2 + 0.8 * self.rng.next_f64();
            if self.rng.next_u64() & 1 == 0 {
                a
            } else {
                -a
            }
        } else {
            0.0
        }
    }
}

/// Mains hum with its 3rd and 5th harmonics.
#[derive(Clone, Debug, Default)]
pub struct Hum {
    rot: Rotator,
}

impl Hum {
    pub fn prepare(&mut self, sr: f64) {
        self.rot.prepare(sr);
    }
    pub fn reset(&mut self) {
        self.rot.reset();
    }
    #[inline]
    pub fn next(&mut self, hz: f64) -> f64 {
        self.rot.set_hz(hz);
        let z = self.rot.next();
        let z3 = z * z * z;
        let z5 = z3 * z * z;
        // unit RMS
        (z.im + 0.5 * z3.im + 0.25 * z5.im) * 1.2
    }
}

/// The analog line (POTS): band, cable tilt, slow AGC, hiss, hum, crosstalk, clicks.
#[derive(Clone, Debug)]
pub struct Line {
    pub hp: HpChain,
    pub lp: LpChain,
    tilt: Svf,
    /// the AGC's envelope (starts at its target, 0 dB, so a first note is not blasted) and its attack / release coefficients
    agc: f64,
    agc_c: (f64, f64),
    rng: Rng,
    hum: Hum,
    babble: Babble,
    clicks: Crackle,
    pub short: bool,
    /// hiss, hum, crosstalk at `noise` 50 %
    lv: [f64; 3],
    sr: f64,
}

/// The line's AGC target (peak) and range.
const LINE_AGC_TARGET: f64 = 0.25;
/// The line's noises (dB re reference): hiss, hum, crosstalk; the short line's hiss and hum.
const LINE_HISS: f64 = -50.0;
const LINE_HUM: f64 = -52.0;
const LINE_XTALK: f64 = -45.0;
const SHORT_HISS: f64 = -60.0;
const SHORT_HUM: f64 = -45.0;
/// Line clicks per second at `dropouts` 100 %.
const LINE_CLICKS: f64 = 20.0;
/// The line band's edges.
pub const LINE_LO: f64 = 280.0;
pub const LINE_HI: f64 = 3500.0;

impl Line {
    pub fn new() -> Self {
        Self { hp: HpChain::default(), lp: LpChain::default(), tilt: Svf::new(), agc: LINE_AGC_TARGET, agc_c: (0.0, 0.0), rng: Rng::new(0x11E5), hum: Hum::default(), babble: Babble::new(0xBAB1), clicks: Crackle::new(0xC11C), short: false, lv: [0.0; 3], sr: 48000.0 }
    }

    /// The analog line or the intercom's short line.
    pub fn set_short(&mut self, short: bool) {
        self.short = short;
        self.lv = if short { [noise_lin(SHORT_HISS), noise_lin(SHORT_HUM), 0.0] } else { [noise_lin(LINE_HISS), noise_lin(LINE_HUM), noise_lin(LINE_XTALK)] };
    }

    pub fn prepare(&mut self, sr: f64) {
        self.sr = sr;
        self.set_short(self.short);
        self.agc_c = ((-1.0 / (0.020 * sr)).exp(), (-1.0 / (0.400 * sr)).exp());
        self.hum.prepare(sr);
        self.babble.prepare(sr);
        self.tilt.set(SvfMode::HighShelf, 2000.0, 0.5, -2.0, sr as f32);
        self.design(0.0, 0, LINE_HI * 2.0);
    }

    /// The band for `band` (octaves); the low-pass edge capped at `cap` Hz.
    pub fn design(&mut self, band: f64, ramp: u32, cap: f64) {
        self.hp.set(LINE_LO * 2f64.powf(-band), self.sr, ramp);
        self.lp.cheby((LINE_HI * 2f64.powf(band)).min(cap), self.sr, ramp);
    }

    pub fn reset(&mut self) {
        self.hp.reset();
        self.lp.reset();
        self.tilt.reset();
        self.agc = LINE_AGC_TARGET;
        self.rng.reset();
        self.hum.reset();
        self.babble.reset();
        self.clicks.reset();
    }

    /// The band alone (Digital Line and Cell Codec run it before their converter).
    #[inline]
    pub fn band(&mut self, x: f32) -> f32 {
        let y = self.hp.process(x);
        self.lp.process(y)
    }

    /// The whole line; `gn` the noise gate (0 – 1).
    #[inline]
    pub fn process(&mut self, x: f32, c: &Ctx, gn: f64) -> f32 {
        let click = self.clicks.next(LINE_CLICKS * c.dropouts, self.sr) * 0.5;
        let y = self.band(x + click as f32);
        let y = self.tilt.process(y) as f64;
        let a = y.abs();
        let co = if a > self.agc { self.agc_c.0 } else { self.agc_c.1 };
        self.agc = a + (self.agc - a) * co;
        let g = (LINE_AGC_TARGET / self.agc.max(1e-9)).clamp(DB_M10, DB_P10);
        let mut out = y * g;
        if gn > 0.0 && c.nf > 0.0 {
            let mut n = self.lv[0] * white(&mut self.rng) + self.lv[1] * self.hum.next(c.mains);
            if !self.short {
                n += self.lv[2] * self.babble.next(self.sr);
            }
            out += gn * c.nf * n;
        }
        out as f32
    }
}

impl Default for Line {
    fn default() -> Self {
        Self::new()
    }
}

/// The 8 kHz converter halves around the virtual samples: the reader samples the input at each
/// instant, the renderer draws the held levels band-limited.
#[derive(Clone, Debug, Default)]
pub struct Conv8k {
    reader: Reader,
    render: Renderer,
    pub adc: Adc,
}

impl Conv8k {
    pub fn reset(&mut self) {
        self.reader.reset();
        self.render.reset();
        self.adc.reset();
    }
    /// Samples `x` at each of the host sample's instants, maps each virtual sample through `f`
    /// (instant index), renders.
    #[inline]
    pub fn process(&mut self, k: &Kernel, x: f32, c: &Ctx, mut f: impl FnMut(usize, f64) -> f64) -> f32 {
        for j in 0..c.ins.n {
            let d = c.ins.d[j];
            let s = self.reader.read(k, d) as f64;
            let w = self.adc.convert(s).level;
            let v = f(j, w);
            self.render.set(k, v as f32, d);
        }
        self.reader.push(x);
        self.render.next()
    }
}

/// The 8 kHz converters' anti-alias and reconstruction edge.
pub const CONV_EDGE: f64 = 3600.0;

/// G.711 µ-law at 8 kHz after the line band.
#[derive(Clone, Debug, Default)]
pub struct Digital {
    pub line: Line,
    conv: Conv8k,
    recon: LpChain,
}

impl Digital {
    pub fn prepare(&mut self, sr: f64) {
        self.line.prepare(sr);
        self.line.design(0.0, 0, CONV_EDGE);
        self.conv.adc.set(Format::MuLaw, Shape::Tread, 8.0, 0.0);
        self.recon.cheby(CONV_EDGE, sr, 0);
    }
    pub fn reset(&mut self) {
        self.line.reset();
        self.conv.reset();
        self.recon.reset();
    }
    #[inline]
    pub fn process(&mut self, k: &Kernel, x: f32, c: &Ctx) -> f32 {
        let b = self.line.band(x);
        let y = self.conv.process(k, b, c, |_, w| w);
        self.recon.process(y)
    }
}

/// The cell codec channel (README §2.3): band, 13-bit 8 kHz converter, noise suppressor, backward-
/// adaptive LPC pair, PLC on the decoder output, AGC; comfort noise on DTX.
#[derive(Clone)]
pub struct Cell {
    pub line: Line,
    conv: Conv8k,
    recon: LpChain,
    ns: NoiseSuppressor,
    pred: Predictor,
    plc: Plc,
    agc: f64,
    agc_att: f64,
    agc_rel: f64,
    rng: Rng,
    cn_lp: f64,
    bg_lp: [Svf; 1],
    keyed_mix: f64,
    sr: f64,
    /// tests: run the converter, band, AGC and comfort noise without the predictor and the bells
    pub bypass_codec: bool,
    /// the virtual-sample counter (the pulse grid's phase)
    grid: u64,
}

/// The cell's background (TX) noise, dB re reference.
const CELL_NOISE: f64 = -55.0;
/// The handset AGC: target (RMS-ish peak), range.
const CELL_AGC_TARGET: f64 = 0.14;

impl Cell {
    pub fn new() -> Self {
        Self {
            line: Line::new(),
            conv: Conv8k::default(),
            recon: LpChain::default(),
            ns: NoiseSuppressor::default(),
            pred: Predictor::default(),
            plc: Plc::default(),
            agc: CELL_AGC_TARGET,
            agc_att: 0.0,
            agc_rel: 0.0,
            rng: Rng::new(0xCE11),
            cn_lp: 0.0,
            bg_lp: [Svf::new()],
            keyed_mix: 1.0,
            sr: 48000.0,
            bypass_codec: false,
            grid: 0,
        }
    }
    pub fn prepare(&mut self, sr: f64) {
        self.sr = sr;
        self.line.prepare(sr);
        self.line.design(0.0, 0, CONV_EDGE);
        self.conv.adc.set(Format::Linear, Shape::Tread, 13.0, 0.0);
        self.recon.cheby(CONV_EDGE, sr, 0);
        self.plc.prepare();
        let r = gfx_dsp::codec::RATE;
        self.agc_att = (-1.0 / (0.050 * r)).exp();
        self.agc_rel = (-1.0 / (1.0 * r)).exp();
        self.bg_lp[0].set(SvfMode::LowPass, 2500.0, 0.7, 0.0, sr as f32);
    }
    pub fn reset(&mut self) {
        self.line.reset();
        self.conv.reset();
        self.recon.reset();
        self.ns.reset();
        self.pred.reset();
        self.plc.reset();
        self.agc = CELL_AGC_TARGET;
        self.rng.reset();
        self.cn_lp = 0.0;
        self.bg_lp[0].reset();
        self.keyed_mix = 1.0;
        self.grid = 0;
    }

    #[inline]
    pub fn process(&mut self, k: &Kernel, x: f32, c: &Ctx, gn: f64) -> f32 {
        // the handset's background noise; on DTX the smoothed comfort noise replaces it
        let mut xin = x as f64;
        if gn > 0.0 && c.nf > 0.0 {
            let target = if c.keyed { 1.0 } else { 0.0 };
            self.keyed_mix += (target - self.keyed_mix) * (1.0 - (-1.0 / (0.010 * self.sr)).exp());
            let w = white(&mut self.rng);
            let bg = self.bg_lp[0].process(w as f32) as f64 * 1.6;
            self.cn_lp += (w - self.cn_lp) * (TAU * 400.0 / self.sr).min(1.0);
            let cn = self.cn_lp * 3.2;
            xin += gn * c.nf * noise_lin(CELL_NOISE) * (bg * self.keyed_mix + cn * (1.0 - self.keyed_mix));
        }
        let b = self.line.band(xin as f32);
        let (q, alpha, cut) = c.codec;
        let bypass = self.bypass_codec;
        let (ns, pred, plc) = (&mut self.ns, &mut self.pred, &mut self.plc);
        let (agc, att, rel) = (&mut self.agc, self.agc_att, self.agc_rel);
        let grid = &mut self.grid;
        let y = self.conv.process(k, b, c, |j, w| {
            let pos = c.ins.pos[j];
            let v = if bypass {
                w
            } else {
                let s = ns.process(w, pos, cut);
                pred.process(s, pos, q, alpha, (*grid).is_multiple_of(3))
            };
            *grid += 1;
            let v = plc.process(v, c.ins.lost[j]);
            let a = v.abs();
            let co = if a > *agc { att } else { rel };
            *agc = a + (*agc - a) * co;
            let g = (CELL_AGC_TARGET / agc.max(1e-9)).clamp(DB_M12, DB_P12);
            v * g
        });
        self.recon.process(y)
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::new()
    }
}

/// A 2× island around an ADAA hard clip at [lo, hi] (the TX limiters).
#[derive(Clone, Debug, Default)]
pub struct TxClip {
    os: Oversampler2x,
    adaa: Adaa1,
}

impl TxClip {
    pub fn reset(&mut self) {
        self.os.reset();
        self.adaa.reset(Shaper::Hard);
    }
    #[inline]
    pub fn process(&mut self, x: f32, lo: f32, hi: f32) -> f32 {
        let (mid, half) = (0.5 * (hi + lo), 0.5 * (hi - lo));
        let u = self.os.up(x);
        let a = mid + half * self.adaa.process(Shaper::Hard, (u[0] - mid) / half);
        let b = mid + half * self.adaa.process(Shaper::Hard, (u[1] - mid) / half);
        self.os.down([a, b])
    }
}

/// The AM broadcast and table-radio receiver, in complex baseband at the host rate.
#[derive(Clone, Debug)]
pub struct Am {
    pe: Svf,
    split: Svf,
    agc: [PeakFollower; 2],
    clip: TxClip,
    tx_lp: LpChain,
    mp: Multipath,
    nb: [Babble; 2],
    nb_rot: [Rotator; 2],
    gauss: Gauss,
    atmos: Crackle,
    atmos_lp: [f64; 2],
    atmos_rng: Rng,
    whistle: Rotator,
    tune: Rotator,
    iff: IfFilter,
    car: f64,
    car_c: f64,
    gain: f64,
    gain_c: f64,
    det: EnvelopeDetector,
    dc: f64,
    dc_c: f64,
    audio: LpChain,
    bn: f64,
    /// the static's variance at the model's CNR, the neighbours' level at `noise` 50 %
    var0: f64,
    nb_g: f64,
    det_drive: f64,
    sr: f64,
    if_hz: f64,
}

/// The AM model's CNR at `noise` 50 % (dB), m nominal, the IF edge and audio edge (Hz), neighbour
/// level re the station (dB), the receiver AGC's ceiling (dB).
pub const AM_CNR: f64 = 35.0;
pub const AM_M: f64 = 0.85;
pub const AM_IF: f64 = 3000.0;
pub const AM_AUDIO: f64 = 4500.0;
const AM_NEIGHBOUR: f64 = -12.0;
pub const AM_AGC_MAX_DB: f64 = 30.0;
/// The spur's receiver frequency (Hz): it beats with the station at |2,500 − Δ|.
pub const AM_SPUR: f64 = -2500.0;
/// Atmospherics per second at `noise` 50 %.
const AM_ATMOS: f64 = 2.0;

impl Am {
    pub fn new() -> Self {
        Self {
            pe: Svf::new(),
            split: Svf::new(),
            agc: [PeakFollower::new(10.0, 300.0), PeakFollower::new(10.0, 300.0)],
            clip: TxClip::default(),
            tx_lp: LpChain::default(),
            mp: Multipath::default(),
            nb: [Babble::new(0xA1), Babble::new(0xA2)],
            nb_rot: [Rotator::default(), Rotator::default()],
            gauss: Gauss::new(0xA3),
            atmos: Crackle::new(0xA4),
            atmos_lp: [0.0; 2],
            atmos_rng: Rng::new(0xA5),
            whistle: Rotator::default(),
            tune: Rotator::default(),
            iff: IfFilter::default(),
            car: 1.0,
            car_c: 0.0,
            gain: 1.0,
            gain_c: 0.0,
            det: EnvelopeDetector::default(),
            dc: 1.0,
            dc_c: 0.0,
            audio: LpChain::default(),
            bn: 6000.0,
            var0: 0.0,
            nb_g: db(AM_NEIGHBOUR),
            det_drive: f64::NAN,
            sr: 48000.0,
            if_hz: AM_IF,
        }
    }

    pub fn prepare(&mut self, sr: f64) {
        self.sr = sr;
        self.pe.set(SvfMode::HighShelf, 3000.0, 0.7, 6.0, sr as f32);
        self.split.set(SvfMode::LowPass, 800.0, 0.7, 0.0, sr as f32);
        for a in self.agc.iter_mut() {
            a.prepare(sr as f32);
        }
        self.tx_lp.butter4(4500.0, sr, 0);
        self.mp.prepare(sr);
        for b in self.nb.iter_mut() {
            b.prepare(sr);
        }
        self.nb_rot[0].prepare(sr);
        self.nb_rot[0].set_hz(9000.0);
        self.nb_rot[1].prepare(sr);
        self.nb_rot[1].set_hz(-9000.0);
        self.whistle.prepare(sr);
        self.whistle.set_hz(AM_SPUR);
        self.tune.prepare(sr);
        self.car_c = (-TAU * 3.0 / sr).exp();
        self.gain_c = (-1.0 / (0.2 * sr)).exp();
        self.dc_c = (-TAU * 30.0 / sr).exp();
        self.design(0.0, 0);
    }

    /// `band` moves the IF (±B) and the audio low-pass (≤ 9 kHz).
    pub fn design(&mut self, band: f64, _ramp: u32) {
        self.if_hz = AM_IF * 2f64.powf(band);
        self.iff.set(6, self.if_hz, self.sr);
        self.bn = noise_bandwidth(6, self.if_hz);
        self.var0 = noise_variance(1.0, self.sr, AM_CNR, self.bn);
        self.audio.butter4((AM_AUDIO * 2f64.powf(band)).min(9000.0), self.sr, 0);
    }

    pub fn reset(&mut self) {
        self.pe.reset();
        self.split.reset();
        for a in self.agc.iter_mut() {
            a.reset();
        }
        self.clip.reset();
        self.tx_lp.reset();
        self.mp.reset();
        for b in self.nb.iter_mut() {
            b.reset();
        }
        for r in self.nb_rot.iter_mut() {
            r.reset();
        }
        self.gauss.reset();
        self.atmos.reset();
        self.atmos_lp = [0.0; 2];
        self.atmos_rng.reset();
        self.whistle.reset();
        self.tune.reset();
        self.iff.reset();
        self.car = 1.0;
        self.gain = 1.0;
        self.det.reset();
        self.det_drive = f64::NAN;
        self.dc = 1.0;
        self.audio.reset();
    }

    /// The receiver AGC's current gain (dB).
    pub fn agc_db(&self) -> f64 {
        20.0 * self.gain.log10()
    }

    #[inline]
    pub fn process(&mut self, x: f32, c: &Ctx, gn: f64) -> f32 {
        let sr = self.sr;
        // ---- broadcast processor
        let p = self.pe.process(x);
        let lo = self.split.process(p);
        let hi = p - lo;
        let gl = (0.5 / (self.agc[0].next(lo) as f64).max(1e-6)).clamp(DB_M12, DB_P12);
        let gh = (0.5 / (self.agc[1].next(hi) as f64).max(1e-6)).clamp(DB_M12, DB_P12);
        let s = (lo as f64 * gl + hi as f64 * gh) as f32;
        let s = self.clip.process(s, -1.0, 1.25);
        let s = self.tx_lp.process(s) as f64;
        // ---- modulation: drive pushes m from 0.85 to 2.5 (overmodulation)
        let m = c.am_m;
        let e = (1.0 + m * s).max(0.0);
        // ---- channel
        let mut cc = self.mp.process(e, c.dropouts);
        // the neighbours are the channel's crosstalk: they follow `noise` and the keying like the static
        let amp = (c.noise / 10.0).min(1.0) * gn;
        if amp > 0.0 {
            let lvl = self.nb_g * c.ng * amp;
            for k in 0..2 {
                let b = self.nb[k].next(sr) * 0.5;
                let env = lvl * (1.0 + AM_M * b.clamp(-1.0, 1.0));
                cc += self.nb_rot[k].next().scale(env);
            }
        } else {
            self.nb_rot[0].next();
            self.nb_rot[1].next();
        }
        if amp > 0.0 {
            cc += self.gauss.complex(self.var0 * c.vk).scale(amp);
            let a = self.atmos.next(AM_ATMOS * c.ng, sr) * 8.0;
            let k = 1.0 - (-TAU * 1000.0 / sr).exp();
            let ph = TAU * self.atmos_rng.next_f64();
            self.atmos_lp[0] += (a * ph.cos() - self.atmos_lp[0]) * k;
            self.atmos_lp[1] += (a * ph.sin() - self.atmos_lp[1]) * k;
            cc += Cx::new(self.atmos_lp[0], self.atmos_lp[1]).scale(amp);
        }
        // ---- receiver: tuning, the birdie, IF
        self.tune.set_hz(-c.tune * 9000.0);
        let mut r = cc * self.tune.next();
        let w = self.whistle.next();
        if c.whistle > 0.0 {
            r += w.scale(c.whistle);
        }
        let f = self.iff.process(r);
        // carrier-referenced AGC (0 … +30 dB)
        let a = f.abs();
        self.car = a + (self.car - a) * self.car_c;
        let gt = (1.0 / self.car.max(1e-9)).clamp(1.0, DB_P30);
        self.gain = gt + (self.gain - gt) * self.gain_c;
        // detector: |c| into the diode RC, DC block, audio low-pass
        if c.drive != self.det_drive {
            self.det_drive = c.drive;
            self.det.set_release((1.0 + 2.0 * c.drive) / (TAU * 3000.0), sr);
        }
        let d = self.det.process(f.scale(self.gain));
        self.dc = d + (self.dc - d) * self.dc_c;
        let y = (d - self.dc) / AM_M;
        self.audio.process(y as f32)
    }
}

impl Default for Am {
    fn default() -> Self {
        Self::new()
    }
}

/// The walkie-talkie's narrowband FM link.
#[derive(Clone, Debug)]
pub struct Nbfm {
    pe: [f64; 4],
    de: [f64; 4],
    pe_s: [f64; 2],
    de_s: [f64; 2],
    clip: TxClip,
    splat: LpChain,
    phase: f64,
    a: f64,
    roger_t: usize,
    roger_ph: f64,
    was_keyed: bool,
    fade_ph: f64,
    fade_rate: Walk,
    gauss: Gauss,
    tune: Rotator,
    iff: IfFilter,
    disc: Discriminator,
    hp: HpChain,
    audio: LpChain,
    pub sq: NoiseSquelch,
    mute: f64,
    sr: f64,
    bn: f64,
    var0: f64,
    tail: f64,
}

/// NBFM: CNR at `noise` 50 %, deviation, IF edge, TX gain into the deviation limiter.
pub const FM_CNR: f64 = 30.0;
pub const FM_DEV: f64 = 2500.0;
pub const FM_IF: f64 = 6000.0;
const FM_TX_GAIN: f64 = 2.0;
/// The noise squelch's level with no carrier (measured: tests::squelch_levels) and its threshold, as a
/// fraction of it; the tail time follows: τ = T / −ln(1 − θ).
pub const SQ_NO_CARRIER: f64 = 1.126;
pub const SQ_THETA: f64 = 0.8;
/// The receiver audio's noise limiter (README §2.3): above SQ_LIM_ONSET × the no-carrier noise level
/// (5 ms average) the audio is turned down by (onset / level)^SQ_LIM_POWER, which holds the open-squelch
/// noise 6 LU under the reference and leaves the audio alone down to about 3 dB CNR.
const SQ_LIM_ONSET: f64 = 0.6;
const SQ_LIM_POWER: f64 = 6.5;
/// The roger beep: tones (Hz), length of each (s).
const ROGER: [f64; 2] = [1000.0, 1500.0];
const ROGER_S: f64 = 0.080;

impl Nbfm {
    pub fn new() -> Self {
        Self {
            pe: [0.0; 4],
            de: [0.0; 4],
            pe_s: [0.0; 2],
            de_s: [0.0; 2],
            clip: TxClip::default(),
            splat: LpChain::default(),
            phase: 0.0,
            a: 1.0,
            roger_t: usize::MAX,
            roger_ph: 0.0,
            was_keyed: true,
            fade_ph: 0.0,
            fade_rate: Walk::new(0xFADE),
            gauss: Gauss::new(0xF0),
            tune: Rotator::default(),
            iff: IfFilter::default(),
            disc: Discriminator::default(),
            hp: HpChain::default(),
            audio: LpChain::default(),
            sq: NoiseSquelch::default(),
            mute: 1.0,
            sr: 48000.0,
            bn: noise_bandwidth(6, FM_IF),
            var0: 0.0,
            tail: f64::NAN,
        }
    }

    /// A first-order bilinear section (b0, b1, a1) with a zero at `z` Hz and a pole at `p` Hz, unity
    /// at 1 kHz, as [b0, b1, a1, 0].
    fn shelf(z: f64, p: f64, sr: f64) -> [f64; 4] {
        let k = 2.0 * sr;
        let (wz, wp) = (TAU * z, TAU * p);
        // H(s) = (1 + s / wz) / (1 + s / wp)
        let (n0, n1, d0, d1) = (1.0, 1.0 / wz, 1.0, 1.0 / wp);
        let a0 = d0 + d1 * k;
        let b0 = (n0 + n1 * k) / a0;
        let b1 = (n0 - n1 * k) / a0;
        let a1 = (d0 - d1 * k) / a0;
        let w = TAU * 1000.0;
        let g = (1.0 + (w / wz).powi(2)).sqrt() / (1.0 + (w / wp).powi(2)).sqrt();
        [b0 / g, b1 / g, a1, 0.0]
    }

    pub fn prepare(&mut self, sr: f64) {
        self.sr = sr;
        self.pe = Self::shelf(300.0, 6000.0, sr);
        self.de = Self::shelf(6000.0, 300.0, sr);
        self.splat.butter4(3000.0, sr, 0);
        self.tune.prepare(sr);
        self.iff.set(6, FM_IF, sr);
        self.disc.set(FM_DEV, sr);
        self.sq.prepare(sr);
        self.var0 = noise_variance(1.0, sr, FM_CNR, self.bn);
        self.design(0.0, 0);
    }

    pub fn design(&mut self, band: f64, ramp: u32) {
        self.hp.set(300.0 * 2f64.powf(-band), self.sr, ramp);
        self.audio.butter4((3000.0 * 2f64.powf(band)).min(9000.0), self.sr, ramp);
    }

    pub fn reset(&mut self) {
        self.pe_s = [0.0; 2];
        self.de_s = [0.0; 2];
        self.clip.reset();
        self.splat.reset();
        self.phase = 0.0;
        self.a = 1.0;
        self.roger_t = usize::MAX;
        self.roger_ph = 0.0;
        self.was_keyed = true;
        self.fade_ph = 0.0;
        self.fade_rate.reset();
        self.gauss.reset();
        self.tune.reset();
        self.iff.reset();
        self.disc.reset();
        self.hp.reset();
        self.audio.reset();
        self.sq.reset();
        self.tail = f64::NAN;
        self.mute = 1.0;
    }

    #[inline]
    fn first_order(c: &[f64; 4], s: &mut [f64; 2], x: f64) -> f64 {
        let y = c[0] * x + c[1] * s[0] - c[2] * s[1];
        s[0] = x;
        s[1] = y;
        y
    }

    #[inline]
    pub fn process(&mut self, x: f32, c: &Ctx) -> f32 {
        let sr = self.sr;
        // ---- keying: the roger beep, then the carrier ramps (2 ms)
        if self.was_keyed && !c.keyed && c.roger {
            self.roger_t = 0;
        }
        if c.keyed {
            self.roger_t = usize::MAX;
        }
        self.was_keyed = c.keyed;
        let roger_n = (ROGER_S * sr) as usize;
        let mut tx = x as f64 * FM_TX_GAIN;
        let carrier_on = if self.roger_t < 2 * roger_n {
            let f = ROGER[self.roger_t / roger_n];
            self.roger_ph = (self.roger_ph + f / sr).fract();
            tx = 0.6 * (TAU * self.roger_ph).sin();
            self.roger_t += 1;
            true
        } else {
            c.keyed
        };
        let step = 1.0 / (0.002 * sr);
        self.a = if carrier_on { (self.a + step).min(1.0) } else { (self.a - step).max(0.0) };
        // ---- TX: pre-emphasis, deviation limiter (2×, ADAA), splatter filter
        let p = Self::first_order(&self.pe, &mut self.pe_s, tx * c.dg);
        let l = self.clip.process(p as f32, -1.0, 1.0);
        let l = self.splat.process(l) as f64;
        self.phase += TAU * FM_DEV * l / sr;
        if self.phase > PI {
            self.phase -= TAU;
        } else if self.phase < -PI {
            self.phase += TAU;
        }
        let mut s = Cx::expj(self.phase).scale(self.a);
        // ---- channel: Rayleigh-like fading of two paths (2 – 8 Hz apart), receiver noise
        if c.dropouts > 0.0 {
            let fd = 5.0 + 3.0 * self.fade_rate.next(0.2 / sr);
            self.fade_ph = (self.fade_ph + fd / sr).fract();
            let a = c.dropouts;
            let g = (Cx::ONE + Cx::expj(TAU * self.fade_ph).scale(a)).scale(1.0 / (1.0 + a * a).sqrt());
            s = s * g;
        }
        if c.noise > 0.0 {
            s += self.gauss.complex(self.var0 * c.vk).scale((c.noise / 10.0).min(1.0));
        }
        // ---- RX: tuning (±12.5 kHz), IF, limiter-discriminator, de-emphasis, audio band
        self.tune.set_hz(-c.tune * 12500.0);
        let r = self.iff.process(s * self.tune.next());
        let (_, f) = self.disc.process(r);
        let de = Self::first_order(&self.de, &mut self.de_s, f);
        let y = self.hp.process(de as f32);
        let y = self.audio.process(y) as f64 / FM_TX_GAIN;
        // ---- noise squelch (fixed threshold; off at `squelch` 0) and the audio stage's noise limiter
        // the follower reaches the threshold in T = 150 ms × `squelch_tail` (less the 5 ms average, the
        // carrier's 2 ms fall and the mute's 5 ms ramp, which the tail also contains)
        if c.tail != self.tail {
            self.tail = c.tail;
            let rise = (0.150 * c.tail - 0.010).max(0.005) / -(1.0 - SQ_THETA).ln();
            self.sq.set(rise, 0.015, SQ_THETA * SQ_NO_CARRIER);
        }
        let open = self.sq.process(f) || !c.squelch_on;
        let st = 1.0 / (0.005 * sr);
        self.mute = if open { (self.mute + st).min(1.0) } else { (self.mute - st).max(0.0) };
        let a = SQ_LIM_ONSET * SQ_NO_CARRIER;
        let lim = if self.sq.noise() > a { (a / self.sq.noise()).powf(SQ_LIM_POWER) } else { 1.0 };
        (y * self.mute * lim) as f32
    }
}

impl Default for Nbfm {
    fn default() -> Self {
        Self::new()
    }
}
