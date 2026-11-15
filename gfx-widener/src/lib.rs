// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-widener — golem's stereo widener and imager (design: README.md).
//!
//! **Width that cannot hurt the mono sum**: M = (L + R) / 2 passes untouched; every generator writes
//! only into the side, so (L + R) / 2 at the output is the dry fold to the rounding of f32 (§2.1).
//! - **Generators** (`mode`, decorr.rs): Velvet (two-band optimised velvet filter, 8 variants),
//!   Allpass (6 second-order all-passes, 8 sets), Comb (M(t − τ)), Double (two micro-shifted voices,
//!   (V₊ − V₋) / √2), M/S (none). Unit power gain, so `amount` maps to correlation.
//! - **`mono_law` Classic**: Haas for Comb (a true LR4 crossover on the mid at `bass_keep`), L / R
//!   voices for Double; inert elsewhere.
//! - **Side chain**: generated side → `high_cut` (2-pole) → LR4 high-pass at `bass_keep` → × amount ×
//!   transient duck; the source's own side → 12 dB / octave high-pass at `bass_keep`; both × `width`,
//!   then the sum-constant `balance` (S − b·M), `rotate` (the only non-exact control), `level`.
//! - **Structure changes** (`mode`, `variant`, `mono_law`, Velvet `time`): 30 ms equal-power crossfade
//!   of the generated contribution between two slots; Comb / Double `time`: two read heads, 50 ms.
//! - **Meters**: output correlation and the fold's deviation from the dry fold (gfx-dsp `stereo`).
//!
//! Zero latency, nothing reported. Real-time safe, deterministic.

#![allow(clippy::needless_range_loop)]

pub mod ap_tables;
pub mod decorr;
pub mod vnd_tables;

#[cfg(feature = "plugin")]
mod plugin;

#[cfg(test)]
mod tests;

use decorr::{Allpass, Velvet, VelvetBands};
use gfx_dsp::crossover::{Order, Twin};
use gfx_dsp::delay::{DelayLine, Interp};
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::onset::Strength;
use gfx_dsp::pitch::CenteredShifter;
use gfx_dsp::smooth::OnePole;
use gfx_dsp::stereo::{self, Correlation, FoldMeter};
use gfx_dsp::svf::{Svf, SvfMode};
use gfx_dsp::util::one_pole_coef;
use gfx_dsp::{Effect, ParamInfo, ParamKind};

pub const NP: usize = 15;

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::enumeration("mode", "Mode", 5, 0),
    ParamInfo::float("amount", "Amount", 0.0, 100.0, 60.0).with_exp(),
    ParamInfo::float("width", "Width", 0.0, 200.0, 100.0).with_exp(),
    ParamInfo::float("time", "Time", 1.0, 40.0, 20.0),
    ParamInfo::float("bass_keep", "Bass Keep", 0.0, 500.0, 120.0),
    ParamInfo::enumeration("mono_law", "Mono Law", 2, 0),
    ParamInfo { id: "variant", name: "Variant", min: 1.0, max: 8.0, default: 1.0, kind: ParamKind::Int, exp: false },
    ParamInfo::float("cents", "Detune", 0.0, 25.0, 9.0).with_exp(),
    ParamInfo::float("high_cut", "Side High Cut", 2000.0, 20000.0, 20000.0),
    ParamInfo::float("transient", "Transient Focus", 0.0, 100.0, 0.0),
    ParamInfo::float("balance", "Balance", -100.0, 100.0, 0.0).with_exp(),
    ParamInfo::float("rotate", "Rotation", -45.0, 45.0, 0.0).with_exp(),
    ParamInfo::float("level", "Level", -12.0, 12.0, 0.0),
    ParamInfo::enumeration("listen", "Listen", 3, 0),
    ParamInfo::boolean("bypass", "Bypass", false),
];

pub mod p {
    pub const MODE: usize = 0;
    pub const AMOUNT: usize = 1;
    pub const WIDTH: usize = 2;
    pub const TIME: usize = 3;
    pub const BASS_KEEP: usize = 4;
    pub const MONO_LAW: usize = 5;
    pub const VARIANT: usize = 6;
    pub const CENTS: usize = 7;
    pub const HIGH_CUT: usize = 8;
    pub const TRANSIENT: usize = 9;
    pub const BALANCE: usize = 10;
    pub const ROTATE: usize = 11;
    pub const LEVEL: usize = 12;
    pub const LISTEN: usize = 13;
    pub const BYPASS: usize = 14;
}

/// Mode labels (enum order, final; append only).
pub const MODES: [&str; 5] = ["Velvet", "Allpass", "Comb", "Double", "M/S"];
/// Mono law labels.
pub const LAWS: [&str; 2] = ["Exact", "Classic"];
/// Listen labels.
pub const LISTENS: [&str; 3] = ["Stereo", "Mono", "Side"];

pub mod mode {
    pub const VELVET: usize = 0;
    pub const ALLPASS: usize = 1;
    pub const COMB: usize = 2;
    pub const DOUBLE: usize = 3;
    pub const MS: usize = 4;
}

/// `high_cut` at this value is off (bypassed).
const HIGH_CUT_OFF: f32 = 20000.0;
const FADE_MS: f32 = 30.0;
const HEADS_MS: f64 = 50.0;
/// The shifters' centre (ms), as gfx-detune.
const SHIFT_CENTRE_MS: f32 = 3.0;
/// The Allpass generator's own delay for the duck's hold (ms).
const ALLPASS_HOLD_MS: f32 = 3.0;
const BW_Q: f32 = std::f32::consts::FRAC_1_SQRT_2;
/// The Haas crossover never goes below this (Hz): `bass_keep` 0 bypasses it instead.
const TWIN_MIN_HZ: f32 = 5.0;

/// What a generator slot plays: only the fields its mode depends on (so an inert change never fades).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Key {
    mode: usize,
    classic: bool,
    variant: usize,
    vtime: f32,
}

#[derive(Clone, Debug, Default)]
struct Slot {
    key: Option<Key>,
    velvet: Velvet,
    allpass: Allpass,
}

/// One slot's output: (side, mid addition, mid offset). Mid addition and offset are Classic only.
#[derive(Clone, Copy, Debug, Default)]
struct Gen {
    side: f32,
    mid: f32,
    offset: f32,
}

pub struct Widener {
    v: [f32; NP],
    sr: f32,
    clock: u64,
    amount: ExpSmoother,
    width: ExpSmoother,
    balance: ExpSmoother,
    rotate: ExpSmoother,
    cents: ExpSmoother,
    transient: OnePole,
    level: OnePole,
    bass: OnePole,
    hcut: OnePole,
    bypass: OnePole,
    listen: [OnePole; 3],
    /// the filters' frequencies as last applied
    bass_set: f32,
    hcut_set: f32,
    rot_set: f32,
    rot_cs: (f32, f32),
    // histories
    hist: DelayLine,
    bands: VelvetBands,
    // side filters
    src_hp: Svf,
    gen_hp: [Svf; 2],
    mid_hp: [Svf; 2],
    gen_lp: Svf,
    mid_lp: Svf,
    twin: Twin,
    // generators
    slots: [Slot; 2],
    cur: usize,
    fade: f32,
    fading: bool,
    fade_step: f32,
    shifters: [CenteredShifter; 2],
    heads: [f64; 2],
    heads_xf: f64,
    // transient focus
    strength: Strength,
    duck_env: f32,
    duck_hold: u32,
    duck_rel: f32,
    // meters
    corr: Correlation,
    fold: FoldMeter,
    /// a structural parameter moved since the slots last matched it
    dirty: bool,
    /// what the slots in use need: (Comb, Double, Classic)
    act: (bool, bool, bool),
}

impl Default for Widener {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn db_gain(db: f32) -> f32 {
    if db == 0.0 {
        1.0
    } else {
        10f32.powf(db / 20.0)
    }
}

impl Widener {
    pub fn new() -> Self {
        let v: [f32; NP] = core::array::from_fn(|i| PARAMS[i].default);
        let exp = |i: usize| ExpSmoother::new(PARAMS[i].min, PARAMS[i].max, v[i]);
        Self {
            v,
            sr: 48000.0,
            clock: 0,
            amount: exp(p::AMOUNT),
            width: exp(p::WIDTH),
            balance: exp(p::BALANCE),
            rotate: exp(p::ROTATE),
            cents: exp(p::CENTS),
            transient: OnePole::new(20.0, 0.0, 1e-6),
            level: OnePole::new(20.0, 1.0, 1e-7),
            bass: OnePole::new(30.0, v[p::BASS_KEEP], 0.01),
            hcut: OnePole::new(30.0, v[p::HIGH_CUT], 0.1),
            bypass: OnePole::new(20.0, 0.0, 1e-6),
            listen: [OnePole::new(10.0, 1.0, 1e-6), OnePole::new(10.0, 0.0, 1e-6), OnePole::new(10.0, 0.0, 1e-6)],
            bass_set: f32::NAN,
            hcut_set: f32::NAN,
            rot_set: 0.0,
            rot_cs: (1.0, 0.0),
            hist: DelayLine::new(),
            bands: VelvetBands::default(),
            src_hp: Svf::new(),
            gen_hp: [Svf::new(), Svf::new()],
            mid_hp: [Svf::new(), Svf::new()],
            gen_lp: Svf::new(),
            mid_lp: Svf::new(),
            twin: Twin::new(Order::Lr4),
            slots: [Slot::default(), Slot::default()],
            cur: 0,
            fade: 1.0,
            fading: false,
            fade_step: 1.0,
            shifters: [CenteredShifter::new(), CenteredShifter::new()],
            heads: [0.0; 2],
            heads_xf: 1.0,
            strength: Strength::default(),
            duck_env: 0.0,
            duck_hold: 0,
            duck_rel: 0.0,
            corr: Correlation::new(300.0),
            fold: FoldMeter::new(300.0),
            dirty: true,
            act: (false, false, false),
        }
    }

    /// The output's L / R correlation (−1 … +1), the `corr` meter.
    pub fn corr(&self) -> f32 {
        self.corr.value()
    }

    /// The fold's deviation from the dry fold (dB, −80 … +20), the `mono_err` meter.
    pub fn mono_err_db(&self) -> f32 {
        self.fold.db()
    }

    /// The transient duck's current gain (1 = none): a diagnostic for the tests.
    pub fn duck(&self) -> f32 {
        1.0 - self.transient.value() * 0.01 * self.duck_env
    }

    fn target_key(&self) -> Key {
        let m = (self.v[p::MODE].round().clamp(0.0, 4.0)) as usize;
        let classic = self.v[p::MONO_LAW] >= 0.5 && matches!(m, mode::COMB | mode::DOUBLE);
        let variant = if matches!(m, mode::VELVET | mode::ALLPASS) { (self.v[p::VARIANT].round().clamp(1.0, 8.0) as usize) - 1 } else { 0 };
        let vtime = if m == mode::VELVET { self.v[p::TIME].clamp(decorr::MIN_TIME_MS, 40.0) } else { 0.0 };
        Key { mode: m, classic, variant, vtime }
    }

    /// The generator's own delay (ms): how long the duck holds after an onset.
    fn gen_delay_ms(&self) -> f32 {
        match self.slots[self.cur].key.map_or(mode::VELVET, |k| k.mode) {
            mode::VELVET => self.v[p::TIME].clamp(decorr::MIN_TIME_MS, 40.0),
            mode::ALLPASS => ALLPASS_HOLD_MS,
            mode::COMB => self.v[p::TIME],
            mode::DOUBLE => self.v[p::TIME] + SHIFT_CENTRE_MS,
            _ => 0.0,
        }
    }

    fn uses_double(&self) -> bool {
        let d = |s: &Slot| s.key.is_some_and(|k| k.mode == mode::DOUBLE);
        d(&self.slots[self.cur]) || (self.fading && d(&self.slots[1 - self.cur]))
    }

    /// Loads a key into a slot from a clean state (no allocation).
    fn load(&mut self, i: usize, key: Key) {
        if key.mode == mode::DOUBLE && !self.uses_double() {
            // the shifters start from reset at each entry (determinism)
            self.shifters.iter_mut().for_each(|s| s.reset());
        }
        if key.classic && !self.any_classic() {
            // the Classic-only filters start from silence too
            self.mid_lp.reset();
            self.mid_hp.iter_mut().for_each(|s| s.reset());
            self.twin.reset();
        }
        let sr = self.sr;
        let s = &mut self.slots[i];
        s.key = Some(key);
        match key.mode {
            mode::VELVET => s.velvet.set(key.variant, key.vtime, sr),
            mode::ALLPASS => {
                s.allpass.set(key.variant, sr);
                s.allpass.reset();
            }
            _ => {}
        }
    }

    fn heads_target(&self) -> f64 {
        self.v[p::TIME].clamp(1.0, 40.0) as f64 * 0.001 * self.sr as f64
    }

    fn apply_bass(&mut self, f: f32) {
        if f == self.bass_set {
            return;
        }
        self.bass_set = f;
        if f > 0.0 {
            let sr = self.sr;
            self.src_hp.set(SvfMode::HighPass, f, BW_Q, 0.0, sr);
            for s in self.gen_hp.iter_mut().chain(self.mid_hp.iter_mut()) {
                s.set(SvfMode::HighPass, f, BW_Q, 0.0, sr);
            }
            self.twin.set_freq(f.max(TWIN_MIN_HZ));
        }
    }

    fn apply_hcut(&mut self, f: f32) {
        if f == self.hcut_set {
            return;
        }
        self.hcut_set = f;
        if f < HIGH_CUT_OFF {
            self.gen_lp.set(SvfMode::LowPass, f, BW_Q, 0.0, self.sr);
            self.mid_lp.set(SvfMode::LowPass, f, BW_Q, 0.0, self.sr);
        }
    }

    /// One slot's generator for this sample. `md`: the comb / double delay line's read; `dbl`: the
    /// shifted voices (V₊, V₋); `haas`: the crossover's twin of M − M.
    #[inline]
    fn generate(&mut self, i: usize, m: f32, md: f32, dbl: (f32, f32), haas: f32) -> Gen {
        let Some(k) = self.slots[i].key else { return Gen::default() };
        match k.mode {
            mode::VELVET => Gen { side: self.slots[i].velvet.process(&self.bands), ..Gen::default() },
            mode::ALLPASS => Gen { side: self.slots[i].allpass.process(m), ..Gen::default() },
            mode::COMB if k.classic => {
                // Haas: L = M, R = (1 − a)·M + a·M(t − τ), i.e. ± (M(t − τ) − M) / 2 in M / S
                let x = 0.5 * (md - m);
                Gen { side: -x, mid: x, offset: haas }
            }
            mode::COMB => Gen { side: md, ..Gen::default() },
            mode::DOUBLE if k.classic => Gen { side: 0.5 * (dbl.0 - dbl.1), mid: 0.5 * (dbl.0 + dbl.1), offset: 0.0 },
            mode::DOUBLE => Gen { side: std::f32::consts::FRAC_1_SQRT_2 * (dbl.0 - dbl.1), ..Gen::default() },
            _ => Gen::default(),
        }
    }

    fn any_classic(&self) -> bool {
        let c = |s: &Slot| s.key.is_some_and(|k| k.classic);
        c(&self.slots[self.cur]) || (self.fading && c(&self.slots[1 - self.cur]))
    }

    fn refresh(&mut self) {
        self.act = (self.any_mode(mode::COMB), self.any_mode(mode::DOUBLE), self.any_classic());
    }

    fn any_mode(&self, m: usize) -> bool {
        let c = |s: &Slot| s.key.is_some_and(|k| k.mode == m);
        c(&self.slots[self.cur]) || (self.fading && c(&self.slots[1 - self.cur]))
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len().min(2);
        if nch == 0 {
            return;
        }
        let n = buffers[0].len();
        let ms = self.sr * 0.001;
        for i in 0..n {
            // structure: a new key fades in once the current fade is over
            if !self.fading && self.dirty {
                let key = self.target_key();
                if self.slots[self.cur].key != Some(key) {
                    let other = 1 - self.cur;
                    self.load(other, key);
                    self.cur = other;
                    self.fade = 0.0;
                    self.fading = true;
                    self.refresh();
                } else {
                    self.dirty = false;
                }
            }
            let (xl, xr) = if nch > 1 { (buffers[0][i], buffers[1][i]) } else { (buffers[0][i], buffers[0][i]) };
            let (m, s_in) = stereo::to_ms(xl, xr);
            self.hist.push(m);
            self.bands.push(m);

            // smoothers
            let a = self.amount.next() * 0.01;
            let w = self.width.next() * 0.01;
            let b = self.balance.next() * 0.01;
            let rot = self.rotate.next();
            let tr = self.transient.next() * 0.01;
            let lvl = self.level.next();
            let bk = self.bass.next();
            let hc = self.hcut.next();
            self.apply_bass(bk);
            self.apply_hcut(hc);
            if rot != self.rot_set {
                self.rot_set = rot;
                let t = rot.to_radians();
                self.rot_cs = if rot == 0.0 { (1.0, 0.0) } else { (t.cos(), t.sin()) };
            }

            // transient duck: strength held for the generator's delay, then a 60 ms release
            // the detector runs only while `transient` is in use (it is a fifth of the frame's cost);
            // switched on mid-signal it starts from silence: the first ms read as an onset, under
            // the 20 ms ramp of `transient`
            let o = if tr > 0.0 || self.transient.target() > 0.0 {
                self.strength.next(m)
            } else {
                if self.duck_env != 0.0 || self.strength.value() != 0.0 {
                    self.strength.reset();
                }
                0.0
            };
            if o >= self.duck_env && o > 0.0 {
                self.duck_env = o;
                self.duck_hold = (self.gen_delay_ms() * ms) as u32;
            } else if self.duck_hold > 0 {
                self.duck_hold -= 1;
            } else {
                self.duck_env = o + (self.duck_env - o) * self.duck_rel;
                if self.duck_env < 1e-6 {
                    self.duck_env = 0.0;
                }
            }
            let dk = a * (1.0 - tr * self.duck_env);

            // comb / double read heads (two heads crossfaded on a `time` change)
            let (comb, double, classic) = self.act;
            let mut md = 0.0;
            if comb || double {
                let target = self.heads_target();
                if self.heads_xf >= 1.0 && (target - self.heads[1]).abs() > 0.5 {
                    self.heads = [self.heads[1], target];
                    self.heads_xf = 0.0;
                }
                if self.heads_xf < 1.0 {
                    self.heads_xf = (self.heads_xf + 1.0 / (HEADS_MS * 0.001 * self.sr as f64)).min(1.0);
                }
                let xf = self.heads_xf as f32;
                let rb = self.hist.read(self.heads[1], Interp::Hermite);
                md = if xf < 1.0 { (1.0 - xf) * self.hist.read(self.heads[0], Interp::Hermite) + xf * rb } else { rb };
            }
            let mut dbl = (0.0, 0.0);
            if double {
                let c = self.cents.next() as f64 / 100.0;
                self.shifters[0].set_semitones(c);
                self.shifters[1].set_semitones(-c);
                dbl = (self.shifters[0].process(md), self.shifters[1].process(md));
            } else {
                self.cents.next();
            }
            let haas_on = comb && classic && self.bass_set > 0.0;
            let haas = if haas_on { (self.twin.process(m as f64) - m as f64) as f32 } else { 0.0 };

            // generators, equal-power crossfade between the slots
            let cur = self.cur;
            let mut g = self.generate(cur, m, md, dbl, haas);
            if self.fading {
                let old = self.generate(1 - cur, m, md, dbl, haas);
                self.fade = (self.fade + self.fade_step).min(1.0);
                let (wo, wn) = if self.fade >= 1.0 {
                    (0.0, 1.0)
                } else {
                    let t = std::f32::consts::FRAC_PI_2 * self.fade;
                    (t.cos(), t.sin())
                };
                g = Gen { side: wn * g.side + wo * old.side, mid: wn * g.mid + wo * old.mid, offset: wn * g.offset + wo * old.offset };
                if self.fade >= 1.0 {
                    self.fading = false;
                    let o = 1 - cur;
                    self.slots[o].key = None;
                    self.refresh();
                }
            }

            // side chain: high cut, then the LR4 high-pass, then amount × duck
            let mut gs = g.side;
            let mut gm = g.mid;
            if self.hcut_set < HIGH_CUT_OFF {
                gs = self.gen_lp.process(gs);
                if classic {
                    gm = self.mid_lp.process(gm);
                }
            }
            let mut s_src = s_in;
            if self.bass_set > 0.0 {
                s_src = self.src_hp.process(s_in);
                gs = self.gen_hp.iter_mut().fold(gs, |y, f| f.process(y));
                if classic {
                    gm = self.mid_hp.iter_mut().fold(gm, |y, f| f.process(y));
                }
            }
            let mc = if classic { m + (gm * dk + g.offset) } else { m };
            let s = stereo::balance(mc, (s_src + gs * dk) * w, b);
            let (mr, sr) = stereo::rotate(mc, s, self.rot_cs.0, self.rot_cs.1);
            let (mut l, mut r) = stereo::to_lr(mr, sr);
            if lvl != 1.0 {
                l *= lvl;
                r *= lvl;
            }

            // bypass (20 ms crossfade)
            let k = self.bypass.next();
            let gain_ref = if k == 0.0 {
                lvl
            } else {
                if k == 1.0 {
                    l = xl;
                    r = xr;
                } else {
                    l += k * (xl - l);
                    r += k * (xr - r);
                }
                lvl + k * (1.0 - lvl)
            };

            // meters (before `listen`)
            self.corr.push(l, r);
            let f = 0.5 * (l + r);
            self.fold.push(f, m, gain_ref);

            // listen: Stereo · Mono (the fold on both) · Side (S on both)
            let (ws, wm, wd) = if self.listen[0].is_settled() && self.listen[1].is_settled() && self.listen[2].is_settled() {
                (self.listen[0].value(), self.listen[1].value(), self.listen[2].value())
            } else {
                (self.listen[0].next(), self.listen[1].next(), self.listen[2].next())
            };
            if nch > 1 {
                if ws == 1.0 {
                    buffers[0][i] = l;
                    buffers[1][i] = r;
                } else {
                    let sd = 0.5 * (l - r);
                    buffers[0][i] = ws * l + wm * f + wd * sd;
                    buffers[1][i] = ws * r + wm * f + wd * sd;
                }
            } else {
                // one channel: the fold (Stereo and Mono), or the side
                buffers[0][i] = if wd == 0.0 { f } else { (ws + wm) * f + wd * 0.5 * (l - r) };
            }
            self.clock += 1;
        }
    }
}

impl Effect for Widener {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        for e in [&mut self.amount, &mut self.width, &mut self.balance, &mut self.rotate, &mut self.cents] {
            e.prepare(sample_rate);
        }
        for o in [&mut self.transient, &mut self.level, &mut self.bass, &mut self.hcut, &mut self.bypass] {
            o.prepare(sample_rate);
        }
        self.listen.iter_mut().for_each(|o| o.prepare(sample_rate));
        let max = (0.041 * sample_rate) as usize + 16;
        self.hist.prepare(max);
        self.bands.prepare(sample_rate, max);
        for s in self.shifters.iter_mut() {
            s.prepare(sample_rate, SHIFT_CENTRE_MS, 1.0, 50.0);
        }
        self.twin.prepare(sample_rate, 0.0);
        self.strength.prepare(sample_rate);
        self.duck_rel = one_pole_coef(0.060 * sample_rate as f64) as f32;
        self.fade_step = 1.0 / (FADE_MS * 0.001 * sample_rate);
        self.corr.prepare(sample_rate);
        self.fold.prepare(sample_rate);
        self.reset();
    }

    fn reset(&mut self) {
        for e in [&mut self.amount, &mut self.width, &mut self.balance, &mut self.rotate, &mut self.cents] {
            e.reset();
        }
        for o in [&mut self.transient, &mut self.level, &mut self.bass, &mut self.hcut, &mut self.bypass] {
            o.settle();
        }
        self.listen.iter_mut().for_each(|o| o.settle());
        self.hist.reset();
        self.bands.reset();
        for s in [&mut self.src_hp, &mut self.gen_lp, &mut self.mid_lp] {
            s.reset();
        }
        self.gen_hp.iter_mut().chain(self.mid_hp.iter_mut()).for_each(|s| s.reset());
        self.twin.reset();
        self.bass_set = f32::NAN;
        self.hcut_set = f32::NAN;
        let (bk, hc) = (self.bass.value(), self.hcut.value());
        self.apply_bass(bk);
        if bk > 0.0 {
            self.twin.set_freq_now(bk.max(TWIN_MIN_HZ));
        }
        self.apply_hcut(hc);
        let rot = self.rotate.value();
        self.rot_set = rot;
        self.rot_cs = if rot == 0.0 { (1.0, 0.0) } else { (rot.to_radians().cos(), rot.to_radians().sin()) };
        self.slots = [Slot::default(), Slot::default()];
        self.shifters.iter_mut().for_each(|s| s.reset());
        self.cur = 0;
        let key = self.target_key();
        self.load(0, key);
        self.fading = false;
        self.fade = 1.0;
        self.dirty = false;
        self.refresh();
        let h = self.heads_target();
        self.heads = [h, h];
        self.heads_xf = 1.0;
        self.strength.reset();
        self.duck_env = 0.0;
        self.duck_hold = 0;
        self.corr.reset();
        self.fold.reset();
        self.clock = 0;
    }

    fn set_param(&mut self, id: &str, value: f32) {
        let Some(i) = PARAMS.iter().position(|pi| pi.id == id) else { return };
        let pi = PARAMS[i];
        let value = value.clamp(pi.min, pi.max);
        self.v[i] = value;
        let now = self.clock;
        match i {
            p::AMOUNT => self.amount.set_target(value, now),
            p::WIDTH => self.width.set_target(value, now),
            p::BALANCE => self.balance.set_target(value, now),
            p::ROTATE => self.rotate.set_target(value, now),
            p::CENTS => self.cents.set_target(value, now),
            p::TRANSIENT => self.transient.set_target(value),
            p::LEVEL => self.level.set_target(db_gain(value)),
            p::BASS_KEEP => self.bass.set_target(value),
            p::HIGH_CUT => self.hcut.set_target(value),
            p::BYPASS => self.bypass.set_target(if value >= 0.5 { 1.0 } else { 0.0 }),
            p::MODE | p::MONO_LAW | p::VARIANT | p::TIME => self.dirty = true,
            p::LISTEN => {
                let l = value.round().clamp(0.0, 2.0) as usize;
                for (k, o) in self.listen.iter_mut().enumerate() {
                    o.set_target(if k == l { 1.0 } else { 0.0 });
                }
            }
            _ => {}
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_block(buffers);
    }
}
