// SPDX-License-Identifier: GPL-3.0-or-later
//! One channel's device chain before the room (README §2): microphone → transmitter and channel →
//! amplifier + motor (one oversampled island) → horn throat → body → makeup, then the listener's
//! distance (direct gain, air, handset leak). The room is shared (lib.rs).

use crate::channel::{db, noise_lin, Am, Cell, Crackle, Digital, Hum, Line, Nbfm, CONV_EDGE};
use crate::voicing::{far, near, AmpKind, Cat, Chan, ChanCfg, Mic, Model, Room, MAKEUP_DB, MODELS, SPEAKERS};
use crate::{Ctx, REF};
use gfx_dsp::adaa::{Adaa1, Adaa1Fn, Antiderivative, HalfSample, Shaper};
use gfx_dsp::blep::Kernel;
use gfx_dsp::envelope::PeakFollower;
use gfx_dsp::lfo::Rng;
use gfx_dsp::oversample::{Oversampler2x, OversamplerN, MAX_FACTOR};
use gfx_dsp::speaker::{Body, Motor, Rattle, Throat};
use gfx_dsp::svf::{Svf, SvfMode};

/// What a chain runs: changing any of it crossfades to a fresh chain (README §2.7).
#[derive(Clone, Copy, Debug)]
pub struct Cfg {
    pub model: usize,
    pub mic: Mic,
    pub chan: ChanCfg,
    /// speaker voicing (0 = None)
    pub speaker: usize,
}

impl PartialEq for Cfg {
    fn eq(&self, o: &Cfg) -> bool {
        self.model == o.model && self.mic == o.mic && self.chan.kind == o.chan.kind && self.chan.short == o.chan.short && self.speaker == o.speaker
    }
}

impl Cfg {
    pub fn model(&self) -> &'static Model {
        &MODELS[self.model]
    }
    pub fn cat(&self) -> Cat {
        if self.speaker == 0 {
            Cat::Other
        } else {
            SPEAKERS[self.speaker].cat
        }
    }
}

/// The carbon mic's modulation per unit input: a −18 dBFS RMS 1 kHz sine reaches h·p = 0.1 after the
/// diaphragm (5 % 2nd harmonic, README §2.2). `drive` adds up to +12 dB.
const CARBON_H: f64 = 0.1 / (REF * std::f64::consts::SQRT_2 * 1.4636);
/// The carbon mic's frying noise and its level-dependent part (dB re reference).
const CARBON_FRY: f64 = -55.0;
const CARBON_CRACKLE: f64 = -38.0;

/// The microphone (README §2.2): pre-filter → memoryless curve at 2× → post-filter.
#[derive(Clone, Debug)]
struct MicStage {
    kind: Mic,
    pre: [Svf; 3],
    npre: usize,
    post: Svf,
    os: Oversampler2x,
    adaa: Adaa1,
    half: HalfSample,
    rng: Rng,
    fry: [Svf; 2],
    fry_g: (f64, f64),
    env: f64,
    env_c: f64,
}

impl MicStage {
    fn new() -> Self {
        Self { kind: Mic::None, pre: Default::default(), npre: 0, post: Svf::new(), os: Default::default(), adaa: Adaa1::new(), half: HalfSample::default(), rng: Rng::new(0xF2E1), fry: Default::default(), fry_g: (noise_lin(CARBON_FRY), noise_lin(CARBON_CRACKLE)), env: 0.0, env_c: 0.0 }
    }

    fn configure(&mut self, kind: Mic, sr: f64) {
        self.kind = kind;
        let s = sr as f32;
        let q = std::f32::consts::FRAC_1_SQRT_2;
        self.npre = 0;
        let push = |m: SvfMode, f: f32, qq: f32, g: f32, pre: &mut [Svf; 3], n: &mut usize| {
            pre[*n].set(m, f, qq, g, s);
            *n += 1;
        };
        match kind {
            Mic::None => {}
            Mic::Clean => push(SvfMode::HighPass, 80.0, q, 0.0, &mut self.pre, &mut self.npre),
            Mic::Carbon => {
                push(SvfMode::LowPass, 1400.0, 1.5, 0.0, &mut self.pre, &mut self.npre);
                push(SvfMode::HighPass, 250.0, q, 0.0, &mut self.pre, &mut self.npre);
                self.post.set(SvfMode::HighPass, 60.0, q, 0.0, s);
            }
            Mic::Dynamic => {
                push(SvfMode::LowShelf, 200.0, q, 6.0, &mut self.pre, &mut self.npre);
                push(SvfMode::Bell, 4000.0, 1.0, 4.0, &mut self.pre, &mut self.npre);
                push(SvfMode::HighPass, 60.0, q, 0.0, &mut self.pre, &mut self.npre);
            }
            Mic::Crystal => {
                push(SvfMode::Bell, 3000.0, 2.0, 8.0, &mut self.pre, &mut self.npre);
                push(SvfMode::HighPass, 150.0, q, 0.0, &mut self.pre, &mut self.npre);
                push(SvfMode::LowPass, 5000.0, 0.5, 0.0, &mut self.pre, &mut self.npre);
            }
            Mic::Electret => {
                push(SvfMode::HighPass, 100.0, q, 0.0, &mut self.pre, &mut self.npre);
                push(SvfMode::LowPass, 8000.0, q, 0.0, &mut self.pre, &mut self.npre);
                self.post.set(SvfMode::HighPass, 30.0, q, 0.0, s);
            }
        }
        self.fry[0].set(SvfMode::HighPass, 1000.0, q, 0.0, s);
        self.fry[1].set(SvfMode::LowPass, 4000.0, q, 0.0, s);
        self.env_c = (-1.0 / (0.05 * sr)).exp();
    }

    fn reset(&mut self) {
        for p in self.pre.iter_mut() {
            p.reset();
        }
        self.post.reset();
        self.os.reset();
        self.adaa.reset(Shaper::Tanh);
        self.half.reset();
        self.rng.reset();
        for f in self.fry.iter_mut() {
            f.reset();
        }
        self.env = 0.0;
    }

    /// The carbon curve: R = R0 (1 − h·p) at constant voltage, guarded at |h·p| ≤ 0.6.
    #[inline]
    fn carbon(u: f32) -> f32 {
        let g = 0.6 * (u / 0.6).tanh();
        1.0 / (1.0 - g) - 1.0
    }

    #[inline]
    fn process(&mut self, x: f32, c: &Ctx, gn: f64) -> f32 {
        let mut y = x;
        for p in self.pre[..self.npre].iter_mut() {
            y = p.process(y);
        }
        // `drive` into the mic: +12 dB at 100 % on the carbon button (README §2.2), a gentler +6 dB on the
        // dynamic and electret capsules (their tanh only colours)
        let g = if self.kind == Mic::Carbon { c.mic_g12 } else { c.mic_g6 } as f32;
        match self.kind {
            Mic::Carbon => {
                let h = CARBON_H as f32 * g;
                let u = self.os.up(y * h);
                let v = [Self::carbon(u[0]), Self::carbon(u[1])];
                let z = self.os.down(v);
                self.env = (z.abs() as f64) + (self.env - z.abs() as f64) * self.env_c;
                let mut out = self.post.process(z / h) as f64;
                if gn > 0.0 && c.nf > 0.0 {
                    let w = self.rng.bipolar() * 1.732;
                    let f = self.fry[0].process(w);
                    let f = self.fry[1].process(f) as f64 * 2.0;
                    let a = c.nf * (self.fry_g.0 + self.fry_g.1 * (self.env / 0.1));
                    out += gn * a * f;
                }
                out as f32
            }
            Mic::Dynamic if c.drive > 0.0 => {
                let u = self.os.up(y * g);
                let k = c.drive as f32;
                let mut v = [0.0f32; 2];
                for j in 0..2 {
                    let l = self.half.process(u[j]);
                    v[j] = l + k * (self.adaa.process(Shaper::Tanh, u[j]) - l);
                }
                self.os.down(v) / g
            }
            Mic::Electret => {
                // bias-starved: asymmetric soft clip at +6 dBFS-equivalent, slope 1 at rest
                let b = 0.3f64;
                let s = (1.0 - b.tanh() * b.tanh()) as f32;
                let u = self.os.up(y * g * 0.5);
                let v = [self.adaa.process(Shaper::Asym(b), u[0]), self.adaa.process(Shaper::Asym(b), u[1])];
                let z = self.os.down(v) * 2.0 / (s * g);
                self.post.process(z)
            }
            _ => y,
        }
    }
}

/// tanh(x / c + b) − tanh(b), scaled to slope 1 at rest and ceiling ≈ c: the tube and soundbox curves.
#[derive(Clone, Copy, Debug)]
struct TanhAt {
    c: f64,
    b: f64,
    tb: f64,
    s: f64,
}

impl TanhAt {
    fn new(c: f64, b: f64) -> Self {
        let tb = b.tanh();
        Self { c, b, tb, s: 1.0 - tb * tb }
    }
}

#[inline]
fn ln_cosh(x: f64) -> f64 {
    let a = x.abs();
    // e^(−2a) < 1e-17 beyond 20: exact in f64 without the exp
    if a > 20.0 {
        a - std::f64::consts::LN_2
    } else {
        a + (-2.0 * a).exp().ln_1p() - std::f64::consts::LN_2
    }
}

impl Antiderivative for TanhAt {
    #[inline]
    fn f(&self, x: f64) -> f64 {
        self.c * ((x / self.c + self.b).tanh() - self.tb) / self.s
    }
    #[inline]
    fn antiderivative(&self, x: f64) -> f64 {
        (self.c * self.c * ln_cosh(x / self.c + self.b) - self.c * self.tb * x) / self.s
    }
}

/// The class-B power stage: the rail clip, then the crossover dead band x − d·tanh(x / d), as one
/// memoryless curve under one ADAA (the crossover's steep centre aliases as much as the rail).
#[derive(Clone, Copy, Debug)]
struct ClassB {
    c: f64,
    d: f64,
}

impl ClassB {
    #[inline]
    fn k(&self, x: f64) -> f64 {
        if self.d > 0.0 {
            let z = x / self.d;
            x - self.d * if z.abs() > 20.0 { z.signum() } else { z.tanh() }
        } else {
            x
        }
    }
    #[inline]
    fn kint(&self, x: f64) -> f64 {
        if self.d > 0.0 {
            0.5 * x * x - self.d * self.d * ln_cosh(x / self.d)
        } else {
            0.5 * x * x
        }
    }
}

impl Antiderivative for ClassB {
    #[inline]
    fn f(&self, x: f64) -> f64 {
        self.k(x.clamp(-self.c, self.c))
    }
    #[inline]
    fn antiderivative(&self, x: f64) -> f64 {
        let c = self.c;
        if x > c {
            self.kint(c) + self.k(c) * (x - c)
        } else if x < -c {
            self.kint(c) - self.k(c) * (x + c)
        } else {
            self.kint(x)
        }
    }
}

/// The amplifier and the speaker's motor in one oversampled island (README §2.4, §2.5).
#[derive(Clone, Debug)]
struct AmpStage {
    kind: AmpKind,
    os: OversamplerN,
    /// the split island (4× amp, 2× motor): outer and inner half-band pairs
    two_rate: bool,
    osa: Oversampler2x,
    osb: Oversampler2x,
    adaa: Adaa1Fn,
    adb: AdaaB,
    motor: Motor,
    rattle: Rattle,
    has_motor: bool,
    gx: f64,
    fs0: f64,
    sag: f64,
    sag_c: f64,
    lim: PeakFollower,
    split: Svf,
    bass: [Svf; 2],
    rng: Rng,
    hum: Hum,
    crackle: Crackle,
    crackle_lp: Svf,
    /// hiss, hum, crackle at `noise` 50 %; whether there is any
    lv: (f64, f64, f64),
    has_noise: bool,
    soft: TanhAt,
    band: f64,
    sr: f64,
}

/// First-order ADAA for the class-B curve, keeping F(x₁) while the rail does not touch x₁ (F inside
/// the rail does not depend on it): one antiderivative per sample instead of two.
#[derive(Clone, Copy, Debug, Default)]
struct AdaaB {
    x1: f64,
    f1: f64,
    c1: f64,
    d1: f64,
}

impl AdaaB {
    #[inline]
    fn process(&mut self, g: &ClassB, x: f64) -> f64 {
        if g.d != self.d1 || (g.c != self.c1 && self.x1.abs() > g.c.min(self.c1)) {
            self.f1 = g.antiderivative(self.x1);
        }
        self.c1 = g.c;
        self.d1 = g.d;
        let fx = g.antiderivative(x);
        let d = x - self.x1;
        let y = if d.abs() < 1e-5 * (1.0 + x.abs()) { g.f(0.5 * (x + self.x1)) } else { (fx - self.f1) / d };
        self.x1 = x;
        self.f1 = fx;
        y
    }
}

/// The amp's memoryless curve at the island's rate.
#[inline]
fn shape(adaa: &mut Adaa1Fn, adb: &mut AdaaB, a: &crate::voicing::Amp, hard: &ClassB, soft: &TanhAt, y: f64) -> f64 {
    match a.kind {
        AmpKind::ClassB | AmpKind::ClassAb | AmpKind::Ic => adb.process(hard, y),
        AmpKind::Line | AmpKind::Smart => a.rail * (y / a.rail).tanh(),
        AmpKind::Tube | AmpKind::TubePa | AmpKind::Soundbox => adaa.process(soft, y),
    }
}

impl AmpStage {
    /// The motor (and the rub) driven by the amp's output.
    #[inline]
    fn speak(&mut self, y: f64, rattle: f64) -> f64 {
        let p = self.motor.process(y * self.gx) / self.gx;
        let (xm, vm) = self.motor.state();
        p + self.rattle.process(xm, vm, rattle) * 0.5
    }

    fn new() -> Self {
        Self {
            kind: AmpKind::Line,
            os: OversamplerN::new(1),
            two_rate: false,
            osa: Oversampler2x::new(),
            osb: Oversampler2x::new(),
            adaa: Adaa1Fn::default(),
            adb: AdaaB::default(),
            motor: Motor::default(),
            rattle: Rattle::default(),
            has_motor: false,
            gx: 1.0,
            fs0: 400.0,
            sag: 0.0,
            sag_c: 0.0,
            lim: PeakFollower::new(5.0, 150.0),
            split: Svf::new(),
            bass: Default::default(),
            rng: Rng::new(0xA417),
            hum: Hum::default(),
            crackle: Crackle::new(0xC4AC),
            crackle_lp: Svf::new(),
            lv: (0.0, 0.0, 0.0),
            has_noise: false,
            soft: TanhAt::new(1.0, 0.0),
            band: f64::NAN,
            sr: 48000.0,
        }
    }

    fn configure(&mut self, m: &Model, speaker: usize, sr: f64) {
        self.sr = sr;
        self.kind = m.amp.kind;
        // hard-rail amps at 4×, the others at 2× (README §2.4); with no speaker after it (the line out for
        // gfx-cab) the island runs one octave higher: at `drive` 100 % the hard rail sat on the −65 dB
        // part-swap bound at 4× (−65.0 dB) and the tube's tanh above it at 2× (−59 dB)
        let extra = usize::from(speaker == 0);
        self.os.set_stages(m.amp.kind.stages() + extra);
        self.has_motor = speaker != 0;
        // a 4× amp runs its motor at 2× (between two half-band pairs): the motor's own nonlinearity is
        // smooth, and at 4× it cost twice the island
        self.two_rate = self.has_motor && m.amp.kind.stages() == 2;
        let rate = if self.two_rate { 2.0 * sr } else { sr * self.os.factor() as f64 };
        let sp = &SPEAKERS[speaker];
        self.gx = sp.gx;
        self.fs0 = sp.motor.fs_hz;
        self.motor.set(sp.motor, rate);
        self.rattle.prepare(rate);
        self.sag_c = (-1.0 / (0.05 * sr)).exp();
        self.lim.prepare(sr as f32);
        self.split.set(SvfMode::LowPass, 200.0, 0.7, 0.0, sr as f32);
        self.bass[0].set(SvfMode::BandPass, 140.0, 0.8, 0.0, sr as f32);
        self.bass[1].set(SvfMode::BandPass, 350.0, 1.0, 0.0, sr as f32);
        self.hum.prepare(sr);
        self.crackle_lp.set(SvfMode::LowPass, 3000.0, 0.7, 0.0, sr as f32);
        self.lv = (noise_lin(m.amp.hiss_db), noise_lin(m.amp.hum_db), if m.amp.crackle > 0.0 { noise_lin(-50.0) } else { 0.0 });
        self.has_noise = self.lv.0 > 0.0 || self.lv.1 > 0.0 || self.lv.2 > 0.0;
        self.soft = TanhAt::new(m.amp.rail, m.amp.bias);
        self.band = f64::NAN;
    }

    fn reset(&mut self) {
        self.os.reset();
        self.osa.reset();
        self.osb.reset();
        self.adaa.reset();
        self.adb = AdaaB::default();
        self.motor.reset();
        self.rattle.reset();
        self.sag = 0.0;
        self.lim.reset();
        self.split.reset();
        for b in self.bass.iter_mut() {
            b.reset();
        }
        self.rng.reset();
        self.hum.reset();
        self.crackle.reset();
        self.crackle_lp.reset();
    }

    #[inline]
    fn process(&mut self, x: f32, m: &Model, c: &Ctx, gn: f64) -> f32 {
        let a = &m.amp;
        let mut u = x as f64 * c.dg;
        match a.kind {
            AmpKind::Smart => {
                let e = self.lim.next(u as f32) as f64;
                u *= (0.5 / e.max(1e-9)).min(1.0);
                if a.enhance > 0.0 {
                    let b = self.bass[0].process(u as f32).abs();
                    u += a.enhance * self.bass[1].process(b) as f64;
                }
            }
            AmpKind::TubePa => {
                let low = self.split.process(u as f32) as f64;
                u += 0.5 * (low / 0.5).tanh() - low;
            }
            _ => {}
        }
        // the model's own noise at the amp (hiss, hum, crackle / surface noise), gated
        if gn > 0.0 && c.nf > 0.0 && self.has_noise {
            let mut n = self.lv.0 * self.rng.bipolar() as f64 * 1.732 + self.lv.1 * self.hum.next(c.mains);
            if a.crackle > 0.0 {
                let r = a.crackle * (1.0 + 20.0 * c.dropouts);
                let k = self.crackle.next(r, self.sr) * 40.0 * self.lv.2;
                n += self.crackle_lp.process(k as f32) as f64;
            }
            u += gn * c.nf * n;
        }
        // the rail: battery, sag (50 ms envelope of the output), sputter below 30 %
        let batt = if a.kind.battery() { c.battery } else { 1.0 };
        let depth = 0.2 + 0.6 * (1.0 - batt) + if batt < 0.3 { 2.0 * (0.3 - batt) } else { 0.0 };
        let rail = (a.rail * (0.3 + 0.7 * batt) * (1.0 - depth.min(0.9) * self.sag.min(1.0))).max(0.03 * a.rail);
        let cross = a.cross * (1.0 + 3.0 * (1.0 - batt));
        let hard = ClassB { c: rail, d: cross };
        let soft = self.soft;
        if self.has_motor && c.band != self.band {
            self.band = c.band;
            self.motor.set_fs(self.fs0 * 2f64.powf(-c.band));
        }
        let out = if self.two_rate {
            let up = self.osa.up64(u);
            let mut m = [0.0f64; 2];
            for j in 0..2 {
                let v = self.osb.up64(up[j]);
                let w = [shape(&mut self.adaa, &mut self.adb, a, &hard, &soft, v[0]), shape(&mut self.adaa, &mut self.adb, a, &hard, &soft, v[1])];
                let y = self.osb.down64(w);
                m[j] = self.speak(y, c.rattle);
            }
            self.osa.down64(m)
        } else {
            let mut buf = [0.0f64; MAX_FACTOR];
            self.os.up(u, &mut buf);
            let n = self.os.factor();
            for v in buf[..n].iter_mut() {
                let y = shape(&mut self.adaa, &mut self.adb, a, &hard, &soft, *v);
                *v = if self.has_motor { self.speak(y, c.rattle) } else { y };
            }
            self.os.down(&mut buf)
        };
        let e = (out.abs() / rail).min(2.0);
        self.sag = e + (self.sag - e) * self.sag_c;
        out as f32
    }
}

/// The noise gate of a closing channel: up over 10 ms when keyed, down over 200 ms × `squelch_tail`.
#[derive(Clone, Debug, Default)]
struct Gate {
    v: f64,
}

impl Gate {
    #[inline]
    fn next(&mut self, keyed: bool, tail: f64, sr: f64) -> f64 {
        if keyed {
            self.v = (self.v + 1.0 / (0.010 * sr)).min(1.0);
        } else {
            self.v = (self.v - 1.0 / (0.200 * tail.max(0.01) * sr)).max(0.0);
        }
        self.v
    }
}

/// One channel's chain.
#[derive(Clone)]
pub struct Chain {
    pub cfg: Cfg,
    mic: MicStage,
    line: Line,
    digital: Digital,
    cell: Cell,
    am: Am,
    nbfm: Nbfm,
    amp: AmpStage,
    throat: Throat,
    has_throat: bool,
    throat_edge: f64,
    body: Body,
    air: Svf,
    leak: Svf,
    gate: Gate,
    /// cached: (drive, makeup) and (distance, room, direct gain, compensation, diffuse level)
    mk: (f64, f64),
    dist: (f64, Room, f64, f64, f64),
    sr: f64,
}

impl Chain {
    pub fn new() -> Self {
        Self {
            cfg: Cfg { model: 0, mic: Mic::None, chan: ChanCfg { kind: Chan::None, short: false }, speaker: 0 },
            mic: MicStage::new(),
            line: Line::new(),
            digital: Digital::default(),
            cell: Cell::new(),
            am: Am::new(),
            nbfm: Nbfm::new(),
            amp: AmpStage::new(),
            throat: Throat::default(),
            has_throat: false,
            throat_edge: 6000.0,
            body: Body::default(),
            air: Svf::new(),
            leak: Svf::new(),
            gate: Gate::default(),
            mk: (f64::NAN, 1.0),
            dist: (f64::NAN, Room::None, 1.0, 1.0, 0.0),
            sr: 48000.0,
        }
    }

    /// Allocates (the PLC history, the pitch detector) and sets the rate.
    pub fn prepare(&mut self, sr: f64) {
        self.sr = sr;
        self.line.prepare(sr);
        self.digital.prepare(sr);
        self.cell.prepare(sr);
        self.am.prepare(sr);
        self.nbfm.prepare(sr);
        self.throat.prepare(sr);
    }

    /// Sets the structure (then `reset`). Filters designed for `band`, `resonance`, `distance`.
    pub fn configure(&mut self, cfg: Cfg, c: &Ctx) {
        self.cfg = cfg;
        let m = cfg.model();
        self.mic.configure(cfg.mic, self.sr);
        self.line.set_short(cfg.chan.short);
        self.amp.configure(m, cfg.speaker, self.sr);
        let sp = &SPEAKERS[cfg.speaker];
        self.has_throat = cfg.speaker != 0 && sp.throat.is_some();
        if let Some((eta, edge)) = sp.throat {
            self.throat.set_eta(eta);
            self.throat_edge = edge;
        }
        let body = if cfg.speaker == 0 { &[][..] } else { sp.body };
        self.body.set(body, c.band, c.resonance, self.sr);
        self.design(c, 0);
        self.design_distance(c);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.mic.reset();
        self.line.reset();
        self.digital.reset();
        self.cell.reset();
        self.am.reset();
        self.nbfm.reset();
        self.amp.reset();
        self.throat.reset();
        self.body.reset();
        self.air.reset();
        self.leak.reset();
        self.gate = Gate::default();
        self.mk.0 = f64::NAN;
        self.dist.0 = f64::NAN;
    }

    /// `band` / `resonance` filters, ramped over `ramp` samples.
    pub fn design(&mut self, c: &Ctx, ramp: u32) {
        match self.cfg.chan.kind {
            Chan::PhoneLine => self.line.design(c.band, ramp, f64::INFINITY),
            Chan::DigitalLine => self.digital.line.design(c.band, ramp, CONV_EDGE),
            Chan::Cell => self.cell.line.design(c.band, ramp, CONV_EDGE),
            Chan::Am => self.am.design(c.band, ramp),
            Chan::Nbfm => self.nbfm.design(c.band, ramp),
            Chan::None => {}
        }
        if self.has_throat {
            self.throat.set_edge((self.throat_edge * 2f64.powf(c.band)).min(9000.0));
        }
        self.body.design(c.band, c.resonance, self.sr, ramp);
    }

    /// The listener's air / off-axis shelf and the handset's leak.
    pub fn design_distance(&mut self, c: &Ctx) {
        let s = self.sr as f32;
        self.air.set(SvfMode::HighShelf, 4000.0, 0.7, (-6.0 * c.distance) as f32, s);
        self.leak.set(SvfMode::HighPass, (100.0 * 8f64.powf(c.distance)) as f32, 0.7, 0.0, s);
    }

    /// The model's makeup at the current drive (dB table, linear interpolation) and wet polarity.
    #[inline]
    fn makeup(&self, drive: f64) -> f64 {
        let t = &MAKEUP_DB[self.cfg.model];
        let p = (drive.clamp(0.0, 1.0) * 4.0).min(3.999_999);
        let i = p as usize;
        db(t[i] + (t[i + 1] - t[i]) * (p - i as f64)) * self.cfg.model().polarity
    }

    /// The direct sound (after distance) and the room's send.
    #[inline]
    pub fn process(&mut self, k: &Kernel, x: f32, c: &Ctx) -> (f32, f32) {
        let gn = self.gate.next(c.keyed, c.tail, self.sr);
        let m = self.cfg.model();
        let y = if self.cfg.mic == Mic::None { x } else { self.mic.process(x, c, gn) };
        let y = match self.cfg.chan.kind {
            Chan::None => y,
            Chan::PhoneLine => self.line.process(y, c, gn),
            Chan::DigitalLine => self.digital.process(k, y, c),
            Chan::Cell => self.cell.process(k, y, c, gn),
            Chan::Am => self.am.process(y, c, gn),
            Chan::Nbfm => self.nbfm.process(y, c),
        };
        let y = self.amp.process(y, m, c, gn);
        let y = if self.has_throat { self.throat.process(y) } else { y };
        let y = self.body.process(y);
        if c.drive != self.mk.0 {
            self.mk = (c.drive, self.makeup(c.drive));
        }
        let y = (y as f64 * self.mk.1) as f32;
        // distance (README §2.6): direct gain, room level held, total loss ≤ 6 LU (5 dB of power, the air
        // shelf takes the rest)
        let cat = self.cfg.cat();
        if c.distance != self.dist.0 || c.room != self.dist.1 {
            let gd = (far(cat) / near(cat)).powf(-c.distance);
            let r = c.room.diffuse(cat);
            let comp = db(-5.0 * c.distance) * ((1.0 + r * r) / (gd * gd + r * r)).sqrt();
            self.dist = (c.distance, c.room, gd, comp, r);
        }
        let (_, _, gd, comp, r) = self.dist;
        let mut d = self.air.process(y);
        if cat == Cat::Handset {
            d = self.leak.process(d);
        }
        ((d as f64 * gd * comp) as f32, (y as f64 * comp * r) as f32)
    }

    /// The NBFM squelch is open (or the channel is not NBFM).
    pub fn squelch_open(&self) -> bool {
        self.cfg.chan.kind != Chan::Nbfm || self.nbfm.sq.is_open()
    }

    /// The NBFM noise squelch's followed level.
    pub fn squelch_level(&self) -> f64 {
        self.nbfm.sq.level()
    }

    /// The AM receiver's AGC gain (dB), tests.
    pub fn am_agc_db(&self) -> f64 {
        self.am.agc_db()
    }

    /// Tests: the cell codec without its predictor and bells.
    pub fn set_codec_bypass(&mut self, on: bool) {
        self.cell.bypass_codec = on;
    }
}

impl Default for Chain {
    fn default() -> Self {
        Self::new()
    }
}
