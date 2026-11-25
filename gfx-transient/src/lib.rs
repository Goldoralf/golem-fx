// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-transient — golem's transient shaper (design: README.md).
//!
//! - **Detector** (gfx-dsp `sidechain` + `transient`), on the undelayed main or key signal: sidechain
//!   filters, a sliding maximum over W = clamp(1.2 / max(`sc_hpf` / 2, 30 Hz), 8 ms, 40 ms) (no ripple
//!   on a steady note), a floor 48 dB under the phrase level (never under `thresh`), and two
//!   differential pairs in dB: attack (E_f against a slow attack E_s) and sustain (a `sus_time`
//!   release against a 10 ms one). One saturating law with a 1.5 dB dead zone gives each a share.
//! - **Gain**: the shares are smoothed (zero latency: one-poles; lookahead: moving maximum, two
//!   boxes, then a delay of L − R, so the gain is complete at the onset), then multiplied by `attack`
//!   and `sustain` (dB). A sustain boost never exceeds the tail's height over the floor.
//! - **Audio**: the attack gain acts on the full band or through an exact first-order shelf (Highs /
//!   Lows, midpoint `att_freq`); the sustain gain and `level` on the full band; `mix` as dry +
//!   m · (wet − dry); the ceiling last (gfx-limiter's Clip style or its zero-latency Clean style).
//!
//! Zero latency by default; `lookahead` 1 / 2 / 5 ms reported. Bit-transparent at `attack` =
//! `sustain` = 0 with `ceil_mode` Off (or Limit under the ceiling). Real-time safe, deterministic,
//! block-size independent.

#![allow(clippy::needless_range_loop)]

#[cfg(feature = "plugin")]
pub mod plugin;

#[cfg(test)]
mod tests;

use gfx_dsp::delay::DelayLine;
use gfx_dsp::envelope::SlidingMax;
use gfx_dsp::exp_smooth::ExpSmoother;
use gfx_dsp::sidechain::{window_secs, ScFilter, MAX_WINDOW_S};
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::svf::OnePoleTpt;
use gfx_dsp::transient::{Detector, Ramp, Settings as DetSettings};
use gfx_dsp::util::one_pole_coef;
use gfx_dsp::{Effect, ParamInfo};
use gfx_limiter::{Limiter, Style};

pub const NP: usize = 21;

pub static PARAMS: [ParamInfo; NP] = [
    ParamInfo::float("attack", "Attack", -24.0, 24.0, 0.0).with_exp(),
    ParamInfo::float("sustain", "Sustain", -24.0, 24.0, 0.0).with_exp(),
    ParamInfo::float("att_time", "Attack Length", 1.0, 100.0, 15.0),
    ParamInfo::float("sus_time", "Sustain Length", 20.0, 3000.0, 300.0),
    ParamInfo::enumeration("att_band", "Attack Band", 3, 0),
    ParamInfo::enumeration("ceil_mode", "Ceiling", 3, 0),
    ParamInfo::float("sens", "Sensitivity", 0.0, 100.0, 50.0),
    ParamInfo::float("smooth", "Smooth", 0.05, 5.0, 0.5),
    ParamInfo::float("att_freq", "Attack Band Freq", 200.0, 8000.0, 1500.0),
    ParamInfo::float("sc_hpf", "Sidechain Low Cut", 0.0, 2000.0, 0.0),
    ParamInfo::float("sc_lpf", "Sidechain High Cut", 500.0, 22000.0, 22000.0),
    ParamInfo::float("thresh", "Noise Floor", -96.0, -30.0, -66.0),
    ParamInfo::enumeration("source", "Detector Source", 2, 0),
    ParamInfo::float("link", "Stereo Link", 0.0, 100.0, 100.0),
    ParamInfo::enumeration("lookahead", "Lookahead", 4, 0),
    ParamInfo::float("ceiling", "Ceiling Level", -24.0, 0.0, 0.0),
    ParamInfo::float("mix", "Mix", 0.0, 100.0, 100.0).with_exp(),
    ParamInfo::float("level", "Level", -24.0, 12.0, 0.0).with_exp(),
    ParamInfo::boolean("listen", "Listen", false),
    ParamInfo::boolean("delta", "Delta", false),
    ParamInfo::boolean("bypass", "Bypass", false),
];

pub mod p {
    pub const ATTACK: usize = 0;
    pub const SUSTAIN: usize = 1;
    pub const ATT_TIME: usize = 2;
    pub const SUS_TIME: usize = 3;
    pub const ATT_BAND: usize = 4;
    pub const CEIL_MODE: usize = 5;
    pub const SENS: usize = 6;
    pub const SMOOTH: usize = 7;
    pub const ATT_FREQ: usize = 8;
    pub const SC_HPF: usize = 9;
    pub const SC_LPF: usize = 10;
    pub const THRESH: usize = 11;
    pub const SOURCE: usize = 12;
    pub const LINK: usize = 13;
    pub const LOOKAHEAD: usize = 14;
    pub const CEILING: usize = 15;
    pub const MIX: usize = 16;
    pub const LEVEL: usize = 17;
    pub const LISTEN: usize = 18;
    pub const DELTA: usize = 19;
    pub const BYPASS: usize = 20;
}

/// `att_band` option labels (enum order, final: append only).
pub const BANDS: [&str; 3] = ["Full", "Highs", "Lows"];
/// `ceil_mode` option labels (enum order, final).
pub const CEIL_MODES: [&str; 3] = ["Off", "Soft", "Limit"];
/// Lookahead options (enum order, final), in ms: gfx-limiter's list.
pub const LOOKAHEAD_MS: [f32; 4] = gfx_limiter::LOOKAHEAD_MS;

const MAX_CH: usize = 2;
/// Control-rate period (samples): detector times, filters, window.
const CTL: u64 = 32;
/// Audio is processed in chunks of this many samples (the ceiling and bypass stages run per chunk).
const CHUNK: usize = 256;

#[inline]
fn idx(v: f32, n: usize) -> usize {
    (v.round().max(0.0) as usize).min(n - 1)
}

#[inline]
fn db_to_lin(db: f64) -> f64 {
    (db * (std::f64::consts::LN_10 / 20.0)).exp()
}

/// One detector: the sliding maximum, the differential pairs, the two smoothed shares.
#[derive(Clone)]
struct Det {
    max: SlidingMax,
    det: Detector,
    ramp_a: Ramp,
    ramp_s: Ramp,
    sh_a: f64,
    sh_s: f64,
}

impl Default for Det {
    fn default() -> Self {
        Self { max: SlidingMax::new(), det: Detector::default(), ramp_a: Ramp::new(true), ramp_s: Ramp::new(false), sh_a: 0.0, sh_s: 0.0 }
    }
}

impl Det {
    fn reset(&mut self) {
        self.max.reset();
        self.det.reset();
        self.ramp_a.reset();
        self.ramp_s.reset();
        self.sh_a = 0.0;
        self.sh_s = 0.0;
    }
}

/// The attack stage of one channel (§2.5): Full (`ga` · x) or an exact first-order shelf,
/// x + (ga − 1) · HP₁(x) with the cut-off at `att_freq` · √ga (Highs), x + (ga − 1) · LP₁(x) at
/// `att_freq` / √ga (Lows): monotonic from 1 to ga, midpoint at `att_freq` for any gain. Both
/// filters run every sample, so a band change finds them warm.
#[derive(Clone, Copy, Debug, Default)]
pub struct AttackStage {
    hi: OnePoleTpt,
    lo: OnePoleTpt,
}

impl AttackStage {
    pub fn reset(&mut self) {
        self.hi.reset();
        self.lo.reset();
    }

    /// One sample through both filters for the gain `ga` (linear), midpoint `f0`; `g0` is the cached
    /// coefficient at `f0` (used when ga = 1). Returns (high-pass, low-pass).
    #[inline]
    pub fn filters(&mut self, x: f64, ga: f64, f0: f64, g0: f64, sample_rate: f64) -> (f64, f64) {
        let (gh, gl) = if ga == 1.0 {
            (g0, g0)
        } else {
            let f0 = f0.clamp(200.0, 8000.0);
            let rg = ga.sqrt();
            (OnePoleTpt::coef(f0 * rg, sample_rate), OnePoleTpt::coef(f0 / rg, sample_rate))
        };
        (x - self.hi.low(x, gh), self.lo.low(x, gl))
    }

    /// The stage's output for `band` (0 Full, 1 Highs, 2 Lows); exactly x at ga = 1.
    #[inline]
    pub fn out(band: usize, ga: f64, x: f64, hp: f64, lp: f64) -> f64 {
        if ga == 1.0 {
            return x;
        }
        match band {
            1 => x + (ga - 1.0) * hp,
            2 => x + (ga - 1.0) * lp,
            _ => ga * x,
        }
    }
}

#[derive(Clone, Default)]
struct Channel {
    sc: ScFilter,
    delay: DelayLine,
    att: AttackStage,
}

pub struct Transient {
    v: [f32; NP],
    sr: f32,
    clock: u64,
    // control-rate parameters (20 ms)
    att_time: OnePole,
    sus_time: OnePole,
    sens: OnePole,
    smooth: OnePole,
    att_freq: OnePole,
    sc_hpf: OnePole,
    sc_lpf: OnePole,
    thresh: OnePole,
    link: OnePole,
    // EXP parameters: the contract's smoother, then 20 ms for host automation
    attack: ExpSmoother,
    attack_s: OnePole,
    sustain: ExpSmoother,
    sustain_s: OnePole,
    mix: ExpSmoother,
    mix_s: OnePole,
    level: ExpSmoother,
    level_s: OnePole,
    // control-rate coefficients
    k_att: f64,
    k_sus: f64,
    shelf_g0: f64,
    link_v: f32,
    // detection
    det: [Det; MAX_CH],
    key_silent: u64,
    // audio
    ch: [Channel; MAX_CH],
    band_cur: usize,
    band_prev: usize,
    band_fade: LinearRamp,
    la_cur: usize,
    la_prev: usize,
    la_fade: LinearRamp,
    listen_g: LinearRamp,
    delta_g: LinearRamp,
    bypass_g: LinearRamp,
    // ceiling
    limiter: Limiter,
    ceil_mode: usize,
    ceil_fade: LinearRamp,
    pre: [[f32; CHUNK]; MAX_CH],
    dry: [[f32; CHUNK]; MAX_CH],
    // meters, witness
    shaping: f32,
    gr: f32,
    last_g: [(f32, f32); MAX_CH],
}

/// `OnePole` fed from an upstream smoother each sample.
trait Chain {
    fn next_from(&mut self, target: f32) -> f32;
}

impl Chain for OnePole {
    #[inline]
    fn next_from(&mut self, target: f32) -> f32 {
        if target != self.target() {
            self.set_target(target);
        }
        self.next()
    }
}

impl Transient {
    pub fn new() -> Self {
        let v: [f32; NP] = core::array::from_fn(|i| PARAMS[i].default);
        let mut t = Self {
            v,
            sr: 48000.0,
            clock: 0,
            att_time: OnePole::new(20.0, v[p::ATT_TIME], 1e-4),
            sus_time: OnePole::new(20.0, v[p::SUS_TIME], 1e-3),
            sens: OnePole::new(20.0, v[p::SENS], 1e-4),
            smooth: OnePole::new(20.0, v[p::SMOOTH], 1e-6),
            att_freq: OnePole::new(20.0, v[p::ATT_FREQ], 1e-2),
            sc_hpf: OnePole::new(20.0, v[p::SC_HPF], 1e-3),
            sc_lpf: OnePole::new(20.0, v[p::SC_LPF], 1e-2),
            thresh: OnePole::new(20.0, v[p::THRESH], 1e-4),
            link: OnePole::new(20.0, v[p::LINK], 1e-4),
            attack: ExpSmoother::new(-24.0, 24.0, v[p::ATTACK]),
            attack_s: OnePole::new(20.0, v[p::ATTACK], 1e-5),
            sustain: ExpSmoother::new(-24.0, 24.0, v[p::SUSTAIN]),
            sustain_s: OnePole::new(20.0, v[p::SUSTAIN], 1e-5),
            mix: ExpSmoother::new(0.0, 100.0, v[p::MIX]),
            mix_s: OnePole::new(20.0, v[p::MIX], 1e-5),
            level: ExpSmoother::new(-24.0, 12.0, v[p::LEVEL]),
            level_s: OnePole::new(20.0, v[p::LEVEL], 1e-5),
            k_att: 0.0,
            k_sus: 0.0,
            shelf_g0: 0.0,
            link_v: 1.0,
            det: Default::default(),
            key_silent: 0,
            ch: Default::default(),
            band_cur: 0,
            band_prev: 0,
            band_fade: LinearRamp::new(20.0, 1.0),
            la_cur: 0,
            la_prev: 0,
            la_fade: LinearRamp::new(20.0, 1.0),
            listen_g: LinearRamp::new(5.0, 0.0),
            delta_g: LinearRamp::new(5.0, 0.0),
            bypass_g: LinearRamp::new(5.0, 1.0),
            limiter: Limiter::new(),
            ceil_mode: 0,
            ceil_fade: LinearRamp::new(20.0, 0.0),
            pre: [[0.0; CHUNK]; MAX_CH],
            dry: [[0.0; CHUNK]; MAX_CH],
            shaping: 0.0,
            gr: 0.0,
            last_g: [(0.0, 0.0); MAX_CH],
        };
        t.set_limiter();
        t
    }

    /// `shaping` meter: the attack + sustain gain applied (dB), the block's largest.
    pub fn shaping_db(&self) -> f32 {
        self.shaping
    }

    /// `gr` meter: the ceiling's reduction (dB, ≥ 0).
    pub fn gr_db(&self) -> f32 {
        self.gr
    }

    /// The (attack, sustain) gains in dB applied to channel `c` at the last sample (test witness).
    pub fn gains(&self, c: usize) -> (f32, f32) {
        self.last_g[c.min(MAX_CH - 1)]
    }

    fn la_samples(&self, i: usize) -> usize {
        (LOOKAHEAD_MS[i] * 0.001 * self.sr).round() as usize
    }

    /// Ramp length R = clamp(`smooth`, 0.25 ms, L), in samples.
    fn ramp_samples(&self, smooth: f32) -> usize {
        let r = (smooth.clamp(0.05, 5.0).max(0.25) * 0.001 * self.sr).round() as usize;
        r.min(self.la_cur)
    }

    fn configure_ramps(&mut self) {
        let la = self.la_cur;
        let r = self.ramp_samples(self.v[p::SMOOTH]);
        for d in self.det.iter_mut() {
            d.ramp_a.configure(la, r);
            d.ramp_s.configure(la, r);
        }
    }

    fn set_limiter(&mut self) {
        let style = if self.ceil_mode == 1 { Style::Clip } else { Style::Clean };
        self.limiter.set(gfx_limiter::Settings {
            threshold: self.v[p::CEILING].clamp(-24.0, 0.0),
            release: 50.0,
            gain: 0.0,
            style,
            lookahead: 0,
            true_peak: false,
            auto_release: true,
            clip: 50.0,
            link: self.v[p::LINK].clamp(0.0, 100.0),
        });
    }

    fn control(&mut self) {
        for s in [&mut self.att_time, &mut self.sus_time, &mut self.sens, &mut self.smooth, &mut self.att_freq, &mut self.sc_hpf, &mut self.sc_lpf, &mut self.thresh, &mut self.link] {
            for _ in 0..CTL {
                s.next();
            }
        }
        let sr = self.sr;
        let ds = DetSettings {
            att_time_ms: self.att_time.value().clamp(1.0, 100.0),
            sus_time_ms: self.sus_time.value().clamp(20.0, 3000.0),
            sens: (self.sens.value() * 0.01).clamp(0.0, 1.0),
            thresh_db: self.thresh.value().clamp(-96.0, -30.0),
        };
        let hpf = self.sc_hpf.value().clamp(0.0, 2000.0);
        let lpf = self.sc_lpf.value().clamp(500.0, 22000.0);
        let w = (window_secs(hpf, 2.0) * sr) as usize;
        for d in self.det.iter_mut() {
            d.det.set(&ds);
            d.max.set_window(w);
        }
        for c in self.ch.iter_mut() {
            c.sc.set(hpf, lpf, sr);
        }
        let sm = self.smooth.value().clamp(0.05, 5.0) as f64;
        self.k_att = one_pole_coef(sm * 0.001 * sr as f64);
        self.k_sus = one_pole_coef(sm.max(0.5) * 0.001 * sr as f64);
        self.shelf_g0 = OnePoleTpt::coef(self.att_freq.value().clamp(200.0, 8000.0) as f64, sr as f64);
        self.link_v = (self.link.value() * 0.01).clamp(0.0, 1.0);
    }

    pub fn process_with_key(&mut self, buffers: &mut [&mut [f32]], key: Option<&[&[f32]]>) {
        let nch = buffers.len().min(MAX_CH);
        let n = buffers.first().map_or(0, |b| b.len());
        if nch == 0 {
            return;
        }
        let v = self.v;
        // key fallback: a key silent for 1 s is not connected
        let mut key = if idx(v[p::SOURCE], 2) == 1 { key.filter(|k| !k.is_empty()) } else { None };
        if let Some(k) = key {
            if k.iter().any(|c| c.iter().any(|&s| s != 0.0)) {
                self.key_silent = 0;
            } else {
                self.key_silent += n as u64;
            }
            if self.key_silent >= self.sr as u64 {
                key = None;
            }
        }
        self.listen_g.set_target(if v[p::LISTEN] >= 0.5 { 1.0 } else { 0.0 });
        self.delta_g.set_target(if v[p::DELTA] >= 0.5 { 1.0 } else { 0.0 });
        self.bypass_g.set_target(if v[p::BYPASS] >= 0.5 { 0.0 } else { 1.0 });
        let ndet = nch;
        let lookahead = self.la_cur > 0;
        let mut shaping = 0.0f32;
        let mut shaping_abs = -1.0f32;

        let mut start = 0;
        while start < n {
            let len = (n - start).min(CHUNK);
            for j in 0..len {
                let i = start + j;
                if self.clock.is_multiple_of(CTL) {
                    self.control();
                }
                self.clock += 1;
                let a_db = self.attack_s.next_from(self.attack.next()) as f64;
                let s_db = self.sustain_s.next_from(self.sustain.next()) as f64;
                let mix = (self.mix_s.next_from(self.mix.next()) * 0.01).clamp(0.0, 1.0);
                let lvl_db = self.level_s.next_from(self.level.next()) as f64;

                // detection: the filtered sidechain, rectified, linked, through the sliding maximum
                let mut listen = [0.0f32; MAX_CH];
                let mut a = [0.0f32; MAX_CH];
                for c in 0..nch {
                    let src = match key {
                        Some(k) => k[c.min(k.len() - 1)].get(i).copied().unwrap_or(0.0),
                        None => buffers[c][i],
                    };
                    let src = if src.is_finite() { src } else { 0.0 };
                    listen[c] = self.ch[c].sc.process(src);
                    a[c] = listen[c].abs();
                }
                if nch == 2 {
                    let m = a[0].max(a[1]);
                    let l = self.link_v;
                    if l >= 1.0 {
                        a = [m, m];
                    } else {
                        for c in 0..2 {
                            a[c] = l * m + (1.0 - l) * a[c];
                        }
                    }
                }
                let mut g = [(0.0f64, 0.0f64); MAX_CH];
                for d in 0..ndet {
                    let det = &mut self.det[d];
                    let r = det.det.next(det.max.next(a[d]));
                    let ta = r.share_att;
                    // the floor bound of a boost: S · share ≤ the tail's height over F (dB)
                    let ts = if s_db > 0.0 && r.share_sus > 0.0 { r.share_sus.min(r.height_db() / s_db) } else { r.share_sus };
                    if lookahead {
                        det.sh_a = det.ramp_a.next(ta);
                        det.sh_s = det.ramp_s.next(ts);
                    } else {
                        // attack: rises through `smooth`, falls as they come (they follow E_s);
                        // sustain: max(`smooth`, 0.5 ms) both ways
                        det.sh_a = if ta < det.sh_a { ta } else { ta + (det.sh_a - ta) * self.k_att };
                        det.sh_s = ts + (det.sh_s - ts) * self.k_sus;
                        if det.sh_a < 1e-12 {
                            det.sh_a = 0.0;
                        }
                        if det.sh_s < 1e-12 {
                            det.sh_s = 0.0;
                        }
                    }
                    g[d] = (a_db * det.sh_a, s_db * det.sh_s);
                }

                // audio
                let xf = self.la_fade.next();
                let bf = self.band_fade.next();
                let lg = self.listen_g.next();
                let dg = self.delta_g.next();
                for c in 0..nch {
                    let (ga_db, gs_db) = g[c.min(ndet - 1)];
                    let total = (ga_db + gs_db) as f32;
                    if total.abs() > shaping_abs {
                        shaping_abs = total.abs();
                        shaping = total;
                    }
                    self.last_g[c] = (ga_db as f32, gs_db as f32);
                    let ch = &mut self.ch[c];
                    let x = buffers[c][i];
                    ch.delay.push(if x.is_finite() { x } else { 0.0 });
                    let xd = if xf >= 1.0 { ch.delay.tap(self.la_cur) } else { ch.delay.tap(self.la_prev) + (ch.delay.tap(self.la_cur) - ch.delay.tap(self.la_prev)) * xf };
                    // attack stage: the shelves run every sample (their cut-off moves with the gain)
                    let xv = xd as f64;
                    let ga = if ga_db == 0.0 { 1.0 } else { db_to_lin(ga_db) };
                    let (hp, lp) = ch.att.filters(xv, ga, self.att_freq.value() as f64, self.shelf_g0, self.sr as f64);
                    let stage = |band: usize| AttackStage::out(band, ga, xv, hp, lp);
                    let y = if bf >= 1.0 {
                        stage(self.band_cur)
                    } else {
                        let o = stage(self.band_prev);
                        o + (stage(self.band_cur) - o) * bf as f64
                    };
                    // sustain gain and level, full band
                    let post = gs_db + lvl_db;
                    let wet = if post == 0.0 { y as f32 } else { (y * db_to_lin(post)) as f32 };
                    let mut out = if mix >= 1.0 { wet } else { xd + mix * (wet - xd) };
                    if dg > 0.0 {
                        // what the shaper adds or removes
                        let delta = out - xd;
                        out = if dg >= 1.0 { delta } else { out + (delta - out) * dg };
                    }
                    if lg > 0.0 {
                        out = if lg >= 1.0 { listen[c] } else { out + (listen[c] - out) * lg };
                    }
                    buffers[c][i] = out;
                    self.dry[c][j] = xd;
                }
            }

            // the ceiling, last
            let cf_active = self.ceil_mode != 0 || !self.ceil_fade.is_settled() || self.ceil_fade.value() > 0.0;
            if cf_active {
                for c in 0..nch {
                    self.pre[c][..len].copy_from_slice(&buffers[c][start..start + len]);
                }
                match nch {
                    1 => {
                        let mut s = [&mut buffers[0][start..start + len]];
                        self.limiter.process_block(&mut s);
                    }
                    _ => {
                        let (l, r) = buffers.split_at_mut(1);
                        let mut s = [&mut l[0][start..start + len], &mut r[0][start..start + len]];
                        self.limiter.process_block(&mut s);
                    }
                }
                for j in 0..len {
                    let f = self.ceil_fade.next();
                    if f < 1.0 {
                        for c in 0..nch {
                            let pre = self.pre[c][j];
                            buffers[c][start + j] = pre + (buffers[c][start + j] - pre) * f;
                        }
                    }
                }
                self.gr = self.limiter.gain_reduction_db();
            } else {
                self.gr = 0.0;
            }

            // bypass: the delayed dry (the latency stays)
            for j in 0..len {
                let b = self.bypass_g.next();
                if b < 1.0 {
                    for c in 0..nch {
                        let d = self.dry[c][j];
                        buffers[c][start + j] = d + (buffers[c][start + j] - d) * b;
                    }
                }
            }
            start += len;
        }
        self.shaping = shaping.clamp(-48.0, 48.0);
    }
}

impl Default for Transient {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Transient {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        for s in [&mut self.att_time, &mut self.sus_time, &mut self.sens, &mut self.smooth, &mut self.att_freq, &mut self.sc_hpf, &mut self.sc_lpf, &mut self.thresh, &mut self.link] {
            s.prepare(sample_rate);
        }
        for (e, s) in [(&mut self.attack, &mut self.attack_s), (&mut self.sustain, &mut self.sustain_s), (&mut self.mix, &mut self.mix_s), (&mut self.level, &mut self.level_s)] {
            e.prepare(sample_rate);
            s.prepare(sample_rate);
        }
        let max_la = (LOOKAHEAD_MS[3] * 0.001 * sample_rate).ceil() as usize + 2;
        let max_w = (MAX_WINDOW_S * sample_rate) as usize + 2;
        for d in self.det.iter_mut() {
            d.max.prepare(max_w);
            d.det.prepare(sample_rate);
            d.ramp_a.prepare(max_la);
            d.ramp_s.prepare(max_la);
        }
        for c in self.ch.iter_mut() {
            c.delay.prepare(max_la);
        }
        for r in [&mut self.band_fade, &mut self.la_fade, &mut self.listen_g, &mut self.delta_g, &mut self.bypass_g, &mut self.ceil_fade] {
            r.prepare(sample_rate);
        }
        self.limiter.prepare(sample_rate, max_block, MAX_CH);
        self.la_cur = self.la_samples(idx(self.v[p::LOOKAHEAD], 4));
        self.la_prev = self.la_cur;
        self.configure_ramps();
    }

    fn reset(&mut self) {
        let v = self.v;
        for (s, i) in [
            (&mut self.att_time, p::ATT_TIME),
            (&mut self.sus_time, p::SUS_TIME),
            (&mut self.sens, p::SENS),
            (&mut self.smooth, p::SMOOTH),
            (&mut self.att_freq, p::ATT_FREQ),
            (&mut self.sc_hpf, p::SC_HPF),
            (&mut self.sc_lpf, p::SC_LPF),
            (&mut self.thresh, p::THRESH),
            (&mut self.link, p::LINK),
        ] {
            s.set_immediate(v[i]);
        }
        for (e, s, i) in [
            (&mut self.attack, &mut self.attack_s, p::ATTACK),
            (&mut self.sustain, &mut self.sustain_s, p::SUSTAIN),
            (&mut self.mix, &mut self.mix_s, p::MIX),
            (&mut self.level, &mut self.level_s, p::LEVEL),
        ] {
            let x = v[i].clamp(PARAMS[i].min, PARAMS[i].max);
            e.settle(x);
            e.reset();
            s.set_immediate(x);
        }
        for d in self.det.iter_mut() {
            d.reset();
        }
        for c in self.ch.iter_mut() {
            c.sc.reset();
            c.delay.reset();
            c.att.reset();
        }
        self.key_silent = 0;
        self.band_cur = idx(v[p::ATT_BAND], 3);
        self.band_prev = self.band_cur;
        self.band_fade.set_immediate(1.0);
        self.la_cur = self.la_samples(idx(v[p::LOOKAHEAD], 4));
        self.la_prev = self.la_cur;
        self.la_fade.set_immediate(1.0);
        self.configure_ramps();
        self.listen_g.set_immediate(if v[p::LISTEN] >= 0.5 { 1.0 } else { 0.0 });
        self.delta_g.set_immediate(if v[p::DELTA] >= 0.5 { 1.0 } else { 0.0 });
        self.bypass_g.set_immediate(if v[p::BYPASS] >= 0.5 { 0.0 } else { 1.0 });
        self.ceil_mode = idx(v[p::CEIL_MODE], 3);
        self.ceil_fade.set_immediate(if self.ceil_mode != 0 { 1.0 } else { 0.0 });
        self.set_limiter();
        self.limiter.reset();
        self.clock = 0;
        self.shaping = 0.0;
        self.gr = 0.0;
        self.last_g = [(0.0, 0.0); MAX_CH];
        self.control();
    }

    fn set_param(&mut self, id: &str, value: f32) {
        let Some(i) = PARAMS.iter().position(|pi| pi.id == id) else { return };
        let old = self.v[i];
        self.v[i] = value;
        let pi = &PARAMS[i];
        let x = value.clamp(pi.min, pi.max);
        match i {
            p::ATTACK if value != old => self.attack.set_target(x, self.clock),
            p::SUSTAIN if value != old => self.sustain.set_target(x, self.clock),
            p::MIX if value != old => self.mix.set_target(x, self.clock),
            p::LEVEL if value != old => self.level.set_target(x, self.clock),
            p::ATT_TIME => self.att_time.set_target(x),
            p::SUS_TIME => self.sus_time.set_target(x),
            p::SENS => self.sens.set_target(x),
            p::SMOOTH => {
                self.smooth.set_target(x);
                // the lookahead ramp is a setup: it restarts only when its length in samples changes
                if self.la_cur > 0 && self.ramp_samples(x) != self.ramp_samples(old) {
                    self.configure_ramps();
                }
            }
            p::ATT_FREQ => self.att_freq.set_target(x),
            p::SC_HPF => self.sc_hpf.set_target(x),
            p::SC_LPF => self.sc_lpf.set_target(x),
            p::THRESH => self.thresh.set_target(x),
            p::LINK => {
                self.link.set_target(x);
                if value != old {
                    self.set_limiter();
                }
            }
            p::CEILING if value != old => self.set_limiter(),
            p::ATT_BAND => {
                let b = idx(value, 3);
                if b != self.band_cur {
                    // the structure in use when the change comes (nearer end of a running fade)
                    self.band_prev = if self.band_fade.is_settled() || self.band_fade.value() >= 0.5 { self.band_cur } else { self.band_prev };
                    self.band_cur = b;
                    self.band_fade.set_immediate(0.0);
                    self.band_fade.set_target(1.0);
                }
            }
            p::CEIL_MODE => {
                let m = idx(value, 3);
                if m != self.ceil_mode {
                    if self.ceil_mode == 0 && self.ceil_fade.value() == 0.0 {
                        // engaging from a settled Off: the limiter starts from a clean state
                        self.ceil_mode = m;
                        self.set_limiter();
                        self.limiter.reset();
                    } else {
                        self.ceil_mode = m;
                        if m != 0 {
                            // Soft ↔ Limit: gfx-limiter crossfades its styles itself (10 ms)
                            self.set_limiter();
                        }
                    }
                    self.ceil_fade.set_target(if m != 0 { 1.0 } else { 0.0 });
                }
            }
            p::LOOKAHEAD => {
                let la = self.la_samples(idx(value, 4));
                if la != self.la_cur {
                    // a setup change: the host re-aligns; the audio crossfades over 20 ms
                    self.la_prev = if self.la_fade.is_settled() || self.la_fade.value() >= 0.5 { self.la_cur } else { self.la_prev };
                    self.la_cur = la;
                    self.la_fade.set_immediate(0.0);
                    self.la_fade.set_target(1.0);
                    self.configure_ramps();
                }
            }
            _ => {}
        }
    }

    fn latency(&self) -> usize {
        self.la_cur
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_with_key(buffers, None);
    }

    /// The key bus (gfx-bench drives it through the trait): the inherent `process_with_key`.
    fn process_with_key(&mut self, buffers: &mut [&mut [f32]], key: Option<&[&[f32]]>) {
        Transient::process_with_key(self, buffers, key);
    }
}
